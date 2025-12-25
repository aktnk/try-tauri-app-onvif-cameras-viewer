use tauri::State;
use crate::models::{Camera, NewCamera, Recording, DiscoveredDevice, PTZCapabilities, PTZMovement, PTZResult, CameraTimeInfo, TimeSyncResult, CameraCapabilities, EncoderSettings, UpdateEncoderSettings, RecordingSchedule, NewRecordingSchedule, UpdateRecordingSchedule};
use crate::AppState;
use crate::gpu_detector::{detect_gpu_capabilities, GpuCapabilities};
use rusqlite::Connection;
use chrono::{Utc, DateTime};
use tokio_cron_scheduler::Job;
use chrono_tz::Asia::Tokyo;
use std::sync::Arc;

fn get_conn(state: &State<AppState>) -> Result<Connection, String> {
    Connection::open(&state.db_path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_cameras(state: State<'_, AppState>) -> Result<Vec<Camera>, String> {
    let conn = get_conn(&state)?;
    let mut stmt = conn.prepare("SELECT id, name, type, host, port, user, pass, xaddr, stream_path, created_at, updated_at FROM cameras").map_err(|e| e.to_string())?;
    
    let cameras_iter = stmt.query_map([], |row| {
        Ok(Camera {
            id: row.get(0)?,
            name: row.get(1)?,
            camera_type: row.get(2)?,
            host: row.get(3)?,
            port: row.get(4)?,
            user: row.get(5)?,
            pass: row.get(6)?,
            xaddr: row.get(7)?,
            stream_path: row.get(8)?,
            created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(9)?).unwrap_or(Utc::now().into()).with_timezone(&Utc),
            updated_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(10)?).unwrap_or(Utc::now().into()).with_timezone(&Utc),
        })
    }).map_err(|e| e.to_string())?;

    let mut cameras = Vec::new();
    for camera in cameras_iter {
        cameras.push(camera.map_err(|e| e.to_string())?);
    }
    Ok(cameras)
}

#[tauri::command]
pub async fn add_camera(state: State<'_, AppState>, camera: NewCamera) -> Result<Camera, String> {
    let conn = get_conn(&state)?;
    conn.execute(
        "INSERT INTO cameras (name, type, host, port, user, pass, xaddr, stream_path, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        (
            &camera.name,
            &camera.camera_type,
            &camera.host,
            &camera.port,
            &camera.user,
            &camera.pass,
            &camera.xaddr,
            &camera.stream_path,
            Utc::now().to_rfc3339(),
            Utc::now().to_rfc3339(),
        ),
    ).map_err(|e| e.to_string())?;
    
    let id = conn.last_insert_rowid() as i32;
    
    // Return the created camera (fetch it back or construct it)
    // Constructing is faster
    Ok(Camera {
        id,
        name: camera.name,
        camera_type: camera.camera_type,
        host: camera.host,
        port: camera.port,
        user: camera.user,
        pass: camera.pass,
        xaddr: camera.xaddr,
        stream_path: camera.stream_path,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    })
}

#[tauri::command]
pub async fn delete_camera(state: State<'_, AppState>, id: i32) -> Result<(), String> {
    let conn = get_conn(&state)?;
    conn.execute("DELETE FROM cameras WHERE id = ?1", [id]).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn discover_cameras() -> Result<Vec<DiscoveredDevice>, String> {
    // TODO: Implement ONVIF discovery
    crate::onvif::discover_devices().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn start_stream(state: State<'_, AppState>, id: i32) -> Result<serde_json::Value, String> {
    // Get camera details
    let cameras = get_cameras(state.clone()).await?;
    let camera = cameras.into_iter().find(|c| c.id == id).ok_or("Camera not found")?;
    
    // Start FFmpeg process via stream module
    match crate::stream::start_stream(state.clone(), camera).await {
        Ok(stream_path_relative) => {
            let port = state.server_port;
            Ok(serde_json::json!({ "streamUrl": format!("http://localhost:{}/{}", port, stream_path_relative) }))
        },
        Err(e) => {
            eprintln!("[Error] Failed to start stream for camera {}: {}", id, e);
            Err(e)
        }
    }
}

#[tauri::command]
pub async fn stop_stream(state: State<'_, AppState>, id: i32) -> Result<serde_json::Value, String> {
    crate::stream::stop_stream(state, id).await.map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "success": true }))
}

#[tauri::command]
pub async fn start_recording(state: State<'_, AppState>, id: i32) -> Result<serde_json::Value, String> {
    let cameras = get_cameras(state.clone()).await?;
    let camera = cameras.into_iter().find(|c| c.id == id).ok_or("Camera not found")?;

    crate::stream::start_recording(state, camera).await.map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "success": true }))
}

#[tauri::command]
pub async fn stop_recording(state: State<'_, AppState>, id: i32) -> Result<serde_json::Value, String> {
    crate::stream::stop_recording(state, id).await.map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "success": true }))
}

#[tauri::command]
pub async fn get_recordings(state: State<'_, AppState>) -> Result<Vec<Recording>, String> {
    let conn = get_conn(&state)?;
    let mut stmt = conn.prepare(
        "SELECT r.id, r.camera_id, r.filename, r.thumbnail, r.start_time, r.end_time, r.is_finished, c.name 
         FROM recordings r 
         LEFT JOIN cameras c ON r.camera_id = c.id 
         ORDER BY r.start_time DESC"
    ).map_err(|e| e.to_string())?;
    
    let recordings_iter = stmt.query_map([], |row| {
        Ok(Recording {
            id: row.get(0)?,
            camera_id: row.get(1)?,
            filename: row.get(2)?,
            thumbnail: row.get(3)?,
            start_time: DateTime::parse_from_rfc3339(&row.get::<_, String>(4)?).unwrap_or(Utc::now().into()).with_timezone(&Utc),
            end_time: row.get::<_, Option<String>>(5)?.map(|t| DateTime::parse_from_rfc3339(&t).unwrap_or(Utc::now().into()).with_timezone(&Utc)),
            is_finished: row.get(6)?,
            camera_name: row.get(7)?,
        })
    }).map_err(|e| e.to_string())?;

    let mut recordings = Vec::new();
    for r in recordings_iter {
        recordings.push(r.map_err(|e| e.to_string())?);
    }
    Ok(recordings)
}

#[tauri::command]
pub async fn delete_recording(state: State<'_, AppState>, id: i32) -> Result<(), String> {
    let conn = get_conn(&state)?;
    
    // Get filename to delete
    let filename: String = conn.query_row(
        "SELECT filename FROM recordings WHERE id = ?1",
        [id],
        |row| row.get(0)
    ).map_err(|e| e.to_string())?;

    // Delete file from filesystem
    let file_path = state.recording_dir.join(&filename);
    if file_path.exists() {
        std::fs::remove_file(file_path).map_err(|e| e.to_string())?;
    }

    conn.execute("DELETE FROM recordings WHERE id = ?1", [id]).map_err(|e| e.to_string())?;
    Ok(())
}

// Time synchronization commands
#[tauri::command]
pub async fn get_camera_time(state: State<'_, AppState>, id: i32) -> Result<CameraTimeInfo, String> {
    let cameras = get_cameras(state.clone()).await?;
    let camera = cameras.into_iter().find(|c| c.id == id).ok_or("Camera not found")?;

    if camera.camera_type != "onvif" {
        return Err("Time synchronization is only supported for ONVIF cameras".to_string());
    }

    let camera_datetime = crate::onvif::get_system_date_time(&camera).await?;
    let server_time = Utc::now();

    Ok(CameraTimeInfo {
        cameraTime: serde_json::json!({
            "year": camera_datetime.year,
            "month": camera_datetime.month,
            "day": camera_datetime.day,
            "hour": camera_datetime.hour,
            "minute": camera_datetime.minute,
            "second": camera_datetime.second,
        }),
        serverTime: server_time.to_rfc3339(),
    })
}

#[tauri::command]
pub async fn sync_camera_time(state: State<'_, AppState>, id: i32) -> Result<TimeSyncResult, String> {
    let cameras = get_cameras(state.clone()).await?;
    let camera = cameras.into_iter().find(|c| c.id == id).ok_or("Camera not found")?;

    if camera.camera_type != "onvif" {
        return Err("Time synchronization is only supported for ONVIF cameras".to_string());
    }

    // Check if streaming is currently active
    let was_streaming = {
        let processes = state.processes.lock().map_err(|e| e.to_string())?;
        processes.contains_key(&id)
    };

    // Get current camera time before sync
    let before_datetime = crate::onvif::get_system_date_time(&camera).await?;

    // Get server time
    let server_time = Utc::now();

    // Convert server time to ONVIF format
    let new_datetime = crate::onvif::ONVIFDateTime::from_chrono(&server_time);

    // Set camera time
    crate::onvif::set_system_date_time(&camera, &new_datetime).await?;

    // Wait a moment for the camera to process the time change
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Verify by reading the time again
    let after_datetime = match crate::onvif::get_system_date_time(&camera).await {
        Ok(dt) => Some(dt),
        Err(e) => {
            println!("[TimeSync] Warning: Could not verify time after sync: {}", e);
            None
        }
    };

    // Restart streaming if it was active before time sync
    if was_streaming {
        println!("[TimeSync] Restarting stream for camera {} after time sync", id);

        // Stop current stream
        if let Err(e) = crate::stream::stop_stream(state.clone(), id).await {
            println!("[TimeSync] Warning: Failed to stop stream: {}", e);
        }

        // Wait for cleanup
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // Restart stream
        if let Err(e) = crate::stream::start_stream(state.clone(), camera.clone()).await {
            println!("[TimeSync] Warning: Failed to restart stream: {}", e);
        } else {
            println!("[TimeSync] Stream restarted successfully for camera {}", id);
        }
    }

    // Calculate time difference
    let before_chrono = before_datetime.to_chrono().ok_or("Invalid camera time format")?;
    let time_diff = server_time.signed_duration_since(before_chrono);
    let diff_seconds = time_diff.num_seconds();

    // Check if verification shows the time was actually set
    let message = if let Some(after_dt) = after_datetime {
        let after_chrono = after_dt.to_chrono().ok_or("Invalid camera time format")?;
        let final_diff = Utc::now().signed_duration_since(after_chrono).num_seconds();

        if final_diff.abs() < 5 {
            format!("Camera time synchronized successfully (adjusted by {}s, verified)", diff_seconds)
        } else {
            format!("Camera time may not have been set correctly (before diff: {}s, after diff: {}s)", diff_seconds, final_diff)
        }
    } else if diff_seconds.abs() < 2 {
        format!("Camera time is already synchronized (difference: {}s)", diff_seconds)
    } else {
        format!("Camera time command sent (adjusted by {}s, verification unavailable)", diff_seconds)
    };

    println!("[TimeSync] Camera {} - {}", id, message);

    Ok(TimeSyncResult {
        success: true,
        beforeTime: serde_json::json!({
            "year": before_datetime.year,
            "month": before_datetime.month,
            "day": before_datetime.day,
            "hour": before_datetime.hour,
            "minute": before_datetime.minute,
            "second": before_datetime.second,
        }),
        serverTime: server_time.to_rfc3339(),
        message,
        error: None,
    })
}

#[tauri::command]
pub async fn check_ptz_capabilities(state: State<'_, AppState>, id: i32) -> Result<PTZCapabilities, String> {
    let cameras = get_cameras(state.clone()).await?;
    let camera = cameras.into_iter().find(|c| c.id == id).ok_or("Camera not found")?;

    if camera.camera_type != "onvif" {
        return Ok(PTZCapabilities { supported: false, capabilities: None });
    }

    match crate::onvif::get_ptz_service_url(&camera).await {
        Ok(_) => Ok(PTZCapabilities { 
            supported: true, 
            capabilities: Some(crate::models::PTZCapabilitiesDetails { hasPanTilt: true, hasZoom: true }) 
        }),
        Err(_) => Ok(PTZCapabilities { supported: false, capabilities: None })
    }
}

#[tauri::command]
pub async fn move_ptz(state: State<'_, AppState>, id: i32, movement: PTZMovement) -> Result<PTZResult, String> {
    let cameras = get_cameras(state.clone()).await?;
    let camera = cameras.into_iter().find(|c| c.id == id).ok_or("Camera not found")?;

    if camera.camera_type != "onvif" {
        return Err("Not an ONVIF camera".to_string());
    }

    let x = movement.x.unwrap_or(0.0);
    let y = movement.y.unwrap_or(0.0);
    let zoom = movement.zoom.unwrap_or(0.0);

    crate::onvif::continuous_move(&camera, x, y, zoom).await?;
    Ok(PTZResult { success: true, message: "Moving".to_string() })
}

#[tauri::command]
pub async fn stop_ptz(state: State<'_, AppState>, id: i32) -> Result<PTZResult, String> {
    let cameras = get_cameras(state.clone()).await?;
    let camera = cameras.into_iter().find(|c| c.id == id).ok_or("Camera not found")?;

    if camera.camera_type != "onvif" {
         return Err("Not an ONVIF camera".to_string());
    }

    crate::onvif::stop_move(&camera).await?;
    Ok(PTZResult { success: true, message: "Stopped".to_string() })
}

#[tauri::command]
pub async fn get_camera_capabilities(_id: i32) -> Result<CameraCapabilities, String> {
     Ok(CameraCapabilities {
        streaming: true,
        recording: true,
        thumbnails: false,
        ptz: true, // Optimistically true, or check dynamically
        discovery: false,
        timeSync: false,
        remoteAccess: false,
    })
}

// ============= GPU & Encoder Commands =============

#[tauri::command]
pub async fn detect_gpu() -> Result<GpuCapabilities, String> {
    println!("[GPU] Detecting GPU capabilities...");
    detect_gpu_capabilities().await
}

#[tauri::command]
pub async fn get_encoder_settings(state: State<'_, AppState>) -> Result<EncoderSettings, String> {
    let conn = get_conn(&state)?;

    let mut stmt = conn.prepare(
        "SELECT id, encoder_mode, gpu_encoder, cpu_encoder, preset, quality FROM encoder_settings WHERE id = 1"
    ).map_err(|e| e.to_string())?;

    let settings = stmt.query_row([], |row| {
        Ok(EncoderSettings {
            id: row.get(0)?,
            encoderMode: row.get(1)?,
            gpuEncoder: row.get(2)?,
            cpuEncoder: row.get(3)?,
            preset: row.get(4)?,
            quality: row.get(5)?,
        })
    }).map_err(|e| e.to_string())?;

    Ok(settings)
}

#[tauri::command]
pub async fn update_encoder_settings(
    state: State<'_, AppState>,
    settings: UpdateEncoderSettings,
) -> Result<EncoderSettings, String> {
    let conn = get_conn(&state)?;

    // Use separate UPDATE statements for each field
    if let Some(mode) = &settings.encoderMode {
        conn.execute("UPDATE encoder_settings SET encoder_mode = ?1 WHERE id = 1", [mode])
            .map_err(|e| e.to_string())?;
    }
    if let Some(gpu_enc) = &settings.gpuEncoder {
        conn.execute("UPDATE encoder_settings SET gpu_encoder = ?1 WHERE id = 1", [gpu_enc])
            .map_err(|e| e.to_string())?;
    }
    if let Some(cpu_enc) = &settings.cpuEncoder {
        conn.execute("UPDATE encoder_settings SET cpu_encoder = ?1 WHERE id = 1", [cpu_enc])
            .map_err(|e| e.to_string())?;
    }
    if let Some(p) = &settings.preset {
        conn.execute("UPDATE encoder_settings SET preset = ?1 WHERE id = 1", [p])
            .map_err(|e| e.to_string())?;
    }
    if let Some(q) = settings.quality {
        conn.execute("UPDATE encoder_settings SET quality = ?1 WHERE id = 1", [q])
            .map_err(|e| e.to_string())?;
    }

    if settings.encoderMode.is_none()
        && settings.gpuEncoder.is_none()
        && settings.cpuEncoder.is_none()
        && settings.preset.is_none()
        && settings.quality.is_none() {
        return Err("No fields to update".to_string());
    }

    // Drop connection before await
    drop(conn);

    // Return updated settings
    get_encoder_settings(state).await
}

// ========== Recording Schedule Commands ==========

fn validate_cron_expression(expr: &str) -> Result<String, String> {
    // Convert 5-field cron (minute hour day month dow) to 6-field (second minute hour day month dow)
    let normalized_expr = if expr.split_whitespace().count() == 5 {
        format!("0 {}", expr) // Add "0" seconds at the beginning
    } else {
        expr.to_string()
    };

    // Validate using the same parser as the scheduler (tokio-cron-scheduler with Tokyo timezone)
    Job::new_async_tz(normalized_expr.as_str(), Tokyo, |_uuid, _lock| {
        Box::pin(async move {
            // Validation only - this job is never executed
        })
    })
    .map(|_| normalized_expr)
    .map_err(|e| format!("Invalid cron expression: {}", e))
}

#[tauri::command]
pub async fn get_recording_schedules(
    state: State<'_, AppState>
) -> Result<Vec<RecordingSchedule>, String> {
    let conn = get_conn(&state)?;

    let mut stmt = conn.prepare(
        "SELECT s.id, s.camera_id, s.name, s.cron_expression, s.duration_minutes, s.fps, s.is_enabled,
                s.created_at, s.updated_at, c.name as camera_name
         FROM recording_schedules s
         LEFT JOIN cameras c ON s.camera_id = c.id
         ORDER BY s.created_at DESC"
    ).map_err(|e| e.to_string())?;

    let schedules_iter = stmt.query_map([], |row| {
        Ok(RecordingSchedule {
            id: row.get(0)?,
            camera_id: row.get(1)?,
            name: row.get(2)?,
            cron_expression: row.get(3)?,
            duration_minutes: row.get(4)?,
            fps: row.get(5)?,
            is_enabled: row.get(6)?,
            created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(7)?).unwrap_or(Utc::now().into()).with_timezone(&Utc),
            updated_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(8)?).unwrap_or(Utc::now().into()).with_timezone(&Utc),
            camera_name: row.get(9)?,
        })
    }).map_err(|e| e.to_string())?;

    let mut schedules = Vec::new();
    for schedule in schedules_iter {
        schedules.push(schedule.map_err(|e| e.to_string())?);
    }

    Ok(schedules)
}

#[tauri::command]
pub async fn add_recording_schedule(
    state: State<'_, AppState>,
    schedule: NewRecordingSchedule
) -> Result<RecordingSchedule, String> {
    // Validate and normalize cron expression (5-field -> 6-field)
    let normalized_cron = validate_cron_expression(&schedule.cron_expression)?;

    let conn = get_conn(&state)?;

    conn.execute(
        "INSERT INTO recording_schedules (camera_id, name, cron_expression, duration_minutes, fps, is_enabled)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        (
            &schedule.camera_id,
            &schedule.name,
            &normalized_cron,
            &schedule.duration_minutes,
            &schedule.fps,
            &schedule.is_enabled,
        ),
    ).map_err(|e| e.to_string())?;

    let id = conn.last_insert_rowid() as i32;

    // Get the created schedule
    let created_schedule = {
        let mut stmt = conn.prepare(
            "SELECT s.id, s.camera_id, s.name, s.cron_expression, s.duration_minutes, s.fps, s.is_enabled,
                    s.created_at, s.updated_at, c.name as camera_name
             FROM recording_schedules s
             LEFT JOIN cameras c ON s.camera_id = c.id
             WHERE s.id = ?1"
        ).map_err(|e| e.to_string())?;

        stmt.query_row([id], |row| {
            Ok(RecordingSchedule {
                id: row.get(0)?,
                camera_id: row.get(1)?,
                name: row.get(2)?,
                cron_expression: row.get(3)?,
                duration_minutes: row.get(4)?,
                fps: row.get(5)?,
                is_enabled: row.get(6)?,
                created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(7)?).unwrap_or(Utc::now().into()).with_timezone(&Utc),
                updated_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(8)?).unwrap_or(Utc::now().into()).with_timezone(&Utc),
                camera_name: row.get(9)?,
            })
        }).map_err(|e| e.to_string())?
    };

    // Drop connection before async operations
    drop(conn);

    // Add to scheduler if enabled
    if created_schedule.is_enabled {
        let state_arc = Arc::new(AppState {
            db_path: state.db_path.clone(),
            server_port: state.server_port,
            stream_dir: state.stream_dir.clone(),
            recording_dir: state.recording_dir.clone(),
            processes: state.processes.clone(),
            recording_processes: state.recording_processes.clone(),
            scheduler: state.scheduler.clone(),
            active_scheduled_recordings: state.active_scheduled_recordings.clone(),
        });

        let scheduler = state.scheduler.lock().await;
        scheduler.add_schedule(created_schedule.clone(), state_arc).await?;
    }

    println!("[Schedule] Created schedule '{}' (ID: {})", created_schedule.name, created_schedule.id);

    Ok(created_schedule)
}

#[tauri::command]
pub async fn update_recording_schedule(
    state: State<'_, AppState>,
    id: i32,
    updates: UpdateRecordingSchedule
) -> Result<RecordingSchedule, String> {
    // Validate and normalize cron expression if provided
    let normalized_cron = if let Some(ref expr) = updates.cron_expression {
        Some(validate_cron_expression(expr)?)
    } else {
        None
    };

    let conn = get_conn(&state)?;

    // Check if schedule exists and get current state
    let old_enabled: bool = conn.query_row(
        "SELECT is_enabled FROM recording_schedules WHERE id = ?1",
        [id],
        |row| row.get(0)
    ).map_err(|e| format!("Schedule not found: {}", e))?;

    // Build dynamic UPDATE query
    {
        let mut set_clauses = Vec::new();
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(ref name) = updates.name {
            set_clauses.push("name = ?");
            params.push(Box::new(name.clone()));
        }
        if let Some(ref cron_expr) = normalized_cron {
            set_clauses.push("cron_expression = ?");
            params.push(Box::new(cron_expr.clone()));
        }
        if let Some(duration) = updates.duration_minutes {
            set_clauses.push("duration_minutes = ?");
            params.push(Box::new(duration));
        }
        if let Some(fps) = updates.fps {
            set_clauses.push("fps = ?");
            params.push(Box::new(fps));
        }
        if let Some(enabled) = updates.is_enabled {
            set_clauses.push("is_enabled = ?");
            params.push(Box::new(enabled));
        }

        // Always update updated_at
        set_clauses.push("updated_at = ?");
        params.push(Box::new(Utc::now().to_rfc3339()));

        // Add id as the last parameter for WHERE clause
        params.push(Box::new(id));

        // Execute single UPDATE if there are fields to update
        if !set_clauses.is_empty() {
            let sql = format!(
                "UPDATE recording_schedules SET {} WHERE id = ?",
                set_clauses.join(", ")
            );

            let params_ref: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
            conn.execute(&sql, params_ref.as_slice())
                .map_err(|e| e.to_string())?;
        }
    } // params is dropped here before any .await

    // Get updated schedule
    let updated_schedule = {
        let mut stmt = conn.prepare(
            "SELECT s.id, s.camera_id, s.name, s.cron_expression, s.duration_minutes, s.fps, s.is_enabled,
                    s.created_at, s.updated_at, c.name as camera_name
             FROM recording_schedules s
             LEFT JOIN cameras c ON s.camera_id = c.id
             WHERE s.id = ?1"
        ).map_err(|e| e.to_string())?;

        stmt.query_row([id], |row| {
            Ok(RecordingSchedule {
                id: row.get(0)?,
                camera_id: row.get(1)?,
                name: row.get(2)?,
                cron_expression: row.get(3)?,
                duration_minutes: row.get(4)?,
                fps: row.get(5)?,
                is_enabled: row.get(6)?,
                created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(7)?).unwrap_or(Utc::now().into()).with_timezone(&Utc),
                updated_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(8)?).unwrap_or(Utc::now().into()).with_timezone(&Utc),
                camera_name: row.get(9)?,
            })
        }).map_err(|e| e.to_string())?
    };

    // Drop connection before async operations
    drop(conn);

    // Handle scheduler updates
    if updates.is_enabled.is_some() || updates.cron_expression.is_some() || updates.duration_minutes.is_some() {
        let state_arc = Arc::new(AppState {
            db_path: state.db_path.clone(),
            server_port: state.server_port,
            stream_dir: state.stream_dir.clone(),
            recording_dir: state.recording_dir.clone(),
            processes: state.processes.clone(),
            recording_processes: state.recording_processes.clone(),
            scheduler: state.scheduler.clone(),
            active_scheduled_recordings: state.active_scheduled_recordings.clone(),
        });

        let scheduler = state.scheduler.lock().await;

        // Remove old job if exists
        if old_enabled {
            let _ = scheduler.remove_schedule(id).await;
        }

        // Add new job if enabled
        if updated_schedule.is_enabled {
            scheduler.add_schedule(updated_schedule.clone(), state_arc).await?;
        }
    }

    println!("[Schedule] Updated schedule '{}' (ID: {})", updated_schedule.name, updated_schedule.id);

    Ok(updated_schedule)
}

#[tauri::command]
pub async fn delete_recording_schedule(
    state: State<'_, AppState>,
    id: i32
) -> Result<(), String> {
    // Remove from scheduler first
    let scheduler = state.scheduler.lock().await;
    let _ = scheduler.remove_schedule(id).await; // Ignore error if not found
    drop(scheduler);

    // Delete from database
    let conn = get_conn(&state)?;
    let affected = conn.execute("DELETE FROM recording_schedules WHERE id = ?1", [id])
        .map_err(|e| e.to_string())?;

    if affected == 0 {
        return Err("Schedule not found".to_string());
    }

    println!("[Schedule] Deleted schedule ID: {}", id);

    Ok(())
}

#[tauri::command]
pub async fn toggle_schedule(
    state: State<'_, AppState>,
    id: i32,
    enabled: bool
) -> Result<RecordingSchedule, String> {
    update_recording_schedule(
        state,
        id,
        UpdateRecordingSchedule {
            name: None,
            cron_expression: None,
            duration_minutes: None,
            fps: None,
            is_enabled: Some(enabled),
        }
    ).await
}

#[tauri::command]
pub async fn get_recording_cameras(
    state: State<'_, AppState>
) -> Result<Vec<i32>, String> {
    // Get list of camera IDs currently recording
    let processes = state.recording_processes.lock()
        .map_err(|e| format!("Failed to lock recording processes: {}", e))?;

    let camera_ids: Vec<i32> = processes.keys().copied().collect();
    Ok(camera_ids)
}
