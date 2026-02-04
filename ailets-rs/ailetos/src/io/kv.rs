//! Key-value buffer storage layer
//!
//! Provides async key-value storage operations with multiple backend support.
//! This is the storage layer that backs the Pipe coordination layer.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────┐
//! │  Pipe (coordination layer)          │
//! │  - notification queue               │
//! │  - async readers waiting for writers│
//! │  - Buffer trait (NO kv imports)     │
//! └─────────────────────────────────────┘
//!          ▲
//!          │ integration layer (future)
//!          │ implements Buffer for shared storage
//!          ▼
//! ┌─────────────────────────────────────┐
//! │  KV (storage layer)                 │
//! │  - simple async storage operations  │
//! │  - returns Arc<Mutex<Vec<u8>>>      │
//! │  - multiple backends                │
//! └─────────────────────────────────────┘
//!      ▲         ▲           ▲
//!      │         │           │
//!   MemKV    SQLiteKV    DynamoKV
//! ```

use parking_lot::Mutex;
use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::sync::Arc;

/// Mode for opening a buffer
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenMode {
    /// Read existing buffer, error if not found
    Read,
    /// Create new empty buffer (overwrites if exists)
    Write,
    /// Get existing or create new buffer
    Append,
}

/// A handle to a buffer in the KV store
pub type KVBuffer = Arc<Mutex<Vec<u8>>>;

/// Errors that can occur in KV operations
#[derive(Debug)]
pub enum KVError {
    /// Path was not found in the store
    NotFound(String),
}

impl fmt::Display for KVError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(path) => write!(f, "Path not found: {path}"),
        }
    }
}

impl std::error::Error for KVError {}

/// Trait for key-value buffer storage backends
///
/// Provides async operations for storing and retrieving byte buffers.
/// Each buffer is identified by a string path.
pub trait KVBuffers: Send + Sync {
    /// Open a buffer at path with given mode.
    ///
    /// - Read: returns existing buffer, error if not found
    /// - Write: creates new empty buffer (overwrites if exists)
    /// - Append: gets existing or creates new buffer
    fn open(
        &self,
        path: &str,
        mode: OpenMode,
    ) -> impl Future<Output = Result<KVBuffer, KVError>> + Send;

    /// Flush buffer to backing store.
    ///
    /// No-op for in-memory implementations like MemKV.
    fn flush(&self, path: &str) -> impl Future<Output = Result<(), KVError>> + Send;

    /// List paths with given prefix.
    ///
    /// If the prefix does not end with '/', one is added for matching.
    fn listdir(&self, dir_name: &str) -> impl Future<Output = Result<Vec<String>, KVError>> + Send;

    /// Clear all buffers.
    fn destroy(&self) -> impl Future<Output = Result<(), KVError>> + Send;
}

/// In-memory implementation of KVBuffers
///
/// Simple hash map based storage, useful for testing and single-process use.
pub struct MemKV {
    buffers: Mutex<HashMap<String, KVBuffer>>,
}

impl MemKV {
    /// Create a new empty MemKV store
    #[must_use]
    pub fn new() -> Self {
        Self {
            buffers: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for MemKV {
    fn default() -> Self {
        Self::new()
    }
}

impl KVBuffers for MemKV {
    async fn open(&self, path: &str, mode: OpenMode) -> Result<KVBuffer, KVError> {
        let mut buffers = self.buffers.lock();

        match mode {
            OpenMode::Read => buffers
                .get(path)
                .cloned()
                .ok_or_else(|| KVError::NotFound(path.to_string())),
            OpenMode::Write => {
                let buffer = Arc::new(Mutex::new(Vec::new()));
                buffers.insert(path.to_string(), Arc::clone(&buffer));
                Ok(buffer)
            }
            OpenMode::Append => {
                if let Some(buffer) = buffers.get(path) {
                    Ok(Arc::clone(buffer))
                } else {
                    let buffer = Arc::new(Mutex::new(Vec::new()));
                    buffers.insert(path.to_string(), Arc::clone(&buffer));
                    Ok(buffer)
                }
            }
        }
    }

    async fn flush(&self, _path: &str) -> Result<(), KVError> {
        // No-op for in-memory storage
        Ok(())
    }

    async fn listdir(&self, dir_name: &str) -> Result<Vec<String>, KVError> {
        let prefix = if dir_name.ends_with('/') {
            dir_name.to_string()
        } else {
            format!("{dir_name}/")
        };

        let buffers = self.buffers.lock();
        let mut paths: Vec<String> = buffers
            .keys()
            .filter(|path| path.starts_with(&prefix))
            .cloned()
            .collect();
        paths.sort();
        Ok(paths)
    }

    async fn destroy(&self) -> Result<(), KVError> {
        let mut buffers = self.buffers.lock();
        buffers.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_open_write_then_read() {
        let kv = MemKV::new();

        // Write creates a new buffer
        let buffer = kv.open("test/path", OpenMode::Write).await.unwrap();
        {
            let mut data = buffer.lock();
            data.extend_from_slice(b"hello world");
        }

        // Read returns the same buffer
        let buffer2 = kv.open("test/path", OpenMode::Read).await.unwrap();
        let data = buffer2.lock();
        assert_eq!(data.as_slice(), b"hello world");
    }

    #[tokio::test]
    async fn test_open_append() {
        let kv = MemKV::new();

        // Append creates new buffer if not exists
        let buffer = kv.open("test/path", OpenMode::Append).await.unwrap();
        {
            let mut data = buffer.lock();
            data.extend_from_slice(b"first");
        }

        // Append returns existing buffer
        let buffer2 = kv.open("test/path", OpenMode::Append).await.unwrap();
        {
            let mut data = buffer2.lock();
            data.extend_from_slice(b" second");
        }

        // Verify both writes are in the buffer
        let data = buffer.lock();
        assert_eq!(data.as_slice(), b"first second");
    }

    #[tokio::test]
    async fn test_open_read_not_found() {
        let kv = MemKV::new();

        let result = kv.open("nonexistent", OpenMode::Read).await;
        assert!(result.is_err());
        match result {
            Err(KVError::NotFound(path)) => assert_eq!(path, "nonexistent"),
            Ok(_) => panic!("Expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn test_open_write_overwrites() {
        let kv = MemKV::new();

        // Write initial data
        let buffer = kv.open("test/path", OpenMode::Write).await.unwrap();
        {
            let mut data = buffer.lock();
            data.extend_from_slice(b"initial data");
        }

        // Write again overwrites
        let buffer2 = kv.open("test/path", OpenMode::Write).await.unwrap();
        {
            let data = buffer2.lock();
            assert!(data.is_empty(), "Write mode should create empty buffer");
        }
    }

    #[tokio::test]
    async fn test_listdir() {
        let kv = MemKV::new();

        // Create some paths
        kv.open("dir1/file1", OpenMode::Write).await.unwrap();
        kv.open("dir1/file2", OpenMode::Write).await.unwrap();
        kv.open("dir2/file1", OpenMode::Write).await.unwrap();

        let paths = kv.listdir("dir1/").await.unwrap();
        assert_eq!(paths, vec!["dir1/file1", "dir1/file2"]);
    }

    #[tokio::test]
    async fn test_listdir_adds_slash() {
        let kv = MemKV::new();

        // Create some paths
        kv.open("dir1/file1", OpenMode::Write).await.unwrap();
        kv.open("dir1/file2", OpenMode::Write).await.unwrap();
        kv.open("dir11/file1", OpenMode::Write).await.unwrap();

        // listdir without trailing slash should still work correctly
        let paths = kv.listdir("dir1").await.unwrap();
        assert_eq!(paths, vec!["dir1/file1", "dir1/file2"]);

        // dir11 should not be included
        assert!(!paths.contains(&"dir11/file1".to_string()));
    }

    #[tokio::test]
    async fn test_destroy() {
        let kv = MemKV::new();

        // Create some paths
        kv.open("path1", OpenMode::Write).await.unwrap();
        kv.open("path2", OpenMode::Write).await.unwrap();

        // Destroy clears all
        kv.destroy().await.unwrap();

        // Verify paths are gone
        assert!(kv.open("path1", OpenMode::Read).await.is_err());
        assert!(kv.open("path2", OpenMode::Read).await.is_err());
    }
}
