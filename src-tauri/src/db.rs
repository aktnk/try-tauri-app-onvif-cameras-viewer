use rusqlite::{Connection, Result};
use std::path::Path;
use std::fs;

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

    Ok(())
}
