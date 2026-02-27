//! Key-value storage types and traits

use super::buffer::Buffer;
use std::future::Future;

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

/// Errors that can occur in KV operations
#[derive(Debug)]
pub enum KVError {
    /// Path was not found in the store
    NotFound(String),
}

impl std::fmt::Display for KVError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(path) => write!(f, "Path not found: {path}"),
        }
    }
}

impl std::error::Error for KVError {}

/// Trait for key-value buffer storage backends
///
/// Provides async operations for storing and retrieving buffers.
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
    ) -> impl Future<Output = Result<Buffer, KVError>> + Send;

    /// List paths with given prefix.
    ///
    /// If the prefix does not end with '/', one is added for matching.
    fn listdir(&self, dir_name: &str) -> impl Future<Output = Result<Vec<String>, KVError>> + Send;

    /// Clear all buffers.
    fn destroy(&self) -> impl Future<Output = Result<(), KVError>> + Send;

    /// Flush a buffer to persistent storage (if applicable).
    ///
    /// For in-memory implementations, this is a no-op.
    /// For persistent implementations (e.g., SQLite), this writes the buffer to storage.
    fn flush_buffer(
        &self,
        buffer: &Buffer,
    ) -> impl Future<Output = Result<(), KVError>> + Send;
}
