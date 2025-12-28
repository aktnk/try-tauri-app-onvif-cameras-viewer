pub mod db;
pub mod models;
pub mod commands;
pub mod stream;
pub mod onvif;
pub mod gpu_detector;
pub mod encoder;
pub mod scheduler;
pub mod camera_plugin;
pub mod plugins;

use tauri::Manager;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::process::Child;
use crate::camera_plugin::PluginManager;

pub struct AppState {
    pub db_path: String,
    pub server_port: u16,
    pub stream_dir: PathBuf,
    pub recording_dir: PathBuf,
    // Map<camera_id, ChildProcess>
    // using std::process::Child allows us to kill it later
    pub processes: Arc<Mutex<HashMap<i32, Child>>>,
    pub recording_processes: Arc<Mutex<HashMap<i32, Child>>>,
    pub scheduler: Arc<tokio::sync::Mutex<scheduler::SchedulerManager>>,
    // Map<schedule_id, camera_id> for active scheduled recordings
    pub active_scheduled_recordings: Arc<tokio::sync::Mutex<HashMap<i32, i32>>>,
    pub app_handle: tauri::AppHandle,
    pub plugin_manager: Arc<PluginManager>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_handle = app.handle().clone();
            let app_dir = app.path().app_data_dir().expect("failed to get app data dir");
            std::fs::create_dir_all(&app_dir).expect("failed to create app data dir");

            let db_path = app_dir.join("cameras.db");
            db::init_db(&db_path).expect("failed to init db");

            // Initialize GPU encoder settings after DB is created
            let db_path_clone = db_path.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = db::init_gpu_encoder_settings(&db_path_clone).await {
                    eprintln!("[Init] Failed to initialize GPU encoder settings: {}", e);
                }
            });

            let stream_dir = app_dir.join("streams");
            // Clear old streams on startup
            if stream_dir.exists() {
                std::fs::remove_dir_all(&stream_dir).ok();
            }
            std::fs::create_dir_all(&stream_dir).expect("failed to create streams dir");

            let recording_dir = app_dir.join("recordings");
            std::fs::create_dir_all(&recording_dir).expect("failed to create recordings dir");

            let thumbnails_dir = recording_dir.join("thumbnails");
            std::fs::create_dir_all(&thumbnails_dir).expect("failed to create thumbnails dir");

            // Initialize scheduler
            let scheduler = tauri::async_runtime::block_on(async {
                scheduler::SchedulerManager::new().await
                    .expect("Failed to create scheduler")
            });

            // Initialize plugin manager and register plugins
            let mut plugin_manager = PluginManager::new();
            plugin_manager.register_plugin(Box::new(plugins::OnvifPlugin::new()));
            println!("[Init] Registered camera plugins: {:?}", plugin_manager.get_plugin_types());

            let state = AppState {
                db_path: db_path.to_string_lossy().to_string(),
                server_port: 3333,
                stream_dir: stream_dir.clone(),
                recording_dir: recording_dir.clone(),
                processes: Arc::new(Mutex::new(HashMap::new())),
                recording_processes: Arc::new(Mutex::new(HashMap::new())),
                scheduler: Arc::new(tokio::sync::Mutex::new(scheduler)),
                active_scheduled_recordings: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
                app_handle: app_handle.clone(),
                plugin_manager: Arc::new(plugin_manager),
            };

            // Manage state first
            app.manage(state);

            // Load existing enabled schedules from DB
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = load_enabled_schedules_from_app(app_handle).await {
                    eprintln!("[Init] Failed to load schedules: {}", e);
                }
            });

            // Start Axum server
            tauri::async_runtime::spawn(async move {
                use axum::Router;
                use tower_http::services::ServeDir;
                use tower_http::cors::CorsLayer;
                use std::net::SocketAddr;

                let app = Router::new()
                    .nest_service("/streams", ServeDir::new(stream_dir))
                    .nest_service("/recordings", ServeDir::new(recording_dir))
                    .layer(CorsLayer::permissive()); // Allow all CORS
                
                let addr = SocketAddr::from(([127, 0, 0, 1], 3333));
                let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
                axum::serve(listener, app).await.unwrap();
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                // Clean up all running FFmpeg processes when the window is closing
                if let Some(state) = window.try_state::<AppState>() {
                    println!("[Cleanup] Application is closing, stopping all FFmpeg processes...");

                    // Stop all streaming processes
                    if let Ok(mut processes) = state.processes.lock() {
                        for (camera_id, mut child) in processes.drain() {
                            println!("[Cleanup] Stopping stream for camera {}", camera_id);
                            let _ = child.kill();
                            let _ = child.wait();
                        }
                    }

                    // Stop all recording processes
                    if let Ok(mut recording_processes) = state.recording_processes.lock() {
                        for (camera_id, mut child) in recording_processes.drain() {
                            println!("[Cleanup] Stopping recording for camera {}", camera_id);
                            let _ = child.kill();
                            let _ = child.wait();
                        }
                    }

                    println!("[Cleanup] All FFmpeg processes stopped");
                }
            }
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
            commands::get_camera_capabilities,
            commands::detect_gpu,
            commands::get_encoder_settings,
            commands::update_encoder_settings,
            commands::get_recording_schedules,
            commands::get_recording_cameras,
            commands::add_recording_schedule,
            commands::update_recording_schedule,
            commands::delete_recording_schedule,
            commands::toggle_schedule
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// Helper function to load enabled schedules on startup
async fn load_enabled_schedules_from_app(app_handle: tauri::AppHandle) -> Result<(), String> {
    use rusqlite::Connection;
    use chrono::DateTime;

    println!("[Init] Loading enabled schedules from database...");

    // Get managed state
    let state = app_handle.state::<AppState>();

    let conn = Connection::open(&state.db_path).map_err(|e| e.to_string())?;

    let schedules = {
        let mut stmt = conn.prepare(
            "SELECT s.id, s.camera_id, s.name, s.cron_expression, s.duration_minutes, s.fps, s.is_enabled,
                    s.created_at, s.updated_at, c.name as camera_name
             FROM recording_schedules s
             LEFT JOIN cameras c ON s.camera_id = c.id
             WHERE s.is_enabled = 1"
        ).map_err(|e| e.to_string())?;

        let schedules_iter = stmt.query_map([], |row| {
            Ok(models::RecordingSchedule {
                id: row.get(0)?,
                camera_id: row.get(1)?,
                name: row.get(2)?,
                cron_expression: row.get(3)?,
                duration_minutes: row.get(4)?,
                fps: row.get(5)?,
                is_enabled: row.get(6)?,
                created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(7)?).unwrap_or(chrono::Utc::now().into()).with_timezone(&chrono::Utc),
                updated_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(8)?).unwrap_or(chrono::Utc::now().into()).with_timezone(&chrono::Utc),
                camera_name: row.get(9)?,
                next_run: None, // Not needed for scheduler initialization
            })
        }).map_err(|e| e.to_string())?;

        let mut schedules = Vec::new();
        for schedule in schedules_iter {
            schedules.push(schedule.map_err(|e| e.to_string())?);
        }
        schedules
    };

    // Drop connection before async operations (stmt is already dropped by this point)
    drop(conn);

    // Create Arc<AppState> for scheduler since it expects Arc
    let state_arc = Arc::new(AppState {
        db_path: state.db_path.clone(),
        server_port: state.server_port,
        stream_dir: state.stream_dir.clone(),
        recording_dir: state.recording_dir.clone(),
        processes: state.processes.clone(),
        recording_processes: state.recording_processes.clone(),
        scheduler: state.scheduler.clone(),
        active_scheduled_recordings: state.active_scheduled_recordings.clone(),
        app_handle: state.app_handle.clone(),
        plugin_manager: state.plugin_manager.clone(),
    });

    let scheduler = state.scheduler.lock().await;

    for schedule in schedules {
        println!("[Init] Adding schedule '{}' (ID: {})", schedule.name, schedule.id);
        if let Err(e) = scheduler.add_schedule(schedule.clone(), state_arc.clone()).await {
            eprintln!("[Init] Failed to add schedule '{}': {}", schedule.name, e);
        }
    }

    println!("[Init] Finished loading schedules");

    Ok(())
}