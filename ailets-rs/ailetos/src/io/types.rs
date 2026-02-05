//! Key-value buffer storage types and traits

use parking_lot::Mutex;
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
