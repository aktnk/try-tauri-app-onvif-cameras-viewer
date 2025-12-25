use crate::models::{Camera, EncoderSettings};
use crate::AppState;
use crate::gpu_detector::detect_gpu_capabilities;
use crate::encoder::EncoderSelector;
use std::process::{Command, Stdio, Child};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use tauri::State;
use std::fs;
use std::path::PathBuf;
use rusqlite::Connection;
use chrono::{Utc, DateTime};

// Windows-specific imports for hiding console window
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

// Helper to get DB connection inside stream module
fn get_conn(state: &State<AppState>) -> Result<Connection, String> {
    Connection::open(&state.db_path).map_err(|e| e.to_string())
}

// Get encoder settings from database
async fn get_encoder_settings(state: &State<'_, AppState>) -> Result<EncoderSettings, String> {
    let conn = get_conn(state)?;

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

// Build encoder selector
async fn build_encoder_selector(state: &State<'_, AppState>) -> Result<EncoderSelector, String> {
    let capabilities = detect_gpu_capabilities().await?;
    let settings = get_encoder_settings(state).await?;

    Ok(EncoderSelector::new(capabilities, settings))
}

pub async fn start_stream(state: State<'_, AppState>, camera: Camera) -> Result<String, String> {
    let id = camera.id;

    // Check if already running
    {
        let processes = state.processes.lock().map_err(|e| e.to_string())?;
        if processes.contains_key(&id) {
            return Ok(format!("streams/{}/index.m3u8", id));
        }
    }

    let stream_dir = state.stream_dir.join(id.to_string());
    if stream_dir.exists() {
        fs::remove_dir_all(&stream_dir).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(&stream_dir).map_err(|e| e.to_string())?;

    let rtsp_url = get_rtsp_url(&camera).await?;

    let output_file = stream_dir.join("index.m3u8");
    let segment_filename = stream_dir.join("segment_%03d.ts");

    println!("[Stream] Starting FFmpeg for camera {}: {}", id, rtsp_url);

    // Get encoder configuration
    let encoder_selector = build_encoder_selector(&state).await?;
    let encoder_config = encoder_selector.select_encoder_for_streaming().await;

    println!("[Stream] Using encoder: {} (GPU: {})", encoder_config.codec, encoder_config.is_gpu);

    // Build FFmpeg command
    let mut args = vec![
        "-y".to_string(),
        "-fflags".to_string(), "nobuffer".to_string(),
        "-rtsp_transport".to_string(), "tcp".to_string(),
        "-i".to_string(), rtsp_url.clone(),
    ];

    // Add encoder-specific arguments
    args.extend(encoder_config.args);

    // Add common streaming arguments
    args.extend_from_slice(&[
        "-an".to_string(), // Disable audio for stability/latency
        "-f".to_string(), "hls".to_string(),
        "-hls_time".to_string(), "2".to_string(),
        "-hls_list_size".to_string(), "15".to_string(),
        "-hls_delete_threshold".to_string(), "3".to_string(),
        "-hls_flags".to_string(), "delete_segments+omit_endlist+program_date_time".to_string(),
        "-hls_segment_type".to_string(), "mpegts".to_string(),
        "-hls_segment_filename".to_string(), segment_filename.to_str().unwrap().to_string(),
        output_file.to_str().unwrap().to_string(),
    ]);

    // Spawn FFmpeg
    let mut cmd = Command::new("ffmpeg");
    cmd.args(&args)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());

    // Hide console window on Windows
    #[cfg(target_os = "windows")]
    {
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let child = cmd.spawn()
        .map_err(|e| format!("Failed to start ffmpeg: {}", e))?;

    // Save process
    {
        let mut processes = state.processes.lock().map_err(|e| e.to_string())?;
        processes.insert(id, child);
    }

    Ok(format!("streams/{}/index.m3u8", id))
}

pub async fn stop_stream(state: State<'_, AppState>, id: i32) -> Result<(), String> {
    let mut processes = state.processes.lock().map_err(|e| e.to_string())?;
    
    if let Some(mut child) = processes.remove(&id) {
        let _ = child.kill(); // Ignore error if already dead
        let _ = child.wait(); // Clean up zombie
    }
    
    let stream_dir = state.stream_dir.join(id.to_string());
    if stream_dir.exists() {
        // Optional: clean up files after stop? Reference does it.
        // fs::remove_dir_all(&stream_dir).map_err(|e| e.to_string())?;
    }
    
    Ok(())
}

pub async fn start_recording(state: State<'_, AppState>, camera: Camera) -> Result<(), String> {
    start_recording_with_options(state, camera.id, None).await
}

pub async fn start_recording_with_options(
    state: State<'_, AppState>,
    camera_id: i32,
    fps: Option<i32>
) -> Result<(), String> {
    start_recording_internal(
        &state.db_path,
        &state.recording_processes,
        &state.recording_dir,
        camera_id,
        fps
    ).await
}

// Internal implementation shared by both Tauri commands and scheduler
async fn start_recording_internal(
    db_path: &str,
    recording_processes: &Arc<Mutex<HashMap<i32, Child>>>,
    recording_dir: &PathBuf,
    camera_id: i32,
    fps: Option<i32>
) -> Result<(), String> {
    let id = camera_id;

    // Check if already recording
    {
        let processes = recording_processes.lock().map_err(|e| e.to_string())?;
        if processes.contains_key(&id) {
             return Err("Recording is already in progress".to_string());
        }
    }

    // Get camera info
    let camera = {
        let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
        let mut stmt = conn.prepare(
            "SELECT id, name, type, host, port, user, pass, xaddr, stream_path, created_at, updated_at
             FROM cameras WHERE id = ?1"
        ).map_err(|e| e.to_string())?;

        stmt.query_row([id], |row| {
            let created_at_str: String = row.get(9)?;
            let updated_at_str: String = row.get(10)?;

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
                created_at: DateTime::parse_from_rfc3339(&created_at_str)
                    .unwrap_or(Utc::now().into())
                    .with_timezone(&Utc),
                updated_at: DateTime::parse_from_rfc3339(&updated_at_str)
                    .unwrap_or(Utc::now().into())
                    .with_timezone(&Utc),
            })
        }).map_err(|e| format!("Camera not found: {}", e))?
    };

    // Get the rtsp url
    let rtsp_url = get_rtsp_url(&camera).await?;

    let temp_filename = format!("temp_rec_{}.ts", id);
    let temp_file_path = recording_dir.join(&temp_filename);

    println!("[Recording] Starting FFmpeg for camera {}: {}", id, rtsp_url);
    if let Some(target_fps) = fps {
        println!("[Recording] Target FPS: {}", target_fps);
    }

    // Get encoder configuration
    let encoder_selector = build_encoder_selector_from_path(db_path).await?;
    let encoder_config = encoder_selector.select_encoder_for_recording().await;

    println!("[Recording] Using encoder: {} (GPU: {})", encoder_config.codec, encoder_config.is_gpu);

    // Build FFmpeg command
    let mut args = vec![
        "-y".to_string(),
        "-rtsp_transport".to_string(), "tcp".to_string(),
        "-i".to_string(), rtsp_url.clone(),
    ];

    // Add FPS filter if specified
    if let Some(target_fps) = fps {
        args.extend_from_slice(&[
            "-r".to_string(),
            target_fps.to_string(),
        ]);
    }

    // Add encoder-specific arguments
    args.extend(encoder_config.args);

    // Add audio and output format
    args.extend_from_slice(&[
        "-c:a".to_string(), "aac".to_string(),
        "-f".to_string(), "mpegts".to_string(),
        temp_file_path.to_str().unwrap().to_string(),
    ]);

    // Spawn FFmpeg for recording
    let mut cmd = Command::new("ffmpeg");
    cmd.args(&args)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());

    // Hide console window on Windows
    #[cfg(target_os = "windows")]
    {
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let child = cmd.spawn()
        .map_err(|e| format!("Failed to start recording ffmpeg: {}", e))?;

    // FFmpeg started successfully - now insert DB record in transaction
    {
        let mut conn = Connection::open(db_path).map_err(|e| e.to_string())?;
        let tx = conn.transaction().map_err(|e| e.to_string())?;

        tx.execute(
            "INSERT INTO recordings (camera_id, filename, start_time, is_finished) VALUES (?1, ?2, ?3, ?4)",
            (id, &temp_filename, Utc::now().to_rfc3339(), false),
        ).map_err(|e| e.to_string())?;

        tx.commit().map_err(|e| {
            eprintln!("[Recording] Failed to commit transaction");
            format!("Failed to commit recording transaction: {}", e)
        })?;

        println!("[Recording] Recording registered in database successfully");
    }

    // Save process
    {
        let mut processes = recording_processes.lock().map_err(|e| e.to_string())?;
        processes.insert(id, child);
    }

    Ok(())
}

pub async fn stop_recording(state: State<'_, AppState>, id: i32) -> Result<(), String> {
    stop_recording_internal(
        &state.db_path,
        &state.recording_processes,
        &state.recording_dir,
        id
    ).await
}

// Internal implementation shared by both Tauri commands and scheduler
async fn stop_recording_internal(
    db_path: &str,
    recording_processes: &Arc<Mutex<HashMap<i32, Child>>>,
    recording_dir: &PathBuf,
    camera_id: i32
) -> Result<(), String> {
    let id = camera_id;

    // Stop process
    {
        let mut processes = recording_processes.lock().map_err(|e| e.to_string())?;
        if let Some(mut child) = processes.remove(&id) {
            if let Err(e) = child.kill() {
                eprintln!("[Recording] Warning: Failed to kill process: {}", e);
            }

            match child.wait() {
                Ok(status) => {
                    if !status.success() {
                        println!("[Recording] FFmpeg exited with status: {}", status);
                    }
                }
                Err(e) => {
                    eprintln!("[Recording] Warning: Failed to wait for process: {}", e);
                }
            }
        } else {
             return Err("No active recording found for this camera".to_string());
        }
    }

    let conn = Connection::open(db_path).map_err(|e| e.to_string())?;

    // Find the active recording for this camera
    let mut stmt = conn.prepare("SELECT id, filename, start_time FROM recordings WHERE camera_id = ?1 AND is_finished = 0 ORDER BY start_time DESC LIMIT 1").map_err(|e| e.to_string())?;

    let recording_info: Option<(i32, String, String)> = stmt.query_row([id], |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
    }).ok();

    if let Some((rec_id, temp_filename, start_time_str)) = recording_info {
        let temp_path = recording_dir.join(&temp_filename);

        if temp_path.exists() {
             // Generate final filename
             let start_time = DateTime::parse_from_rfc3339(&start_time_str)
                 .map_err(|e| format!("Invalid start_time: {}", e))?
                 .with_timezone(&Utc);
             let final_filename = format!("rec_{}_{}.mp4", id, start_time.format("%Y%m%d_%H%M%S"));
             let final_path = recording_dir.join(&final_filename);

             println!("[Recording] Converting {} to {}", temp_filename, final_filename);

             // Convert TS to MP4 (remux)
             let mut cmd = Command::new("ffmpeg");
             cmd.args([
                    "-y",
                    "-i", temp_path.to_str().unwrap(),
                    "-c", "copy",
                    "-movflags", "+faststart",
                    final_path.to_str().unwrap()
                ]);

             // Hide console window on Windows
             #[cfg(target_os = "windows")]
             {
                 const CREATE_NO_WINDOW: u32 = 0x08000000;
                 cmd.creation_flags(CREATE_NO_WINDOW);
             }

             let output = cmd.output()
                .map_err(|e| format!("Failed to remux recording: {}", e))?;

             if !output.status.success() {
                 return Err(format!("FFmpeg remux failed: {}", String::from_utf8_lossy(&output.stderr)));
             }

             // Remove temp file
             let _ = fs::remove_file(&temp_path);

             // Generate thumbnail
             let thumbnail_filename = final_filename.replace(".mp4", ".jpg");
             let thumbnail_path = recording_dir.join("thumbnails").join(&thumbnail_filename);

             // Ensure thumbnails directory exists
             if let Some(parent) = thumbnail_path.parent() {
                 fs::create_dir_all(parent).map_err(|e| format!("Failed to create thumbnails directory: {}", e))?;
             }

             // Try to generate thumbnail (non-fatal if it fails)
             let thumbnail_result = generate_thumbnail(&final_path, &thumbnail_path);
             let thumbnail_db_value = match thumbnail_result {
                 Ok(_) => Some(thumbnail_filename),
                 Err(e) => {
                     eprintln!("[Thumbnail] Warning: Failed to generate thumbnail: {}", e);
                     None
                 }
             };

             // Update DB
             conn.execute(
                "UPDATE recordings SET is_finished = 1, filename = ?1, thumbnail = ?2, end_time = ?3 WHERE id = ?4",
                (&final_filename, thumbnail_db_value, Utc::now().to_rfc3339(), rec_id)
             ).map_err(|e| e.to_string())?;

             println!("[Recording] Recording saved: {}", final_filename);
        } else {
            // Temp file missing?
            conn.execute("DELETE FROM recordings WHERE id = ?1", [rec_id]).map_err(|e| e.to_string())?;
            return Err("Recording temp file not found".to_string());
        }
    } else {
        return Err("No active recording found in DB".to_string());
    }

    Ok(())
}

async fn get_rtsp_url(camera: &Camera) -> Result<String, String> {
     if camera.camera_type == "onvif" {
        // Use ONVIF protocol to get the stream URI
        crate::onvif::get_onvif_stream_url(&camera).await
    } else {
        // RTSP Camera
        let base_url = if let Some(path) = &camera.stream_path {
            format!("rtsp://{}:{}{}", camera.host, camera.port, path)
        } else {
             // Default fallback for RTSP if no path? Should probably error or assume root
            format!("rtsp://{}:{}/", camera.host, camera.port)
        };

        if let (Some(user), Some(pass)) = (&camera.user, &camera.pass) {
            if !user.is_empty() {
                 Ok(base_url.replace("rtsp://", &format!("rtsp://{}:{}@", user, urlencoding::encode(pass))))
            } else {
                Ok(base_url)
            }
        } else {
            Ok(base_url)
        }
    }
}

// Generate thumbnail from video file using FFmpeg
fn generate_thumbnail(video_path: &PathBuf, thumbnail_path: &PathBuf) -> Result<(), String> {
    println!("[Thumbnail] Generating thumbnail from {:?} to {:?}", video_path, thumbnail_path);

    // FFmpeg command: extract frame at 2 seconds, scale to 320px width, high quality
    let mut cmd = Command::new("ffmpeg");
    cmd.args([
            "-y",
            "-ss", "00:00:02",
            "-i", video_path.to_str().unwrap(),
            "-vframes", "1",
            "-vf", "scale=320:-1",
            "-q:v", "2",
            thumbnail_path.to_str().unwrap()
        ]);

    // Hide console window on Windows
    #[cfg(target_os = "windows")]
    {
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let output = cmd.output()
        .map_err(|e| format!("Failed to spawn FFmpeg for thumbnail: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        println!("[Thumbnail] FFmpeg failed: {}", stderr);
        return Err(format!("FFmpeg thumbnail generation failed: {}", stderr));
    }

    println!("[Thumbnail] Successfully generated thumbnail");
    Ok(())
}

// Direct versions of functions for scheduler (no State wrapper needed)
pub async fn start_recording_with_options_direct(
    state: &AppState,
    camera_id: i32,
    fps: Option<i32>
) -> Result<(), String> {
    start_recording_internal(
        &state.db_path,
        &state.recording_processes,
        &state.recording_dir,
        camera_id,
        fps
    ).await
}

pub async fn stop_recording_direct(state: &AppState, id: i32) -> Result<(), String> {
    stop_recording_internal(
        &state.db_path,
        &state.recording_processes,
        &state.recording_dir,
        id
    ).await
}

// Helper function to build encoder selector from db_path
async fn build_encoder_selector_from_path(db_path: &str) -> Result<EncoderSelector, String> {
    let capabilities = detect_gpu_capabilities().await?;

    let conn = Connection::open(db_path).map_err(|e| e.to_string())?;

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

    Ok(EncoderSelector::new(capabilities, settings))
}

