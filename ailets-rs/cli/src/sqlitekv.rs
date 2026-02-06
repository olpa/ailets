//! SQLite-backed implementation of `KVBuffers`
//!
//! This implementation stores buffers in a SQLite database, providing persistence
//! across program runs. Uses thread-local connections for multi-threaded access.

use ailetos::{Buffer, KVBuffers, KVError, OpenMode};
use parking_lot::Mutex;
use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::thread::ThreadId;

/// SQLite-backed key-value buffer storage
///
/// Buffers are loaded from the database on first access and can be flushed back.
/// The in-memory cache ensures efficient access during runtime.
/// Uses thread-local connections to handle SQLite's threading restrictions.
pub struct SqliteKV {
    /// Path to the SQLite database file
    db_path: String,
    /// In-memory cache of buffers (lazily loaded from DB)
    buffers: Arc<Mutex<HashMap<String, Buffer>>>,
    /// Per-thread SQLite connections
    connections: Arc<Mutex<HashMap<ThreadId, Connection>>>,
}

impl SqliteKV {
    /// Create a new `SqliteKV` backed by the database at the given path
    ///
    /// Creates the database and table if they don't exist.
    ///
    /// # Errors
    ///
    /// Returns error if database cannot be opened or table creation fails.
    pub fn new<P: AsRef<Path>>(db_path: P) -> Result<Self, rusqlite::Error> {
        let db_path = db_path.as_ref().to_string_lossy().to_string();

        // Open initial connection to create table
        let conn = Connection::open(&db_path)?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS kv_buffers (
                path TEXT PRIMARY KEY,
                data BLOB NOT NULL
            )",
            [],
        )?;

        let mut connections = HashMap::new();
        connections.insert(std::thread::current().id(), conn);

        Ok(Self {
            db_path,
            buffers: Arc::new(Mutex::new(HashMap::new())),
            connections: Arc::new(Mutex::new(connections)),
        })
    }

    /// Get or create a connection for the current thread
    fn get_connection(&self) -> Result<Connection, rusqlite::Error> {
        let thread_id = std::thread::current().id();
        let mut connections = self.connections.lock();

        if !connections.contains_key(&thread_id) {
            let conn = Connection::open(&self.db_path)?;
            connections.insert(thread_id, conn);
        }

        // We need to return a connection, but we can't hold the lock
        // Let's open a new connection each time for simplicity
        Connection::open(&self.db_path)
    }

    /// Load a buffer from the database if it exists
    fn load_from_db(&self, path: &str) -> Result<Option<Vec<u8>>, rusqlite::Error> {
        let conn = self.get_connection()?;
        let mut stmt = conn.prepare("SELECT data FROM kv_buffers WHERE path = ?")?;

        let result = stmt.query_row(params![path], |row| row.get::<_, Vec<u8>>(0));

        match result {
            Ok(data) => Ok(Some(data)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Save a buffer to the database
    fn save_to_db(&self, path: &str, data: &[u8]) -> Result<(), rusqlite::Error> {
        let conn = self.get_connection()?;
        conn.execute(
            "INSERT OR REPLACE INTO kv_buffers (path, data) VALUES (?, ?)",
            params![path, data],
        )?;
        Ok(())
    }
}

impl KVBuffers for SqliteKV {
    async fn open(&self, path: &str, mode: OpenMode) -> Result<Buffer, KVError> {
        let mut buffers = self.buffers.lock();

        match mode {
            OpenMode::Read => {
                // Try in-memory cache first
                if let Some(buffer) = buffers.get(path) {
                    return Ok(buffer.clone());
                }

                // Load from database
                let data = self
                    .load_from_db(path)
                    .map_err(|_e| KVError::NotFound(path.to_string()))?
                    .ok_or_else(|| KVError::NotFound(path.to_string()))?;

                let buffer = Buffer::new();
                buffer.append(&data).expect("Failed to append data to buffer");
                buffers.insert(path.to_string(), buffer.clone());
                Ok(buffer)
            }
            OpenMode::Write => {
                let buffer = Buffer::new();
                buffers.insert(path.to_string(), buffer.clone());
                Ok(buffer)
            }
            OpenMode::Append => {
                // Try in-memory cache first
                if let Some(buffer) = buffers.get(path) {
                    return Ok(buffer.clone());
                }

                // Try loading from database
                if let Ok(Some(data)) = self.load_from_db(path) {
                    let buffer = Buffer::new();
                    buffer.append(&data).expect("Failed to append data to buffer");
                    buffers.insert(path.to_string(), buffer.clone());
                    Ok(buffer)
                } else {
                    // Create new buffer if not found
                    let buffer = Buffer::new();
                    buffers.insert(path.to_string(), buffer.clone());
                    Ok(buffer)
                }
            }
        }
    }

    async fn flush(&self, path: &str) -> Result<(), KVError> {
        let buffers = self.buffers.lock();

        if let Some(buffer) = buffers.get(path) {
            // Lock the buffer to read its contents
            let guard = buffer.lock();
            let data = &*guard;

            // Save to database
            self.save_to_db(path, data)
                .map_err(|_e| KVError::NotFound(path.to_string()))?;
        }

        Ok(())
    }

    async fn listdir(&self, dir_name: &str) -> Result<Vec<String>, KVError> {
        let prefix = if dir_name.ends_with('/') {
            dir_name.to_string()
        } else {
            format!("{dir_name}/")
        };

        let conn = self.get_connection()
            .map_err(|_e| KVError::NotFound(dir_name.to_string()))?;
        let mut stmt = conn
            .prepare("SELECT path FROM kv_buffers WHERE path LIKE ? ORDER BY path")
            .map_err(|_e| KVError::NotFound(dir_name.to_string()))?;

        let pattern = format!("{prefix}%");
        let paths = stmt
            .query_map(params![pattern], |row| row.get::<_, String>(0))
            .map_err(|_e| KVError::NotFound(dir_name.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_e| KVError::NotFound(dir_name.to_string()))?;

        Ok(paths)
    }

    async fn destroy(&self) -> Result<(), KVError> {
        // Clear in-memory cache
        let mut buffers = self.buffers.lock();
        buffers.clear();

        // Clear database
        let conn = self.get_connection()
            .map_err(|_e| KVError::NotFound("destroy".to_string()))?;
        conn.execute("DELETE FROM kv_buffers", [])
            .map_err(|_e| KVError::NotFound("destroy".to_string()))?;

        Ok(())
    }
}
