//! Shared buffer with internal locking
//!
//! Provides a thread-safe buffer that can be shared across async tasks.

use parking_lot::{Mutex, MutexGuard};
use std::ops::Deref;
use std::sync::Arc;

/// Error type for buffer operations
#[derive(Debug)]
pub enum BufferError {
    /// Buffer operation failed (placeholder for future error variants)
    Failed(String),
}

impl std::fmt::Display for BufferError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Failed(msg) => write!(f, "Buffer error: {msg}"),
        }
    }
}

impl std::error::Error for BufferError {}

/// Read-only guard to buffer contents
///
/// Holds the lock and provides read-only access to the underlying data.
/// The lock is released when the guard is dropped.
pub struct BufferReadGuard<'a>(MutexGuard<'a, Vec<u8>>);

impl Deref for BufferReadGuard<'_> {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.0
    }
}

impl AsRef<[u8]> for BufferReadGuard<'_> {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// Shared buffer with internal locking
///
/// A thread-safe buffer backed by `Arc<Mutex<Vec<u8>>>`. Multiple clones
/// share the same underlying data.
///
/// # Thread Safety
///
/// All operations use internal locking via `parking_lot::Mutex`.
/// - `append()` locks, writes, and releases
/// - `lock()` returns a guard that holds the lock until dropped
///
/// # Example
///
/// ```
/// use ailetos::io::Buffer;
///
/// let buffer = Buffer::new();
/// buffer.append(b"hello").unwrap();
///
/// let guard = buffer.lock();
/// assert_eq!(&*guard, b"hello");
/// ```
#[derive(Clone)]
pub struct Buffer(Arc<Mutex<Vec<u8>>>);

impl Buffer {
    /// Create a new empty buffer
    #[must_use]
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(Vec::new())))
    }

    /// Append data to the buffer
    ///
    /// Writes all data or fails. Currently infallible for in-memory buffers,
    /// but returns Result for future compatibility with bounded buffers or
    /// persistent storage.
    ///
    /// # Errors
    ///
    /// Returns `BufferError::Failed` if the write fails (reserved for future use).
    pub fn append(&self, data: &[u8]) -> Result<(), BufferError> {
        let mut buf = self.0.lock();
        buf.extend_from_slice(data);
        Ok(())
    }

    /// Get the current length of the buffer
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.lock().len()
    }

    /// Check if the buffer is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.lock().is_empty()
    }

    /// Lock the buffer for reading
    ///
    /// Returns a read-only guard that provides access to the buffer contents.
    /// The lock is held until the guard is dropped.
    ///
    /// # Example
    ///
    /// ```
    /// use ailetos::io::Buffer;
    ///
    /// let buffer = Buffer::new();
    /// buffer.append(b"hello world").unwrap();
    ///
    /// let guard = buffer.lock();
    /// assert_eq!(&guard[0..5], b"hello");
    /// // Lock released when guard goes out of scope
    /// ```
    #[must_use]
    pub fn lock(&self) -> BufferReadGuard<'_> {
        BufferReadGuard(self.0.lock())
    }
}

impl Default for Buffer {
    fn default() -> Self {
        Self::new()
    }
}
