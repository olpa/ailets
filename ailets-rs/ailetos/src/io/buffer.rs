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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_buffer_is_empty() {
        let buffer = Buffer::new();
        assert!(buffer.is_empty());
        assert_eq!(buffer.len(), 0);
    }

    #[test]
    fn test_append_and_read() {
        let buffer = Buffer::new();
        buffer.append(b"hello").unwrap();
        buffer.append(b" world").unwrap();

        let guard = buffer.lock();
        assert_eq!(&*guard, b"hello world");
    }

    #[test]
    fn test_clone_shares_data() {
        let buffer1 = Buffer::new();
        let buffer2 = buffer1.clone();

        buffer1.append(b"from buffer1").unwrap();

        let guard = buffer2.lock();
        assert_eq!(&*guard, b"from buffer1");
    }

    #[test]
    fn test_read_guard_deref() {
        let buffer = Buffer::new();
        buffer.append(b"test data").unwrap();

        let guard = buffer.lock();
        // Test Deref
        assert_eq!(guard.len(), 9);
        assert_eq!(&guard[0..4], b"test");
    }

    #[test]
    fn test_read_guard_as_ref() {
        let buffer = Buffer::new();
        buffer.append(b"test").unwrap();

        let guard = buffer.lock();
        let slice: &[u8] = guard.as_ref();
        assert_eq!(slice, b"test");
    }
}
