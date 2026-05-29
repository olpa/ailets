//! Writer side of the memory pipe

use std::fmt;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tracing::{trace, warn};

use crate::errno::{EBADF, ENOSPC, EPIPE};
use crate::idgen::Handle;
use crate::storage::Buffer;

use super::rw_shared::{ReaderSharedData, SharedBuffer};

/// Writer side of the memory pipe
///
/// Writes append to the shared buffer and notify waiting readers via a
/// `tokio::sync::watch` channel. Monotonic fields (`errno`, `closed`,
/// `had_readers`) are atomics; the buffer retains its own internal mutex.
pub struct Writer {
    shared: Arc<SharedBuffer>,
    handle: Handle,
    watch_tx: tokio::sync::watch::Sender<()>,
    debug_hint: String,
}

impl Writer {
    #[must_use]
    pub fn new(handle: Handle, debug_hint: &str, buffer: Buffer) -> Self {
        // The initial receiver from watch::channel() is dropped on purpose.
        // Unlike mpsc, a watch Sender stays open after all receivers are gone,
        // and new receivers can join later via watch_tx.subscribe().
        // Readers call subscribe() when they are created (see create_reader).
        let (watch_tx, _) = tokio::sync::watch::channel(());
        Self {
            shared: Arc::new(SharedBuffer::new(buffer)),
            handle,
            watch_tx,
            debug_hint: debug_hint.to_string(),
        }
    }

    /// Get the current position (bytes written)
    #[must_use]
    pub fn tell(&self) -> usize {
        self.shared.buffer.len()
    }

    /// Get current error state
    #[must_use]
    pub fn get_error(&self) -> i32 {
        self.shared.errno.load(Ordering::Acquire)
    }

    /// Set error state and notify readers
    pub fn set_error(&self, errno: i32) {
        self.shared.errno.store(errno, Ordering::Release);
        if self.watch_tx.send(()).is_err() {
            warn!(handle = ?self.handle, errno, "Writer::set_error: no receivers to notify");
        }
    }

    /// Check if writer is closed
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.shared.closed.load(Ordering::Acquire)
    }

    /// Get a reference-counted handle to the underlying buffer
    #[must_use]
    pub fn buffer(&self) -> Buffer {
        self.shared.buffer.clone()
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
        if self.shared.closed.load(Ordering::Acquire) {
            return Err(EBADF);
        }

        // Set EPIPE if all readers are gone. compare_exchange ensures only the
        // first writer to notice sets it (errno is monotonic: set once, never cleared).
        // Note: receiver_count() and compare_exchange are not atomic together — a reader
        // joining via share_with_reader() between the two could get a spurious EPIPE on
        // its first read. Acceptable because share_with_reader() is called before the
        // reader starts consuming, and errno is checked before any data is read.
        if self.shared.had_readers.load(Ordering::Acquire) && self.watch_tx.receiver_count() == 0 {
            self.shared
                .errno
                .compare_exchange(0, EPIPE, Ordering::AcqRel, Ordering::Acquire)
                .ok(); // Err means another writer already set errno; that's fine, first writer wins
        }

        let errno = self.shared.errno.load(Ordering::Acquire);
        if errno != 0 {
            return Err(errno);
        }

        if data.is_empty() {
            // IMPORTANT: Return early without notifying observers.
            // Empty writes should not wake up waiting readers.
            return Ok(0);
        }

        let result = if self.shared.buffer.append(data).is_ok() {
            Ok(data.len())
        } else {
            // Buffer append failed - treat as ENOSPC
            self.shared.errno.store(ENOSPC, Ordering::Release);
            Err(ENOSPC)
        };

        // Notify outside lock (both data and errors wake waiting readers)
        if self.watch_tx.send(()).is_err() {
            warn!(handle = ?self.handle, "Writer::write: no receivers to notify");
        }
        result
    }

    /// Close the writer and notify all readers.
    ///
    /// # Errors
    /// Returns `EBADF` if already closed.
    pub fn close(&self) -> Result<(), i32> {
        // compare_exchange from false→true is the "set once" close; if it was
        // already true, someone else closed first.
        if self
            .shared
            .closed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            warn!("Writer::close() called on already closed writer: {self:?}");
            return Err(EBADF);
        }
        // Wake all waiting readers; dropping watch_tx would also work but this
        // is explicit and consistent with the error/write notification pattern.
        if self.watch_tx.send(()).is_err() {
            warn!(handle = ?self.handle, "Writer::close: no receivers to notify");
        }
        Ok(())
    }

    /// Get the handle for this writer
    #[must_use]
    pub fn handle(&self) -> &Handle {
        &self.handle
    }

    /// Create shared data for a new reader.
    #[must_use]
    pub fn share_with_reader(&self) -> ReaderSharedData {
        self.shared.had_readers.store(true, Ordering::Release);
        ReaderSharedData {
            buffer: Arc::clone(&self.shared),
            watch_rx: self.watch_tx.subscribe(),
        }
    }
}

impl fmt::Debug for Writer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Pipe.Writer(handle={:?}, closed={}, tell={}, errno={}, hint={})",
            self.handle,
            self.shared.closed.load(Ordering::Acquire),
            self.shared.buffer.len(),
            self.shared.errno.load(Ordering::Acquire),
            self.debug_hint
        )
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
