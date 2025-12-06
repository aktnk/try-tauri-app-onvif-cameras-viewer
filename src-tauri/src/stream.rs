use crate::models::Camera;
use crate::AppState;
use std::process::{Command, Stdio};
use tauri::State;
use std::fs;

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

    // Construct RTSP URL
    let rtsp_url = if camera.camera_type == "onvif" {
        // Use ONVIF protocol to get the stream URI
        crate::onvif::get_onvif_stream_url(&camera).await?
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
                 base_url.replace("rtsp://", &format!("rtsp://{}:{}@", user, urlencoding::encode(pass)))
            } else {
                base_url
            }
        } else {
            base_url
        }
    };

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

pub async fn start_recording(state: State<'_, AppState>, id: i32) -> Result<(), String> {
    Err("Recording not yet implemented".to_string())
}

pub async fn stop_recording(state: State<'_, AppState>, id: i32) -> Result<(), String> {
    Err("Recording not yet implemented".to_string())
}
