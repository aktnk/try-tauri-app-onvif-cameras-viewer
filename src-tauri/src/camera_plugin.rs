use crate::models::Camera;
use async_trait::async_trait;
use std::collections::HashMap;

/// Information about a discovered camera
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CameraInfo {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub camera_type: String,
    pub user: Option<String>,
    pub pass: Option<String>,
    // UVC-specific fields
    pub device_path: Option<String>,      // Linux: /dev/video0
    pub device_id: Option<String>,        // Windows: device GUID
    pub device_index: Option<u32>,        // macOS: AVFoundation index
}

/// PTZ movement direction
#[derive(Debug, Clone)]
pub enum PtzDirection {
    Up,
    Down,
    Left,
    Right,
    ZoomIn,
    ZoomOut,
}

/// Recording options
#[derive(Debug, Clone)]
pub struct RecordingOptions {
    pub duration_minutes: Option<u32>,
    pub fps: Option<u32>,
}

/// Camera plugin trait
/// Each camera type (ONVIF, UVC, etc.) implements this trait
#[async_trait]
pub trait CameraPlugin: Send + Sync {
    /// Returns the plugin type identifier (e.g., "onvif", "uvc")
    fn plugin_type(&self) -> &str;

    /// Discover cameras of this type on the network/system
    async fn discover(&self) -> Result<Vec<CameraInfo>, String>;

    /// Get the stream URL for a camera
    /// For ONVIF: RTSP URL
    /// For UVC: device path (e.g., /dev/video0)
    async fn get_stream_url(&self, camera: &Camera) -> Result<String, String>;

    /// Check if this plugin supports PTZ control
    fn supports_ptz(&self) -> bool {
        false
    }

    /// Check if this plugin supports time synchronization
    fn supports_time_sync(&self) -> bool {
        false
    }

    /// Move PTZ camera (only if supports_ptz() returns true)
    async fn ptz_move(
        &self,
        _camera: &Camera,
        _direction: PtzDirection,
        _duration_ms: u32,
    ) -> Result<(), String> {
        Err("PTZ not supported by this plugin".to_string())
    }

    /// Stop PTZ movement (only if supports_ptz() returns true)
    async fn ptz_stop(&self, _camera: &Camera) -> Result<(), String> {
        Err("PTZ not supported by this plugin".to_string())
    }

    /// Get camera's current time (only if supports_time_sync() returns true)
    async fn get_camera_time(&self, _camera: &Camera) -> Result<chrono::DateTime<chrono::Utc>, String> {
        Err("Time sync not supported by this plugin".to_string())
    }

    /// Set camera's time (only if supports_time_sync() returns true)
    async fn set_camera_time(
        &self,
        _camera: &Camera,
        _time: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), String> {
        Err("Time sync not supported by this plugin".to_string())
    }

    /// Get ONVIF profiles (only for ONVIF cameras)
    async fn get_profiles(&self, _camera: &Camera) -> Result<Vec<(String, String)>, String> {
        Err("Profiles not supported by this plugin".to_string())
    }
}

/// Plugin manager that manages all camera plugins
pub struct PluginManager {
    plugins: HashMap<String, Box<dyn CameraPlugin>>,
}

impl PluginManager {
    /// Create a new plugin manager
    pub fn new() -> Self {
        let plugins: HashMap<String, Box<dyn CameraPlugin>> = HashMap::new();
        Self { plugins }
    }

    /// Register a camera plugin
    pub fn register_plugin(&mut self, plugin: Box<dyn CameraPlugin>) {
        let plugin_type = plugin.plugin_type().to_string();
        println!("[PluginManager] Registering plugin: {}", plugin_type);
        self.plugins.insert(plugin_type, plugin);
    }

    /// Get a plugin by type
    pub fn get_plugin(&self, camera_type: &str) -> Option<&Box<dyn CameraPlugin>> {
        self.plugins.get(camera_type)
    }

    /// Discover all cameras from all plugins
    pub async fn discover_all(&self) -> Result<Vec<CameraInfo>, String> {
        let mut all_cameras = Vec::new();

        for (plugin_type, plugin) in &self.plugins {
            println!("[PluginManager] Discovering cameras from plugin: {}", plugin_type);
            match plugin.discover().await {
                Ok(cameras) => {
                    println!(
                        "[PluginManager] Plugin '{}' found {} camera(s)",
                        plugin_type,
                        cameras.len()
                    );
                    all_cameras.extend(cameras);
                }
                Err(e) => {
                    println!(
                        "[PluginManager] Plugin '{}' discovery failed: {}",
                        plugin_type, e
                    );
                }
            }
        }

        Ok(all_cameras)
    }

    /// Get list of registered plugin types
    pub fn get_plugin_types(&self) -> Vec<String> {
        self.plugins.keys().cloned().collect()
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}
