//! Writer side of the memory pipe

use parking_lot::Mutex;
use std::cmp::Ordering;
use std::fmt;
use std::sync::Arc;
use tracing::{error, trace};

use crate::idgen::Handle;
use crate::notification_queue::NotificationQueueArc;
use crate::storage::Buffer;

use super::rw_shared::{ReaderSharedData, SharedBuffer};

/// Writer side of the memory pipe
///
/// Writes append to the shared buffer and notify waiting readers.
///
/// # Thread Safety
///
/// Writer is thread-safe and can be shared between threads (via Arc or references).
/// All write operations use interior mutability with `parking_lot::Mutex` protection.
///
/// - **Thread-safe**: Multiple threads can call `write()`, `write()`, and other
///   methods concurrently. The internal Mutex serializes access to shared state.
/// - **Concurrent writes**: The write lock is released before sending notifications,
///   allowing high concurrency. Notification happens outside the critical section.
/// - **NOT reentrant**: Mutex is not reentrant. Calling `write()` from within
///   another `write()` on the same thread (e.g., from a callback) would deadlock.
///   However, this is not an issue in practice since notifications are sent after
///   the lock is released.
pub struct Writer {
    shared: Arc<Mutex<SharedBuffer>>,
    handle: Handle,
    queue: NotificationQueueArc,
    debug_hint: String,
}

impl Writer {
    #[must_use]
    pub fn new(
        handle: Handle,
        queue: NotificationQueueArc,
        debug_hint: &str,
        buffer: Buffer,
    ) -> Self {
        queue.whitelist(handle, &format!("memPipe.writer {debug_hint}"));

        Self {
            shared: Arc::new(Mutex::new(SharedBuffer::new(buffer))),
            handle,
            queue,
            debug_hint: debug_hint.to_string(),
        }
    }

    /// Get the current position (bytes written)
    #[must_use]
    pub fn tell(&self) -> usize {
        self.shared.lock().buffer.len()
    }

    /// Get current error state
    #[must_use]
    pub fn get_error(&self) -> i32 {
        self.shared.lock().errno
    }

    /// Set error state and notify readers
    pub fn set_error(&self, errno: i32) {
        {
            let mut shared = self.shared.lock();
            shared.errno = errno;
        }
        self.queue.notify(self.handle, -i64::from(errno));
    }

    /// Check if writer is closed
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.shared.lock().closed
    }

    /// Get a reference-counted handle to the underlying buffer
    #[must_use]
    pub fn buffer(&self) -> Buffer {
        self.shared.lock().buffer.clone()
    }

    /// Write data to the pipe (POSIX-style)
    ///
    /// Returns:
    /// - Positive value: number of bytes written
    /// - 0: empty write (no notification sent)
    /// - -1: error (writer closed, errno is set, or buffer write failed)
    ///
    /// # Important behavior
    ///
    /// - If the writer is closed, returns -1
    /// - If errno is set, returns -1
    /// - If data is empty, returns 0 WITHOUT notifying observers
    ///   (this avoids unnecessary wakeups of waiting readers)
    /// - If data is non-empty, appends to buffer and:
    ///   - If successful: notifies observers and returns the count
    ///   - If failed: sets errno and returns -1
    #[must_use]
    pub fn write(&self, data: &[u8]) -> isize {
        let notification = {
            let mut shared = self.shared.lock();

            if shared.closed {
                return -1;
            }

            if shared.errno != 0 {
                return -1;
            }

            if data.is_empty() {
                // IMPORTANT: Return early without notifying observers.
                // Empty writes should not wake up waiting readers.
                return 0;
            }

            if shared.buffer.append(data).is_ok() {
                // Safe conversion from usize to isize
                // On 64-bit platforms, check if length exceeds isize::MAX
                if let Ok(n) = isize::try_from(data.len()) {
                    n
                } else {
                    // Write succeeded but length exceeds isize::MAX
                    // This should never happen in practice with realistic I/O sizes
                    error!(
                        data_len = data.len(),
                        isize_max = isize::MAX,
                        "CRITICAL: write length exceeds isize::MAX"
                    );
                    isize::MAX
                }
            } else {
                // Buffer append failed - treat as ENOSPC
                shared.errno = 28; // ENOSPC
                -28
            }
        };

        // Notify outside lock
        self.queue.notify(self.handle, notification as i64);
        match notification.cmp(&0) {
            Ordering::Greater => notification,
            Ordering::Equal | Ordering::Less => -1,
        }
    }

    /// Close the writer and notify all readers
    pub fn close(&self) {
        {
            let mut shared = self.shared.lock();
            if shared.closed {
                log::warn!("Writer::close() called on already closed writer: {self:?}");
                return;
            }
            shared.closed = true;
        }
        // Unregister handle from queue
        // This will notify with -1 and wake all waiters
        self.queue.unlist(self.handle);
    }

    /// Get the handle for this writer
    #[must_use]
    pub fn handle(&self) -> &Handle {
        &self.handle
    }

    /// Create shared data for a new reader
    #[must_use]
    pub fn share_with_reader(&self) -> ReaderSharedData {
        ReaderSharedData {
            buffer: Arc::clone(&self.shared),
            writer_handle: self.handle,
            queue: self.queue.clone(),
        }
    }
}

impl fmt::Debug for Writer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Use try_lock to avoid deadlock if called while holding the lock
        if let Some(shared) = self.shared.try_lock() {
            write!(
                f,
                "Pipe.Writer(handle={:?}, closed={}, tell={}, errno={}, hint={})",
                self.handle,
                shared.closed,
                shared.buffer.len(),
                shared.errno,
                self.debug_hint
            )
        } else {
            write!(
                f,
                "Pipe.Writer(handle={:?}, <locked>, hint={})",
                self.handle, self.debug_hint
            )
        }
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        trace!(handle = ?self.handle, "Writer: destroying (drop)");
        if !self.is_closed() {
            self.close();
        }
    }
}
