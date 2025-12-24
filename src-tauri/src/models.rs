use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[allow(non_snake_case)]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Camera {
    pub id: i32,
    pub name: String,
    #[serde(rename = "type")]
    pub camera_type: String, // "onvif" or "rtsp"
    pub host: String,
    pub port: i32,
    pub user: Option<String>,
    pub pass: Option<String>,
    pub xaddr: Option<String>,
    pub stream_path: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NewCamera {
    pub name: String,
    #[serde(rename = "type")]
    pub camera_type: String,
    pub host: String,
    pub port: i32,
    pub user: Option<String>,
    pub pass: Option<String>,
    pub xaddr: Option<String>,
    pub stream_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Recording {
    pub id: i32,
    pub camera_id: i32,
    pub filename: String,
    pub thumbnail: Option<String>,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub is_finished: bool,
    // Joined fields
    pub camera_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DiscoveredDevice {
    pub address: String,
    pub port: i32,
    pub hostname: String,
    pub name: String,
    pub manufacturer: String,
    pub xaddr: Option<String>,
}

#[allow(non_snake_case)]
#[derive(Debug, Serialize, Deserialize)]
pub struct CameraTimeInfo {
    pub cameraTime: serde_json::Value, // Using Value for flexibility
    pub serverTime: String,
}

#[allow(non_snake_case)]
#[derive(Debug, Serialize, Deserialize)]
pub struct TimeSyncResult {
    pub success: bool,
    pub beforeTime: serde_json::Value,
    pub serverTime: String,
    pub message: String,
    pub error: Option<String>,
}

#[allow(non_snake_case)]
#[derive(Debug, Serialize, Deserialize)]
pub struct PTZCapabilities {
    pub supported: bool,
    pub capabilities: Option<PTZCapabilitiesDetails>,
}

#[allow(non_snake_case)]
#[derive(Debug, Serialize, Deserialize)]
pub struct PTZCapabilitiesDetails {
    pub hasPanTilt: bool,
    pub hasZoom: bool,
}

#[allow(non_snake_case)]
#[derive(Debug, Serialize, Deserialize)]
pub struct PTZMovement {
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub zoom: Option<f32>,
    pub timeout: Option<u64>,
}

#[allow(non_snake_case)]
#[derive(Debug, Serialize, Deserialize)]
pub struct PTZResult {
    pub success: bool,
    pub message: String,
}

#[allow(non_snake_case)]
#[derive(Debug, Serialize, Deserialize)]
pub struct CameraCapabilities {
    pub streaming: bool,
    pub recording: bool,
    pub thumbnails: bool,
    pub ptz: bool,
    pub discovery: bool,
    pub timeSync: bool,
    pub remoteAccess: bool,
}

// Encoder Settings
#[allow(non_snake_case)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncoderSettings {
    pub id: i32,
    pub encoderMode: String,        // "Auto", "GpuOnly", "CpuOnly"
    pub gpuEncoder: Option<String>,  // "h264_nvenc", "h264_qsv", etc.
    pub cpuEncoder: String,          // "libx264" (fallback)
    pub preset: String,              // "ultrafast", "fast", "medium"
    pub quality: i32,                // CRF/CQ value (18-28)
}

impl Default for EncoderSettings {
    fn default() -> Self {
        EncoderSettings {
            id: 1,
            encoderMode: "Auto".to_string(),
            gpuEncoder: None,
            cpuEncoder: "libx264".to_string(),
            preset: "ultrafast".to_string(),
            quality: 23,
        }
    }
}

#[allow(non_snake_case)]
#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateEncoderSettings {
    pub encoderMode: Option<String>,
    pub gpuEncoder: Option<String>,
    pub cpuEncoder: Option<String>,
    pub preset: Option<String>,
    pub quality: Option<i32>,
}

// Recording Schedule
#[allow(non_snake_case)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingSchedule {
    pub id: i32,
    pub camera_id: i32,
    pub name: String,
    pub cron_expression: String,
    pub duration_minutes: i32,
    pub fps: Option<i32>,
    pub is_enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    // Joined fields
    pub camera_name: Option<String>,
}

#[allow(non_snake_case)]
#[derive(Debug, Serialize, Deserialize)]
pub struct NewRecordingSchedule {
    pub camera_id: i32,
    pub name: String,
    pub cron_expression: String,
    pub duration_minutes: i32,
    pub fps: Option<i32>,
    pub is_enabled: bool,
}

#[allow(non_snake_case)]
#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateRecordingSchedule {
    pub name: Option<String>,
    pub cron_expression: Option<String>,
    pub duration_minutes: Option<i32>,
    pub fps: Option<i32>,
    pub is_enabled: Option<bool>,
}
