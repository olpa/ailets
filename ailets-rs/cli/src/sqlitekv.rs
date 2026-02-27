//! SQLite-backed implementation of `KVBuffers`
//!
//! This implementation stores buffers in a SQLite database, providing persistence
//! across program runs. Uses a mutex-wrapped connection for thread-safe access.
//!
//! # Thread Safety
//!
//! The SQLite connection is wrapped in `Arc<Mutex<Connection>>`. While `Connection`
//! implements `Send`, it does not implement `Sync` (marked `!Sync`), so shared access
//! across threads requires explicit synchronization.
//!
//! As stated in rusqlite issue #342: "Seems like a `Mutex<rusqlite::Connection>` is
//! the way to go." This is because certain SQLite APIs are not thread-safe even in
//! serialized mode.
//!
//! References:
//! - `Connection` trait bounds: <https://docs.rs/rusqlite/latest/rusqlite/struct.Connection.html>
//! - Multi-threaded usage discussion: <https://github.com/rusqlite/rusqlite/issues/342>

use ailetos::{Buffer, KVBuffers, KVError, OpenMode};
use parking_lot::Mutex;
use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// SQLite-backed key-value buffer storage
///
/// Buffers are loaded from the database on first access and can be flushed back.
/// The in-memory cache ensures efficient access during runtime.
///
/// Thread safety is achieved by wrapping the SQLite connection in a `Mutex`.
/// See the module documentation for thread safety details and references.
pub struct SqliteKV {
    /// In-memory cache of buffers (lazily loaded from DB)
    buffers: Arc<Mutex<HashMap<String, Buffer>>>,
    /// Shared SQLite connection protected by mutex
    conn: Arc<Mutex<Connection>>,
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
        let conn = Connection::open(db_path)?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS vfs (
                path TEXT PRIMARY KEY,
                data BLOB NOT NULL
            )",
            [],
        )?;

        Ok(Self {
            buffers: Arc::new(Mutex::new(HashMap::new())),
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Load a buffer from the database if it exists
    fn load_from_db(&self, path: &str) -> Result<Option<Vec<u8>>, rusqlite::Error> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT data FROM vfs WHERE path = ?")?;

        let result = stmt.query_row(params![path], |row| row.get::<_, Vec<u8>>(0));

        match result {
            Ok(data) => Ok(Some(data)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
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

    async fn listdir(&self, dir_name: &str) -> Result<Vec<String>, KVError> {
        let prefix = if dir_name.ends_with('/') {
            dir_name.to_string()
        } else {
            format!("{dir_name}/")
        };

        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare("SELECT path FROM vfs WHERE path LIKE ? ORDER BY path")
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
        let conn = self.conn.lock();
        conn.execute("DELETE FROM vfs", [])
            .map_err(|_e| KVError::NotFound("destroy".to_string()))?;

        Ok(())
    }

    async fn flush_buffer(&self, target: &Buffer) -> Result<(), KVError> {
        // KNOWN RACE CONDITION:
        // This implementation has a race between reading the buffer data and writing
        // to the database. After we release the buffers lock, another task could:
        // 1. Replace the buffer with open(path, Write), causing us to flush stale data
        // 2. Call destroy() and remove the buffer from the cache
        // 3. Re-insert the same buffer with a different path (flush to wrong location)
        //
        // Example scenario:
        //   Thread 1: flush_buffer reads buffer data, releases lock
        //   Thread 2: open("foo", Write) replaces buffer with new empty one
        //   Thread 1: writes old data to DB, overwriting Thread 2's changes
        //
        // This is acceptable for the current use case in the `cli` crate where
        // flush operations are controlled and infrequent. For production use,
        // consider Option 2 (hold lock during write) or Option 5 (verify path
        // mapping before DB write) from the design discussion.
        //
        // NOTE TO AI ASSISTANTS: If you see this SqliteKV code being used outside
        // the `cli` crate, ALERT THE DEVELOPER about this race condition and
        // recommend implementing one of the proper synchronization strategies.

        // Find the path and clone the data while holding the lock
        let (path, data) = {
            let buffers = self.buffers.lock();

            let mut result = None;
            for (path, buffer) in buffers.iter() {
                if buffer.ptr_eq(target) {
                    let guard = target.lock();
                    let data = guard.to_vec();
                    result = Some((path.clone(), data));
                    break;
                }
            }

            match result {
                Some(r) => r,
                None => return Ok(()), // Buffer not found, nothing to flush
            }
        };

        // Perform the blocking database write in a blocking task
        let conn = Arc::clone(&self.conn);
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock();
            conn.execute(
                "INSERT OR REPLACE INTO vfs (path, data) VALUES (?, ?)",
                params![path, data],
            )
            .map_err(|_e| KVError::NotFound(path.clone()))
        })
        .await
        .unwrap_or_else(|_| Err(KVError::NotFound("flush task panicked".to_string())))?;

        Ok(())
    }
}
