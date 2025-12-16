use crate::models::Camera;
use crate::AppState;
use std::process::{Command, Stdio};
use tauri::State;
use std::fs;
use std::path::PathBuf;
use rusqlite::Connection;
use chrono::Utc;

// Helper to get DB connection inside stream module
fn get_conn(state: &State<AppState>) -> Result<Connection, String> {
    Connection::open(&state.db_path).map_err(|e| e.to_string())
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

    println!("Starting FFmpeg for camera {}: {}", id, rtsp_url);

    // Spawn FFmpeg
    // Matches reference implementation:
    // -rtsp_transport tcp -i [URL] -c:v copy -an -f hls ...
    let child = Command::new("ffmpeg")
        .args([
            "-y",
            "-fflags", "nobuffer",
            "-rtsp_transport", "tcp",
            "-i", &rtsp_url,
            "-c:v", "libx264",
            "-preset", "ultrafast",
            "-tune", "zerolatency",
            "-an", // Disable audio for stability/latency per reference
            "-f", "hls",
            "-hls_time", "2",
            "-hls_list_size", "3",
            "-hls_flags", "delete_segments+omit_endlist",
            "-hls_segment_filename", segment_filename.to_str().unwrap(),
            output_file.to_str().unwrap()
        ])
        .stdout(Stdio::null()) // Stdout to null
        .stderr(Stdio::inherit()) // Stderr to inherit for debugging in console
        .spawn()
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
    let id = camera.id;

    // Check if already recording
    {
        let processes = state.recording_processes.lock().map_err(|e| e.to_string())?;
        if processes.contains_key(&id) {
             return Err("Recording is already in progress".to_string());
        }
    }

    let conn = get_conn(&state)?;
    
    // Insert initial recording record
    // Using a temporary filename for now
    let temp_filename = format!("temp_rec_{}.ts", id);
    conn.execute(
        "INSERT INTO recordings (camera_id, filename, start_time, is_finished) VALUES (?1, ?2, ?3, ?4)",
        (id, &temp_filename, Utc::now().to_rfc3339(), false),
    ).map_err(|e| e.to_string())?;
    
    // Get the rtsp url
    let rtsp_url = get_rtsp_url(&camera).await?;
    
    let temp_file_path = state.recording_dir.join(&temp_filename);
    
    println!("Starting Recording FFmpeg for camera {}: {}", id, rtsp_url);

    // Spawn FFmpeg for recording
    // -i [URL] -c:v libx264 -preset ultrafast -c:a aac -f mpegts [out.ts]
    let child = Command::new("ffmpeg")
        .args([
            "-y",
            "-rtsp_transport", "tcp",
            "-i", &rtsp_url,
            "-c:v", "libx264",
            "-preset", "ultrafast",
            "-c:a", "aac", // Encode audio to AAC
            "-f", "mpegts",
            temp_file_path.to_str().unwrap()
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("Failed to start recording ffmpeg: {}", e))?;

    // Save process
    {
        let mut processes = state.recording_processes.lock().map_err(|e| e.to_string())?;
        processes.insert(id, child);
    }

    Ok(())
}

pub async fn stop_recording(state: State<'_, AppState>, id: i32) -> Result<(), String> {
    // Stop process
    {
        let mut processes = state.recording_processes.lock().map_err(|e| e.to_string())?;
        if let Some(mut child) = processes.remove(&id) {
            let _ = child.kill();
            let _ = child.wait();
        } else {
             return Err("No active recording found for this camera".to_string());
        }
    }

    let conn = get_conn(&state)?;
    
    // Find the active recording for this camera
    let mut stmt = conn.prepare("SELECT id, filename, start_time FROM recordings WHERE camera_id = ?1 AND is_finished = 0 ORDER BY start_time DESC LIMIT 1").map_err(|e| e.to_string())?;
    
    let recording_info: Option<(i32, String, String)> = stmt.query_row([id], |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
    }).ok();

    if let Some((rec_id, temp_filename, start_time_str)) = recording_info {
        let temp_path = state.recording_dir.join(&temp_filename);
        
        if temp_path.exists() {
             // Generate final filename
             let start_time = chrono::DateTime::parse_from_rfc3339(&start_time_str).unwrap_or(Utc::now().into()).with_timezone(&Utc);
             let final_filename = format!("rec_{}_{}.mp4", id, start_time.format("%Y%m%d_%H%M%S"));
             let final_path = state.recording_dir.join(&final_filename);

             println!("Converting recording {} to {}", temp_filename, final_filename);

             // Convert TS to MP4 (remux)
             // ffmpeg -i temp.ts -c copy final.mp4
             let output = Command::new("ffmpeg")
                .args([
                    "-y",
                    "-i", temp_path.to_str().unwrap(),
                    "-c", "copy",
                    "-movflags", "+faststart",
                    final_path.to_str().unwrap()
                ])
                .output()
                .map_err(|e| format!("Failed to remux recording: {}", e))?;

             if !output.status.success() {
                 return Err(format!("FFmpeg remux failed: {}", String::from_utf8_lossy(&output.stderr)));
             }

             // Remove temp file
             let _ = fs::remove_file(temp_path);

             // Generate thumbnail
             let thumbnail_filename = final_filename.replace(".mp4", ".jpg");
             let thumbnail_path = state.recording_dir.join("thumbnails").join(&thumbnail_filename);

             // Ensure thumbnails directory exists
             if let Some(parent) = thumbnail_path.parent() {
                 fs::create_dir_all(parent).map_err(|e| format!("Failed to create thumbnails directory: {}", e))?;
             }

             // Try to generate thumbnail (non-fatal if it fails)
             let thumbnail_result = generate_thumbnail(&final_path, &thumbnail_path);
             let thumbnail_db_value = match thumbnail_result {
                 Ok(_) => Some(thumbnail_filename),
                 Err(e) => {
                     println!("[Thumbnail] Warning: Failed to generate thumbnail: {}", e);
                     None
                 }
             };

             // Update DB
             conn.execute(
                "UPDATE recordings SET is_finished = 1, filename = ?1, thumbnail = ?2, end_time = ?3 WHERE id = ?4",
                (&final_filename, thumbnail_db_value, Utc::now().to_rfc3339(), rec_id)
             ).map_err(|e| e.to_string())?;
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
    let output = Command::new("ffmpeg")
        .args([
            "-y",
            "-ss", "00:00:02",
            "-i", video_path.to_str().unwrap(),
            "-vframes", "1",
            "-vf", "scale=320:-1",
            "-q:v", "2",
            thumbnail_path.to_str().unwrap()
        ])
        .output()
        .map_err(|e| format!("Failed to spawn FFmpeg for thumbnail: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        println!("[Thumbnail] FFmpeg failed: {}", stderr);
        return Err(format!("FFmpeg thumbnail generation failed: {}", stderr));
    }

    println!("[Thumbnail] Successfully generated thumbnail");
    Ok(())
}

