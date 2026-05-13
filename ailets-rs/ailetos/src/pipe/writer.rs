//! Writer side of the memory pipe

use parking_lot::Mutex;
use std::fmt;
use std::sync::Arc;
use tracing::{trace, warn};

use crate::errno::{EBADF, ENOSPC, EPIPE};
use crate::idgen::Handle;
use crate::notification_queue::NotificationQueueArc;
use crate::storage::Buffer;

use super::rw_shared::{ReaderCountGuard, ReaderSharedData, SharedBuffer};

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

    /// Write data to the pipe.
    ///
    /// # Returns
    ///
    /// - `Ok(n)` where `n > 0`: number of bytes written
    /// - `Ok(0)`: empty write (no notification sent)
    /// - `Err(errno)`: error (writer closed, errno is set, or buffer write failed)
    ///
    /// # Important behavior
    ///
    /// - If the writer is closed, returns `Err(EBADF)`
    /// - If errno is set, returns `Err(errno)`
    /// - If data is empty, returns `Ok(0)` WITHOUT notifying observers
    ///   (this avoids unnecessary wakeups of waiting readers)
    /// - If data is non-empty, appends to buffer and:
    ///   - If successful: notifies observers and returns the count
    ///   - If failed: sets errno and returns `Err(ENOSPC)`
    ///
    /// # Errors
    /// Returns `EBADF` if closed, current errno if set, or `ENOSPC` if buffer write fails.
    pub fn write(&self, data: &[u8]) -> Result<usize, i32> {
        let result: Result<usize, i32> = {
            let mut shared = self.shared.lock();

            if shared.closed {
                return Err(EBADF);
            }

            if shared.had_readers && shared.reader_count == 0 && shared.errno == 0 {
                shared.errno = EPIPE;
            }

            if shared.errno != 0 {
                return Err(shared.errno);
            }

            if data.is_empty() {
                // IMPORTANT: Return early without notifying observers.
                // Empty writes should not wake up waiting readers.
                return Ok(0);
            }

            if shared.buffer.append(data).is_ok() {
                Ok(data.len())
            } else {
                // Buffer append failed - treat as ENOSPC
                shared.errno = ENOSPC;
                Err(ENOSPC)
            }
        };

        // Notify outside lock
        match &result {
            Ok(n) => {
                #[allow(clippy::cast_possible_wrap)] // byte counts won't exceed i64::MAX
                self.queue.notify(self.handle, *n as i64);
            }
            Err(errno) => {
                self.queue.notify(self.handle, -i64::from(*errno));
            }
        }
        result
    }

    /// Close the writer and notify all readers.
    ///
    /// # Errors
    /// Returns `EBADF` if already closed.
    pub fn close(&self) -> Result<(), i32> {
        {
            let mut shared = self.shared.lock();
            if shared.closed {
                warn!("Writer::close() called on already closed writer: {self:?}");
                return Err(EBADF);
            }
            shared.closed = true;
        }
        // Unregister handle from queue
        // This will notify with -1 and wake all waiters
        self.queue.unlist(self.handle);
        Ok(())
    }

    /// Get the handle for this writer
    #[must_use]
    pub fn handle(&self) -> &Handle {
        &self.handle
    }

    /// Create shared data and a drop guard for a new reader.
    ///
    /// The caller must pass the `ReaderCountGuard` to the `Reader`. When the guard is
    /// dropped, `reader_count` is decremented automatically.
    #[must_use]
    pub fn share_with_reader(&self) -> (ReaderSharedData, ReaderCountGuard) {
        let mut shared = self.shared.lock();
        shared.reader_count += 1;
        shared.had_readers = true;
        drop(shared);
        let data = ReaderSharedData {
            buffer: Arc::clone(&self.shared),
            writer_handle: self.handle,
            queue: self.queue.clone(),
        };
        let guard = ReaderCountGuard(Arc::clone(&self.shared));
        (data, guard)
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
            if let Err(errno) = self.close() {
                warn!(handle = ?self.handle, errno, "Writer::drop: close failed");
            }
        }
    }
}
