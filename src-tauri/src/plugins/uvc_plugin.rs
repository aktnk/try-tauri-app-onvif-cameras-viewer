use crate::camera_plugin::{CameraInfo, CameraPlugin};
use crate::models::Camera;
use async_trait::async_trait;
use std::process::Command;

/// UVC camera plugin implementation
/// Supports USB Video Class cameras via FFmpeg
pub struct UvcPlugin;

impl UvcPlugin {
    pub fn new() -> Self {
        UvcPlugin
    }
}

#[async_trait]
impl CameraPlugin for UvcPlugin {
    fn plugin_type(&self) -> &str {
        "uvc"
    }

    async fn discover(&self) -> Result<Vec<CameraInfo>, String> {
        println!("[UvcPlugin] Starting UVC camera discovery...");

        // Platform-specific discovery
        #[cfg(target_os = "linux")]
        {
            discover_v4l2_cameras().await
        }

        #[cfg(target_os = "windows")]
        {
            discover_directshow_cameras().await
        }

        #[cfg(target_os = "macos")]
        {
            discover_avfoundation_cameras().await
        }

        #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
        {
            Err("UVC cameras not supported on this platform".to_string())
        }
    }

    async fn get_stream_url(&self, camera: &Camera) -> Result<String, String> {
        println!("[UvcPlugin] Getting stream URL for camera: {}", camera.name);

        // For UVC cameras, return device path/identifier
        // FFmpeg will use this directly as input
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

    fn supports_ptz(&self) -> bool {
        false // UVC cameras typically don't support PTZ
    }

    fn supports_time_sync(&self) -> bool {
        false // UVC cameras don't have time sync capability
    }

    async fn get_profiles(&self, _camera: &Camera) -> Result<Vec<(String, String)>, String> {
        Err("Profiles not supported for UVC cameras".to_string())
    }
}

// ============================================================================
// Linux v4l2 Discovery
// ============================================================================

#[cfg(target_os = "linux")]
async fn discover_v4l2_cameras() -> Result<Vec<CameraInfo>, String> {
    use std::fs;
    use std::path::Path;

    println!("[UvcPlugin] Discovering v4l2 devices on Linux...");

    let mut cameras = Vec::new();

    // Read /dev directory for video devices
    let dev_dir = Path::new("/dev");
    if !dev_dir.exists() {
        return Err("/dev directory not found".to_string());
    }

    let entries = fs::read_dir(dev_dir).map_err(|e| format!("Failed to read /dev: {}", e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
        let path = entry.path();
        let path_str = path.to_string_lossy().to_string();

        // Filter for /dev/videoN devices
        if path_str.starts_with("/dev/video") {
            // Extract device number
            if let Some(num_str) = path_str.strip_prefix("/dev/video") {
                if num_str.parse::<u32>().is_ok() {
                    // Check if device supports Video Capture (exclude metadata-only devices)
                    if !check_v4l2_video_capture(&path_str) {
                        println!("[UvcPlugin] Skipping {} (metadata-only device)", path_str);
                        continue;
                    }

                    // Try to get device name using v4l2-ctl (if available)
                    let device_name = get_v4l2_device_name(&path_str).unwrap_or_else(|| {
                        format!("USB Camera ({})", num_str)
                    });

                    // Detect best video format, resolution, and FPS
                    let (video_format, video_width, video_height, video_fps) =
                        get_best_video_format(&path_str);

                    println!("[UvcPlugin] Detected format for {}: {:?} {}x{} @ {}fps",
                        path_str, video_format, video_width.unwrap_or(0),
                        video_height.unwrap_or(0), video_fps.unwrap_or(0));

                    cameras.push(CameraInfo {
                        name: device_name,
                        host: "localhost".to_string(), // UVC is local
                        port: 0, // Not applicable for UVC
                        camera_type: "uvc".to_string(),
                        user: None,
                        pass: None,
                        device_path: Some(path_str.clone()),
                        device_id: None,
                        device_index: None,
                        video_format,
                        video_width,
                        video_height,
                        video_fps,
                    });

                    println!("[UvcPlugin] Found v4l2 device: {}", path_str);
                }
            }
        }
    }

    println!("[UvcPlugin] Found {} v4l2 camera(s)", cameras.len());
    Ok(cameras)
}

#[cfg(target_os = "linux")]
fn get_v4l2_device_name(device_path: &str) -> Option<String> {
    // Try to use v4l2-ctl to get device name
    // v4l2-ctl --device=/dev/video0 --info
    let output = Command::new("v4l2-ctl")
        .args(&["--device", device_path, "--info"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse "Card type" line
    // Example: "Card type      : HD Pro Webcam C920"
    // Note: Camera name may contain ':' (e.g., "UVC Camera (046d:0825)")
    for line in stdout.lines() {
        if line.contains("Card type") {
            // Split only on first ':' by using splitn(2)
            let parts: Vec<&str> = line.splitn(2, ':').collect();
            if parts.len() == 2 {
                return Some(parts[1].trim().to_string());
            }
        }
    }

    None
}

#[cfg(target_os = "linux")]
fn check_v4l2_video_capture(device_path: &str) -> bool {
    // Check if device supports Video Capture
    // v4l2-ctl --device=/dev/videoX --all
    // Look for "Device Caps" section containing "Video Capture"
    let output = match Command::new("v4l2-ctl")
        .args(&["--device", device_path, "--all"])
        .output()
    {
        Ok(output) => output,
        Err(_) => return false,
    };

    if !output.status.success() {
        return false;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse "Device Caps" section
    // Example:
    // Device Caps      : 0x04200001
    //     Video Capture
    //     Streaming
    //     Extended Pix Format
    let mut in_device_caps = false;
    for line in stdout.lines() {
        if line.contains("Device Caps") {
            in_device_caps = true;
            continue;
        }

        // Check if we're in Device Caps section and find "Video Capture"
        if in_device_caps {
            // End of Device Caps section (next section starts or empty line)
            if !line.starts_with('\t') && !line.trim().is_empty() && !line.contains("Video Capture") {
                break;
            }

            if line.contains("Video Capture") && !line.contains("Metadata Capture") {
                return true;
            }
        }
    }

    false
}

#[cfg(target_os = "linux")]
fn get_best_video_format(device_path: &str) -> (Option<String>, Option<i32>, Option<i32>, Option<i32>) {
    // Get supported formats using v4l2-ctl --list-formats-ext
    // Priority: MJPEG > YUYV, highest resolution, highest FPS
    let output = match Command::new("v4l2-ctl")
        .args(&["--device", device_path, "--list-formats-ext"])
        .output()
    {
        Ok(output) => output,
        Err(_) => return (None, None, None, None),
    };

    if !output.status.success() {
        return (None, None, None, None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    let mut best_format: Option<String> = None;
    let mut best_width: Option<i32> = None;
    let mut best_height: Option<i32> = None;
    let mut best_fps: Option<i32> = None;
    let mut best_score = 0i32;

    let mut current_format: Option<String> = None;
    let mut current_width: Option<i32> = None;
    let mut current_height: Option<i32> = None;

    for line in stdout.lines() {
        // Detect format: [0]: 'MJPG' or [1]: 'YUYV'
        if line.contains("'MJPG'") || line.contains("Motion-JPEG") {
            current_format = Some("mjpeg".to_string());
        } else if line.contains("'YUYV'") {
            current_format = Some("yuyv".to_string());
        }

        // Detect size: Size: Discrete 1280x720
        if line.contains("Size: Discrete") {
            if let Some(size_str) = line.split("Size: Discrete").nth(1) {
                let size_parts: Vec<&str> = size_str.trim().split('x').collect();
                if size_parts.len() == 2 {
                    current_width = size_parts[0].parse().ok();
                    current_height = size_parts[1].parse().ok();
                }
            }
        }

        // Detect FPS: Interval: Discrete 0.033s (30.000 fps)
        if line.contains("Interval: Discrete") && line.contains("fps") {
            if let Some(fps_str) = line.split('(').nth(1) {
                if let Some(fps_num_str) = fps_str.split('.').next() {
                    if let Ok(fps) = fps_num_str.trim().parse::<i32>() {
                        // Calculate score: format priority + resolution + fps
                        let format_score = if current_format.as_deref() == Some("mjpeg") {
                            10000 // MJPEG gets high priority
                        } else {
                            0
                        };
                        let resolution_score = current_width.unwrap_or(0) * current_height.unwrap_or(0) / 1000;
                        let fps_score = fps;
                        let total_score = format_score + resolution_score + fps_score;

                        if total_score > best_score {
                            best_score = total_score;
                            best_format = current_format.clone();
                            best_width = current_width;
                            best_height = current_height;
                            best_fps = Some(fps);
                        }
                    }
                }
            }
        }
    }

    (best_format, best_width, best_height, best_fps)
}

// ============================================================================
// Windows DirectShow Discovery
// ============================================================================

#[cfg(target_os = "windows")]
async fn discover_directshow_cameras() -> Result<Vec<CameraInfo>, String> {
    println!("[UvcPlugin] Discovering DirectShow devices on Windows...");

    // Use FFmpeg to list DirectShow devices
    // ffmpeg -list_devices true -f dshow -i dummy
    let output = Command::new("ffmpeg")
        .args(&["-list_devices", "true", "-f", "dshow", "-i", "dummy"])
        .output()
        .map_err(|e| format!("Failed to run ffmpeg: {}", e))?;

    // FFmpeg outputs device list to stderr
    let stderr = String::from_utf8_lossy(&output.stderr);

    let mut cameras = Vec::new();
    let mut in_video_section = false;

    for line in stderr.lines() {
        // DirectShow video devices section starts with:
        // [dshow @ ...] DirectShow video devices
        if line.contains("DirectShow video devices") {
            in_video_section = true;
            continue;
        }

        // Audio devices section (end of video section)
        if line.contains("DirectShow audio devices") {
            in_video_section = false;
            break;
        }

        // Parse device lines:
        // [dshow @ ...] "HP HD Camera"
        if in_video_section && line.contains("\"") {
            if let Some(device_name) = parse_dshow_device_line(line) {
                cameras.push(CameraInfo {
                    name: device_name.clone(),
                    host: "localhost".to_string(),
                    port: 0,
                    camera_type: "uvc".to_string(),
                    user: None,
                    pass: None,
                    device_path: None,
                    device_id: Some(device_name), // Use device name as ID for dshow
                    device_index: None,
                    video_format: None, // TODO: implement detection for Windows
                    video_width: None,
                    video_height: None,
                    video_fps: None,
                });

                println!("[UvcPlugin] Found DirectShow device: {}", device_name);
            }
        }
    }

    println!("[UvcPlugin] Found {} DirectShow camera(s)", cameras.len());
    Ok(cameras)
}

#[cfg(target_os = "windows")]
fn parse_dshow_device_line(line: &str) -> Option<String> {
    // Extract device name from quotes
    // Example: [dshow @ 0x...] "HP HD Camera"
    let start = line.find('"')?;
    let end = line.rfind('"')?;
    if end > start {
        Some(line[start + 1..end].to_string())
    } else {
        None
    }
}

// ============================================================================
// macOS AVFoundation Discovery
// ============================================================================

#[cfg(target_os = "macos")]
async fn discover_avfoundation_cameras() -> Result<Vec<CameraInfo>, String> {
    println!("[UvcPlugin] Discovering AVFoundation devices on macOS...");

    // Use FFmpeg to list AVFoundation devices
    // ffmpeg -f avfoundation -list_devices true -i ""
    let output = Command::new("ffmpeg")
        .args(&["-f", "avfoundation", "-list_devices", "true", "-i", ""])
        .output()
        .map_err(|e| format!("Failed to run ffmpeg: {}", e))?;

    // FFmpeg outputs device list to stderr
    let stderr = String::from_utf8_lossy(&output.stderr);

    let mut cameras = Vec::new();
    let mut device_index = 0;
    let mut in_video_section = false;

    for line in stderr.lines() {
        // AVFoundation video devices section starts with:
        // [AVFoundation indev @ ...] AVFoundation video devices:
        if line.contains("AVFoundation video devices") {
            in_video_section = true;
            continue;
        }

        // Audio devices section (end of video section)
        if line.contains("AVFoundation audio devices") {
            in_video_section = false;
            break;
        }

        // Parse device lines:
        // [AVFoundation indev @ ...] [0] FaceTime HD Camera
        if in_video_section && line.contains(']') && line.contains('[') {
            if let Some(device_name) = parse_avfoundation_device_line(line) {
                cameras.push(CameraInfo {
                    name: device_name,
                    host: "localhost".to_string(),
                    port: 0,
                    camera_type: "uvc".to_string(),
                    user: None,
                    pass: None,
                    device_path: None,
                    device_id: None,
                    device_index: Some(device_index),
                    video_format: None, // TODO: implement detection for macOS
                    video_width: None,
                    video_height: None,
                    video_fps: None,
                });

                println!("[UvcPlugin] Found AVFoundation device [{}]", device_index);
                device_index += 1;
            }
        }
    }

    println!("[UvcPlugin] Found {} AVFoundation camera(s)", cameras.len());
    Ok(cameras)
}

#[cfg(target_os = "macos")]
fn parse_avfoundation_device_line(line: &str) -> Option<String> {
    // Extract device name after [index]
    // Example: [AVFoundation indev @ 0x...] [0] FaceTime HD Camera
    let parts: Vec<&str> = line.split(']').collect();
    if parts.len() >= 3 {
        // parts[2] should contain the device name
        Some(parts[2].trim().to_string())
    } else {
        None
    }
}
