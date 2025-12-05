pub mod db;
pub mod models;
pub mod commands;
pub mod stream;
pub mod onvif;

use tauri::Manager;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::process::Child;

pub struct AppState {
    pub db_path: String,
    pub server_port: u16,
    pub stream_dir: PathBuf,
    // Map<camera_id, ChildProcess>
    // using std::process::Child allows us to kill it later
    pub processes: Arc<Mutex<HashMap<i32, Child>>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_handle = app.handle();
            let app_dir = app.path().app_data_dir().expect("failed to get app data dir");
            std::fs::create_dir_all(&app_dir).expect("failed to create app data dir");
            
            let db_path = app_dir.join("cameras.db");
            db::init_db(&db_path).expect("failed to init db");

            let stream_dir = app_dir.join("streams");
            // Clear old streams on startup
            if stream_dir.exists() {
                std::fs::remove_dir_all(&stream_dir).ok();
            }
            std::fs::create_dir_all(&stream_dir).expect("failed to create streams dir");

            let state = AppState {
                db_path: db_path.to_string_lossy().to_string(),
                server_port: 3333,
                stream_dir: stream_dir.clone(),
                processes: Arc::new(Mutex::new(HashMap::new())),
            };
            
            app.manage(state);

            // Start Axum server
            tauri::async_runtime::spawn(async move {
                use axum::Router;
                use tower_http::services::ServeDir;
                use tower_http::cors::CorsLayer;
                use std::net::SocketAddr;

                let app = Router::new()
                    .nest_service("/streams", ServeDir::new(stream_dir))
                    .layer(CorsLayer::permissive()); // Allow all CORS
                
                let addr = SocketAddr::from(([127, 0, 0, 1], 3333));
                let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
                axum::serve(listener, app).await.unwrap();
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_cameras,
            commands::add_camera,
            commands::delete_camera,
            commands::discover_cameras,
            commands::start_stream,
            commands::stop_stream,
            commands::start_recording,
            commands::stop_recording,
            commands::get_recordings,
            commands::delete_recording,
            commands::get_camera_time,
            commands::sync_camera_time,
            commands::check_ptz_capabilities,
            commands::move_ptz,
            commands::stop_ptz,
            commands::get_camera_capabilities
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}