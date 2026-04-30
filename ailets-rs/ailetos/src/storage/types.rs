//! Key-value storage types and traits

use async_trait::async_trait;
use super::buffer::Buffer;

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
    /// Buffer operation failed
    BufferError(super::buffer::BufferError),
    /// Attempted to create a duplicate resource
    AlreadyExists(String),
}

impl std::fmt::Display for KVError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(path) => write!(f, "Path not found: {path}"),
            Self::BufferError(e) => write!(f, "Buffer error: {e}"),
            Self::AlreadyExists(msg) => write!(f, "Already exists: {msg}"),
        }
    }
}

impl std::error::Error for KVError {}

impl From<super::buffer::BufferError> for KVError {
    fn from(e: super::buffer::BufferError) -> Self {
        Self::BufferError(e)
    }
}

/// Trait for key-value buffer storage backends
///
/// Provides async operations for storing and retrieving buffers.
/// Each buffer is identified by a string path.
#[async_trait]
pub trait KVBuffers: Send + Sync {
    /// Open a buffer at path with given mode.
    ///
    /// - Read: returns existing buffer, error if not found
    /// - Write: creates new empty buffer (overwrites if exists)
    /// - Append: gets existing or creates new buffer
    async fn open(&self, path: &str, mode: OpenMode) -> Result<Buffer, KVError>;

    /// List paths with given prefix.
    ///
    /// If the prefix does not end with '/', one is added for matching.
    async fn listdir(&self, dir_name: &str) -> Result<Vec<String>, KVError>;

    /// Clear all buffers.
    async fn destroy(&self) -> Result<(), KVError>;

    /// Flush a buffer to persistent storage (if applicable).
    ///
    /// For in-memory implementations, this is a no-op.
    /// For persistent implementations (e.g., `SQLite`), this writes the buffer to storage.
    async fn flush_buffer(&self, buffer: &Buffer) -> Result<(), KVError>;
}
