use tauri::State;
use crate::models::{Camera, NewCamera, Recording, DiscoveredDevice, PTZCapabilities, PTZMovement, PTZResult, CameraTimeInfo, TimeSyncResult, CameraCapabilities};
use crate::AppState;
use rusqlite::Connection;
use chrono::{Utc, DateTime};
use std::str::FromStr;

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

// Time and PTZ stubs
#[tauri::command]
pub async fn get_camera_time(id: i32) -> Result<CameraTimeInfo, String> {
    Err("Not implemented".to_string())
}

#[tauri::command]
pub async fn sync_camera_time(id: i32) -> Result<TimeSyncResult, String> {
    Err("Not implemented".to_string())
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
pub async fn get_camera_capabilities(id: i32) -> Result<CameraCapabilities, String> {
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
