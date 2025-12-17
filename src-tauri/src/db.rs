use rusqlite::{Connection, Result};
use std::path::Path;
use std::fs;
use crate::gpu_detector;

pub fn init_db<P: AsRef<Path>>(path: P) -> Result<()> {
    if let Some(parent) = path.as_ref().parent() {
        fs::create_dir_all(parent).ok();
    }

    let conn = Connection::open(path)?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS cameras (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            type TEXT NOT NULL DEFAULT 'onvif',
            host TEXT NOT NULL,
            port INTEGER NOT NULL,
            user TEXT,
            pass TEXT,
            xaddr TEXT,
            stream_path TEXT,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT DEFAULT CURRENT_TIMESTAMP
        )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS recordings (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            camera_id INTEGER NOT NULL,
            filename TEXT NOT NULL,
            thumbnail TEXT,
            start_time TEXT NOT NULL,
            end_time TEXT,
            is_finished BOOLEAN DEFAULT 0,
            FOREIGN KEY(camera_id) REFERENCES cameras(id) ON DELETE CASCADE
        )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS encoder_settings (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            encoder_mode TEXT NOT NULL DEFAULT 'Auto',
            gpu_encoder TEXT,
            cpu_encoder TEXT NOT NULL DEFAULT 'libx264',
            preset TEXT NOT NULL DEFAULT 'ultrafast',
            quality INTEGER NOT NULL DEFAULT 23
        )",
        [],
    )?;

    // Insert default encoder settings if not exists
    conn.execute(
        "INSERT OR IGNORE INTO encoder_settings (id, encoder_mode, gpu_encoder, cpu_encoder, preset, quality)
         VALUES (1, 'Auto', NULL, 'libx264', 'ultrafast', 23)",
        [],
    )?;

    Ok(())
}

/// Initialize GPU encoder settings by detecting available hardware
pub async fn init_gpu_encoder_settings<P: AsRef<Path>>(path: P) -> Result<(), String> {
    println!("[Init] Initializing GPU encoder settings...");

    // Detect GPU capabilities
    let capabilities = gpu_detector::detect_gpu_capabilities().await
        .map_err(|e| format!("Failed to detect GPU: {}", e))?;

    // Only update if a preferred encoder was found
    if let Some(preferred_encoder) = capabilities.preferredEncoder {
        println!("[Init] Found GPU encoder: {}", preferred_encoder);

        let conn = Connection::open(path)
            .map_err(|e| format!("Failed to open database: {}", e))?;

        // Update the encoder settings only if gpu_encoder is NULL
        conn.execute(
            "UPDATE encoder_settings SET gpu_encoder = ?1 WHERE id = 1 AND gpu_encoder IS NULL",
            [&preferred_encoder],
        ).map_err(|e| format!("Failed to update encoder settings: {}", e))?;

        println!("[Init] GPU encoder settings initialized: {}", preferred_encoder);
    } else {
        println!("[Init] No GPU encoder found, keeping CPU-only mode");
    }

    Ok(())
}
