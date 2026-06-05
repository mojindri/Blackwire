use std::{
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard},
};

use anyhow::Result;
use rusqlite::Connection;

use crate::{db, error::AppError};

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
}

impl AppState {
    /// Acquire the database lock, converting a poison error into a 500 response.
    pub fn lock_db(&self) -> Result<MutexGuard<'_, Connection>, AppError> {
        self.db
            .lock()
            .map_err(|_| AppError::internal(anyhow::anyhow!("db lock poisoned")))
    }

    pub fn open() -> Result<Self> {
        let data_dir = std::env::var("BLACK_UI_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("black-ui/data"));
        std::fs::create_dir_all(&data_dir)?;
        let conn = Connection::open(data_dir.join("black-ui.db"))?;
        db::init(&conn, &data_dir)?;
        Ok(Self {
            db: Arc::new(Mutex::new(conn)),
        })
    }
}
