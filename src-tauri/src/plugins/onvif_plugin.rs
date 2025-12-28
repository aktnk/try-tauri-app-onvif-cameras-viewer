use crate::camera_plugin::{CameraInfo, CameraPlugin, PtzDirection};
use crate::models::Camera;
use async_trait::async_trait;
use chrono::Utc;
use reqwest::Client;
use std::time::Duration;

// Re-export ONVIF module functions for existing code compatibility
pub use crate::onvif::*;

/// ONVIF camera plugin implementation
pub struct OnvifPlugin;

impl OnvifPlugin {
    pub fn new() -> Self {
        OnvifPlugin
    }
}

#[async_trait]
impl CameraPlugin for OnvifPlugin {
    fn plugin_type(&self) -> &str {
        "onvif"
    }

    async fn discover(&self) -> Result<Vec<CameraInfo>, String> {
        println!("[OnvifPlugin] Starting ONVIF camera discovery...");

        // Use existing ONVIF discovery function
        let devices = crate::onvif::discover_devices().await?;

        // Convert DiscoveredDevice to CameraInfo
        let cameras: Vec<CameraInfo> = devices
            .into_iter()
            .map(|device| CameraInfo {
                name: device.name,
                host: device.address,
                port: device.port as u16,
                camera_type: "onvif".to_string(),
                user: None,
                pass: None,
                device_path: None,
                device_id: None,
                device_index: None,
            })
            .collect();

        println!("[OnvifPlugin] Found {} ONVIF camera(s)", cameras.len());
        Ok(cameras)
    }

    async fn get_stream_url(&self, camera: &Camera) -> Result<String, String> {
        println!("[OnvifPlugin] Getting stream URL for camera: {}", camera.name);

        // Use existing ONVIF stream URL retrieval
        crate::onvif::get_onvif_stream_url(camera).await
    }

    fn supports_ptz(&self) -> bool {
        true
    }

    fn supports_time_sync(&self) -> bool {
        true
    }

    async fn ptz_move(
        &self,
        camera: &Camera,
        direction: PtzDirection,
        duration_ms: u32,
    ) -> Result<(), String> {
        println!(
            "[OnvifPlugin] PTZ move: camera={}, direction={:?}, duration={}ms",
            camera.name, direction, duration_ms
        );

        // Convert PtzDirection to velocity values
        let (x, y, zoom) = match direction {
            PtzDirection::Up => (0.0, 0.5, 0.0),
            PtzDirection::Down => (0.0, -0.5, 0.0),
            PtzDirection::Left => (-0.5, 0.0, 0.0),
            PtzDirection::Right => (0.5, 0.0, 0.0),
            PtzDirection::ZoomIn => (0.0, 0.0, 0.5),
            PtzDirection::ZoomOut => (0.0, 0.0, -0.5),
        };

        // Use existing ONVIF continuous move function
        crate::onvif::continuous_move(camera, x, y, zoom).await
    }

    async fn ptz_stop(&self, camera: &Camera) -> Result<(), String> {
        println!("[OnvifPlugin] Stopping PTZ movement for camera: {}", camera.name);

        // Use existing ONVIF stop function
        crate::onvif::stop_move(camera).await
    }

    async fn get_camera_time(&self, camera: &Camera) -> Result<chrono::DateTime<Utc>, String> {
        println!("[OnvifPlugin] Getting camera time: {}", camera.name);

        // Use existing ONVIF get time function
        let onvif_dt = crate::onvif::get_system_date_time(camera).await?;

        // Convert ONVIFDateTime to chrono::DateTime
        onvif_dt
            .to_chrono()
            .ok_or_else(|| "Failed to convert ONVIF time to chrono DateTime".to_string())
    }

    async fn set_camera_time(
        &self,
        camera: &Camera,
        time: chrono::DateTime<Utc>,
    ) -> Result<(), String> {
        println!(
            "[OnvifPlugin] Setting camera time: camera={}, time={}",
            camera.name, time
        );

        // Convert chrono::DateTime to ONVIFDateTime
        let onvif_dt = crate::onvif::ONVIFDateTime::from_chrono(&time);

        // Use existing ONVIF set time function
        crate::onvif::set_system_date_time(camera, &onvif_dt).await
    }

    async fn get_profiles(&self, camera: &Camera) -> Result<Vec<(String, String)>, String> {
        println!("[OnvifPlugin] Getting profiles for camera: {}", camera.name);

        let xaddr = camera
            .xaddr
            .clone()
            .ok_or("No xAddr available for ONVIF camera")?;
        let user = camera.user.clone().unwrap_or_default();
        let pass = camera.pass.clone().unwrap_or_default();

        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .danger_accept_invalid_certs(true)
            .build()
            .map_err(|e| e.to_string())?;

        // GetProfiles
        let profiles_body = r###"<GetProfiles xmlns="http://www.onvif.org/ver10/media/wsdl"/>"###;
        let profiles_envelope = crate::onvif::build_soap_envelope(&user, &pass, profiles_body);

        let profiles_res = client
            .post(&xaddr)
            .header(
                "Content-Type",
                "application/soap+xml; charset=utf-8; action=\"http://www.onvif.org/ver10/media/wsdl/GetProfiles\"",
            )
            .body(profiles_envelope)
            .send()
            .await
            .map_err(|e| format!("Failed to GetProfiles: {}", e))?;

        let profiles_xml = profiles_res.text().await.map_err(|e| e.to_string())?;

        // Parse all profiles (simplified version - returns token only)
        // In a full implementation, you would parse profile names and tokens
        let mut profiles = Vec::new();

        // For now, return a simple result indicating success
        if let Some(token) = parse_first_profile_token(&profiles_xml) {
            profiles.push(("MainProfile".to_string(), token));
        }

        Ok(profiles)
    }
}

// Helper function to parse profile token
fn parse_first_profile_token(xml: &str) -> Option<String> {
    use regex::Regex;

    let re = Regex::new(r#"(?s)<[^>]*:Profiles[^>]*\stoken="([^"]+)""#).ok()?;

    if let Some(caps) = re.captures(xml) {
        return Some(caps[1].to_string());
    }

    None
}
