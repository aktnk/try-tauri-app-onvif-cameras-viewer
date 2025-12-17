use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_snake_case)]
pub struct GpuCapabilities {
    pub availableEncoders: Vec<String>,
    pub preferredEncoder: Option<String>,
    pub gpuType: String,
    pub gpuName: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GpuType {
    Nvidia,
    Amd,
    Intel,
    VaApi,
    VideoToolbox,
    None,
}

impl GpuType {
    pub fn to_string(&self) -> String {
        match self {
            GpuType::Nvidia => "NVIDIA".to_string(),
            GpuType::Amd => "AMD".to_string(),
            GpuType::Intel => "Intel".to_string(),
            GpuType::VaApi => "VA-API".to_string(),
            GpuType::VideoToolbox => "VideoToolbox".to_string(),
            GpuType::None => "None".to_string(),
        }
    }
}

pub async fn detect_gpu_capabilities() -> Result<GpuCapabilities, String> {
    println!("[GPU] Detecting GPU capabilities...");

    // Step 1: Get available encoders from FFmpeg
    let available_encoders = get_available_encoders().await?;
    println!("[GPU] Available encoders: {:?}", available_encoders);

    // Step 2: Detect GPU type
    let (gpu_type, gpu_name) = detect_gpu_type().await;
    println!("[GPU] Detected GPU type: {:?}, name: {:?}", gpu_type, gpu_name);

    // Step 3: Select preferred encoder based on GPU type
    let preferred_encoder = select_preferred_encoder(&gpu_type, &available_encoders);
    println!("[GPU] Preferred encoder: {:?}", preferred_encoder);

    Ok(GpuCapabilities {
        availableEncoders: available_encoders,
        preferredEncoder: preferred_encoder.clone(),
        gpuType: gpu_type.to_string(),
        gpuName: gpu_name,
    })
}

async fn get_available_encoders() -> Result<Vec<String>, String> {
    let output = Command::new("ffmpeg")
        .args(["-encoders", "-hide_banner"])
        .output()
        .map_err(|e| format!("Failed to run ffmpeg: {}", e))?;

    if !output.status.success() {
        return Err("FFmpeg command failed".to_string());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut encoders = Vec::new();

    // Target hardware encoders
    let target_encoders = vec![
        "h264_nvenc",
        "hevc_nvenc",
        "h264_qsv",
        "hevc_qsv",
        "h264_amf",
        "hevc_amf",
        "h264_vaapi",
        "hevc_vaapi",
        "h264_videotoolbox",
        "hevc_videotoolbox",
    ];

    for line in stdout.lines() {
        for encoder in &target_encoders {
            if line.contains(encoder) {
                encoders.push(encoder.to_string());
            }
        }
    }

    Ok(encoders)
}

async fn detect_gpu_type() -> (GpuType, Option<String>) {
    // Try NVIDIA first (most common)
    if let Ok(nvidia_name) = detect_nvidia_gpu().await {
        return (GpuType::Nvidia, Some(nvidia_name));
    }

    // Try Intel
    if let Ok(intel_name) = detect_intel_gpu().await {
        return (GpuType::Intel, Some(intel_name));
    }

    // Try AMD
    if let Ok(amd_name) = detect_amd_gpu().await {
        return (GpuType::Amd, Some(amd_name));
    }

    // Check for VideoToolbox (macOS)
    #[cfg(target_os = "macos")]
    {
        return (GpuType::VideoToolbox, Some("Apple GPU".to_string()));
    }

    // Check for VA-API (Linux)
    #[cfg(target_os = "linux")]
    {
        if std::path::Path::new("/dev/dri/renderD128").exists() {
            return (GpuType::VaApi, Some("VA-API Device".to_string()));
        }
    }

    (GpuType::None, None)
}

async fn detect_nvidia_gpu() -> Result<String, String> {
    let output = Command::new("nvidia-smi")
        .args(["--query-gpu=name", "--format=csv,noheader"])
        .output()
        .map_err(|_| "nvidia-smi not found".to_string())?;

    if !output.status.success() {
        return Err("nvidia-smi failed".to_string());
    }

    let gpu_name = String::from_utf8_lossy(&output.stdout)
        .trim()
        .to_string();

    if gpu_name.is_empty() {
        Err("No NVIDIA GPU found".to_string())
    } else {
        Ok(gpu_name)
    }
}

async fn detect_intel_gpu() -> Result<String, String> {
    #[cfg(target_os = "linux")]
    {
        let output = Command::new("lspci")
            .output()
            .map_err(|_| "lspci not found".to_string())?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if line.to_lowercase().contains("vga") && line.to_lowercase().contains("intel") {
                return Ok(line.split(':').nth(2).unwrap_or("Intel GPU").trim().to_string());
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        // Windows: check via wmic or registry
        let output = Command::new("wmic")
            .args(["path", "win32_VideoController", "get", "name"])
            .output()
            .map_err(|_| "wmic failed".to_string())?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if line.to_lowercase().contains("intel") {
                return Ok(line.trim().to_string());
            }
        }
    }

    Err("No Intel GPU found".to_string())
}

async fn detect_amd_gpu() -> Result<String, String> {
    #[cfg(target_os = "linux")]
    {
        let output = Command::new("lspci")
            .output()
            .map_err(|_| "lspci not found".to_string())?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if line.to_lowercase().contains("vga") &&
               (line.to_lowercase().contains("amd") || line.to_lowercase().contains("radeon")) {
                return Ok(line.split(':').nth(2).unwrap_or("AMD GPU").trim().to_string());
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        let output = Command::new("wmic")
            .args(["path", "win32_VideoController", "get", "name"])
            .output()
            .map_err(|_| "wmic failed".to_string())?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if line.to_lowercase().contains("amd") || line.to_lowercase().contains("radeon") {
                return Ok(line.trim().to_string());
            }
        }
    }

    Err("No AMD GPU found".to_string())
}

fn select_preferred_encoder(gpu_type: &GpuType, available_encoders: &[String]) -> Option<String> {
    match gpu_type {
        GpuType::Nvidia => {
            // Prefer h264_nvenc
            if available_encoders.contains(&"h264_nvenc".to_string()) {
                Some("h264_nvenc".to_string())
            } else {
                None
            }
        }
        GpuType::Intel => {
            // Prefer h264_qsv
            if available_encoders.contains(&"h264_qsv".to_string()) {
                Some("h264_qsv".to_string())
            } else {
                None
            }
        }
        GpuType::Amd => {
            // Prefer h264_amf
            if available_encoders.contains(&"h264_amf".to_string()) {
                Some("h264_amf".to_string())
            } else {
                None
            }
        }
        GpuType::VaApi => {
            // Prefer h264_vaapi
            if available_encoders.contains(&"h264_vaapi".to_string()) {
                Some("h264_vaapi".to_string())
            } else {
                None
            }
        }
        GpuType::VideoToolbox => {
            // Prefer h264_videotoolbox
            if available_encoders.contains(&"h264_videotoolbox".to_string()) {
                Some("h264_videotoolbox".to_string())
            } else {
                None
            }
        }
        GpuType::None => None,
    }
}

/// Test if an encoder actually works by encoding a short test video
pub async fn test_encoder(encoder: &str) -> bool {
    println!("[GPU] Testing encoder: {}", encoder);

    // Build test command based on encoder type
    let mut args = vec![
        "-f".to_string(), "lavfi".to_string(),
        "-i".to_string(), "testsrc=duration=1:size=320x240:rate=30".to_string(),
    ];

    // Add hardware initialization for specific encoders
    match encoder {
        "h264_qsv" | "hevc_qsv" => {
            args.extend_from_slice(&[
                "-init_hw_device".to_string(), "qsv=hw".to_string(),
                "-filter_hw_device".to_string(), "hw".to_string(),
            ]);
        }
        "h264_vaapi" | "hevc_vaapi" => {
            args.extend_from_slice(&[
                "-init_hw_device".to_string(), "vaapi=va:/dev/dri/renderD128".to_string(),
                "-filter_hw_device".to_string(), "va".to_string(),
            ]);
        }
        _ => {}
    }

    args.extend_from_slice(&[
        "-c:v".to_string(), encoder.to_string(),
        "-frames:v".to_string(), "10".to_string(),
        "-f".to_string(), "null".to_string(),
        "-".to_string(),
    ]);

    println!("[GPU] Running test command: ffmpeg {}", args.join(" "));

    let output = Command::new("ffmpeg")
        .args(&args)
        .output();

    match output {
        Ok(result) => {
            let success = result.status.success();
            let stderr = String::from_utf8_lossy(&result.stderr);

            if !success {
                println!("[GPU] Encoder test FAILED for {}:", encoder);
                println!("[GPU] Exit code: {:?}", result.status.code());
                println!("[GPU] Last 10 lines of stderr:");
                let lines: Vec<_> = stderr.lines().collect();
                for line in lines.iter().rev().take(10).rev() {
                    println!("[GPU]   {}", line);
                }
            } else {
                println!("[GPU] Encoder test SUCCEEDED: {}", encoder);
                // Check if frames were actually encoded
                if stderr.contains("frame=") {
                    println!("[GPU] Frames encoded successfully");
                }
            }
            success
        }
        Err(e) => {
            println!("[GPU] Failed to run encoder test command: {}", e);
            false
        }
    }
}
