use crate::models::{Camera, EncoderSettings};
use crate::AppState;
use crate::gpu_detector::detect_gpu_capabilities;
use crate::encoder::EncoderSelector;
use std::process::{Command, Stdio, Child};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use tauri::{State, Emitter};
use std::fs;
use std::path::PathBuf;
use rusqlite::Connection;
use chrono::{Utc, DateTime};
use chrono_tz::Asia::Tokyo;

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

    // Get encoder configuration with camera FPS
    let encoder_selector = build_encoder_selector(&state).await?;
    let encoder_config = encoder_selector.select_encoder_for_streaming(camera.video_fps).await;

    println!("[Stream] Using encoder: {} (GPU: {}) with FPS: {:?}", encoder_config.codec, encoder_config.is_gpu, camera.video_fps);

    // Build FFmpeg command
    let mut args = vec!["-y".to_string()];

    // Add input format and device arguments based on camera type
    match camera.camera_type.as_str() {
        "uvc" => {
            // UVC camera - use device input with detected settings
            #[cfg(target_os = "linux")]
            {
                args.extend_from_slice(&[
                    "-err_detect".to_string(), "ignore_err".to_string(),  // Ignore MJPEG decode errors (APP field issues)
                    "-fflags".to_string(), "nobuffer+genpts".to_string(),  // Minimize buffering + generate timestamps
                    "-flags".to_string(), "low_delay".to_string(),   // Low delay mode
                    "-avoid_negative_ts".to_string(), "make_zero".to_string(),  // Handle timestamp issues
                ]);

                // Use detected video format if available
                if let Some(ref format) = camera.video_format {
                    args.extend_from_slice(&[
                        "-input_format".to_string(), format.clone(),
                    ]);
                }

                // Use detected resolution if available
                if let (Some(width), Some(height)) = (camera.video_width, camera.video_height) {
                    args.extend_from_slice(&[
                        "-video_size".to_string(), format!("{}x{}", width, height),
                    ]);
                }

                // Use detected FPS if available
                if let Some(fps) = camera.video_fps {
                    args.extend_from_slice(&[
                        "-framerate".to_string(), fps.to_string(),
                    ]);
                }

                args.extend_from_slice(&[
                    "-f".to_string(), "v4l2".to_string(),
                    "-i".to_string(), rtsp_url.clone(),
                ]);

                println!("[Stream] UVC input: format={:?}, size={:?}x{:?}, fps={:?}",
                    camera.video_format, camera.video_width, camera.video_height, camera.video_fps);
            }

            #[cfg(target_os = "windows")]
            {
                args.extend_from_slice(&[
                    "-fflags".to_string(), "nobuffer".to_string(),
                    "-flags".to_string(), "low_delay".to_string(),
                    "-f".to_string(), "dshow".to_string(),
                    "-i".to_string(), format!("video={}", rtsp_url),
                ]);
                // TODO: Add format/resolution/fps detection for Windows
            }

            #[cfg(target_os = "macos")]
            {
                args.extend_from_slice(&[
                    "-fflags".to_string(), "nobuffer".to_string(),
                    "-flags".to_string(), "low_delay".to_string(),
                    "-f".to_string(), "avfoundation".to_string(),
                    "-i".to_string(), rtsp_url.clone(),
                ]);
                // TODO: Add format/resolution/fps detection for macOS
            }
        }
        _ => {
            // ONVIF/RTSP camera - use RTSP input
            args.extend_from_slice(&[
                "-fflags".to_string(), "nobuffer".to_string(),
                "-rtsp_transport".to_string(), "tcp".to_string(),
                "-i".to_string(), rtsp_url.clone(),
            ]);
        }
    }

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
    println!("[Stream] Stopping stream for camera {}", id);

    // Stop streaming process
    {
        let mut processes = state.processes.lock().map_err(|e| e.to_string())?;

        if let Some(mut child) = processes.remove(&id) {
            println!("[Stream] Killing streaming FFmpeg process for camera {}", id);

            // Get PID before killing (for double-check)
            let pid = child.id();

            // Try to kill the process
            if let Err(e) = child.kill() {
                eprintln!("[Stream] Warning: Failed to kill FFmpeg process: {}", e);
            }

            // Wait for process to terminate
            match child.wait() {
                Ok(status) => {
                    println!("[Stream] FFmpeg process exited with status: {}", status);
                }
                Err(e) => {
                    eprintln!("[Stream] Warning: Failed to wait for FFmpeg process: {}", e);
                }
            }

            // Double-check: Kill by process ID (Linux/Unix only)
            #[cfg(unix)]
            {
                use std::process::Command as StdCommand;
                let _ = StdCommand::new("kill")
                    .args(&["-9", &pid.to_string()])
                    .output();
                println!("[Stream] Sent additional SIGKILL to PID {} for safety", pid);
            }
        } else {
            println!("[Stream] No active streaming process found for camera {}", id);
        }
    }

    // Also stop recording if active (user expects both to stop)
    {
        let mut recording_processes = state.recording_processes.lock().map_err(|e| e.to_string())?;

        if let Some(mut child) = recording_processes.remove(&id) {
            println!("[Stream] Stopping active recording for camera {}", id);
            let _ = child.kill();
            let _ = child.wait();

            // Clean up recording database entry
            // Note: This is a simplified cleanup - the recording will be marked as unfinished
            // A full implementation might want to finalize the recording properly
            if let Ok(conn) = Connection::open(&state.db_path) {
                let _ = conn.execute(
                    "DELETE FROM recordings WHERE camera_id = ?1 AND is_finished = 0",
                    [id]
                );
                println!("[Stream] Cleaned up unfinished recording for camera {}", id);
            }
        }
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
            "SELECT id, name, type, host, port, user, pass, xaddr, stream_path,
                    device_path, device_id, device_index,
                    video_format, video_width, video_height, video_fps,
                    created_at, updated_at
             FROM cameras WHERE id = ?1"
        ).map_err(|e| e.to_string())?;

        stmt.query_row([id], |row| {
            let created_at_str: String = row.get(16)?;
            let updated_at_str: String = row.get(17)?;

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
                device_path: row.get(9)?,
                device_id: row.get(10)?,
                device_index: row.get(11)?,
                video_format: row.get(12)?,
                video_width: row.get(13)?,
                video_height: row.get(14)?,
                video_fps: row.get(15)?,
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
    let mut args = vec!["-y".to_string()];

    // Add input format and device arguments based on camera type
    match camera.camera_type.as_str() {
        "uvc" => {
            // UVC camera - use device input with detected settings
            #[cfg(target_os = "linux")]
            {
                // Error handling flags for robust MJPEG decoding
                args.extend_from_slice(&[
                    "-err_detect".to_string(), "ignore_err".to_string(),  // Ignore MJPEG decode errors
                    "-fflags".to_string(), "+genpts".to_string(),         // Generate timestamps
                    "-avoid_negative_ts".to_string(), "make_zero".to_string(),  // Handle timestamp issues
                ]);

                // Use detected video format if available
                if let Some(ref format) = camera.video_format {
                    args.extend_from_slice(&[
                        "-input_format".to_string(), format.clone(),
                    ]);
                }

                // Use detected resolution if available
                if let (Some(width), Some(height)) = (camera.video_width, camera.video_height) {
                    args.extend_from_slice(&[
                        "-video_size".to_string(), format!("{}x{}", width, height),
                    ]);
                }

                // Use detected FPS if available
                if let Some(fps) = camera.video_fps {
                    args.extend_from_slice(&[
                        "-framerate".to_string(), fps.to_string(),
                    ]);
                }

                args.extend_from_slice(&[
                    "-f".to_string(), "v4l2".to_string(),
                    "-i".to_string(), rtsp_url.clone(),
                ]);

                println!("[Recording] UVC input: format={:?}, size={:?}x{:?}, fps={:?}",
                    camera.video_format, camera.video_width, camera.video_height, camera.video_fps);
            }

            #[cfg(target_os = "windows")]
            {
                args.extend_from_slice(&[
                    "-f".to_string(), "dshow".to_string(),
                    "-i".to_string(), format!("video={}", rtsp_url),
                ]);
                // TODO: Add format/resolution/fps detection for Windows
            }

            #[cfg(target_os = "macos")]
            {
                args.extend_from_slice(&[
                    "-f".to_string(), "avfoundation".to_string(),
                    "-i".to_string(), rtsp_url.clone(),
                ]);
                // TODO: Add format/resolution/fps detection for macOS
            }
        }
        _ => {
            // ONVIF/RTSP camera - use RTSP input
            args.extend_from_slice(&[
                "-rtsp_transport".to_string(), "tcp".to_string(),
                "-i".to_string(), rtsp_url.clone(),
            ]);
        }
    }

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

pub async fn stop_recording(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    id: i32
) -> Result<(), String> {
    stop_recording_internal(
        &state.db_path,
        &state.recording_processes,
        &state.recording_dir,
        id,
        Some(&app_handle)
    ).await
}

// Internal implementation shared by both Tauri commands and scheduler
async fn stop_recording_internal(
    db_path: &str,
    recording_processes: &Arc<Mutex<HashMap<i32, Child>>>,
    recording_dir: &PathBuf,
    camera_id: i32,
    app_handle: Option<&tauri::AppHandle>
) -> Result<(), String> {
    let id = camera_id;

    // Stop process
    let process_was_running = {
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
            true
        } else {
            println!("[Recording] No active recording process found for camera {}, checking database...", id);
            false
        }
    };

    let conn = Connection::open(db_path).map_err(|e| e.to_string())?;

    // Find the active recording for this camera
    let mut stmt = conn.prepare("SELECT id, filename, start_time FROM recordings WHERE camera_id = ?1 AND is_finished = 0 ORDER BY start_time DESC LIMIT 1").map_err(|e| e.to_string())?;

    let recording_info: Option<(i32, String, String)> = stmt.query_row([id], |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
    }).ok();

    if let Some((rec_id, temp_filename, start_time_str)) = recording_info {
        let temp_path = recording_dir.join(&temp_filename);

        if temp_path.exists() {
             // Generate final filename using JST timezone
             let start_time = DateTime::parse_from_rfc3339(&start_time_str)
                 .map_err(|e| format!("Invalid start_time: {}", e))?
                 .with_timezone(&Tokyo);
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

             // Emit event to frontend to update recording list
             if let Some(app) = app_handle {
                 if let Err(e) = app.emit("recording-completed", camera_id) {
                     eprintln!("[Event] Warning: Failed to emit recording-completed event: {}", e);
                 } else {
                     println!("[Event] Emitted recording-completed event for camera {}", camera_id);
                 }
             }
        } else {
            // Temp file missing - clean up DB entry
            conn.execute("DELETE FROM recordings WHERE id = ?1", [rec_id]).map_err(|e| e.to_string())?;
            println!("[Recording] Warning: Recording temp file not found, cleaned up DB entry");
        }
    } else {
        // No DB record found
        if !process_was_running {
            // Neither process nor DB record - already stopped or never started
            println!("[Recording] No active recording found for camera {}, already stopped", id);
            return Ok(());
        }
        // Process was running but no DB record - unexpected, but continue
        println!("[Recording] Warning: Recording process was running but no DB record found for camera {}", id);
    }

    Ok(())
}

async fn get_rtsp_url(camera: &Camera) -> Result<String, String> {
    match camera.camera_type.as_str() {
        "onvif" => {
            // Use ONVIF protocol to get the stream URI
            crate::onvif::get_onvif_stream_url(&camera).await
        }
        "uvc" => {
            // For UVC cameras, return device path (not RTSP URL)
            // This will be used as FFmpeg input device
            #[cfg(target_os = "linux")]
            {
                camera.device_path.clone()
                    .ok_or_else(|| "No device path for UVC camera".to_string())
            }

            #[cfg(target_os = "windows")]
            {
                camera.device_id.clone()
                    .ok_or_else(|| "No device ID for UVC camera".to_string())
            }

            #[cfg(target_os = "macos")]
            {
                camera.device_index
                    .map(|idx| idx.to_string())
                    .ok_or_else(|| "No device index for UVC camera".to_string())
            }

            #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
            {
                Err("UVC cameras not supported on this platform".to_string())
            }
        }
        _ => {
            // RTSP Camera
            let base_url = if let Some(path) = &camera.stream_path {
                format!("rtsp://{}:{}{}", camera.host, camera.port, path)
            } else {
                // Default fallback for RTSP if no path
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

pub async fn stop_recording_direct(
    state: &AppState,
    id: i32,
    app_handle: Option<&tauri::AppHandle>
) -> Result<(), String> {
    stop_recording_internal(
        &state.db_path,
        &state.recording_processes,
        &state.recording_dir,
        id,
        app_handle
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

