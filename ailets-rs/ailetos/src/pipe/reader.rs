//! Reader side of the memory pipe

use parking_lot::Mutex;
use std::fmt;
use std::sync::Arc;
use tracing::{error, trace, warn};

use crate::errno::{EBADF, EIO, EPIPE};
use crate::idgen::Handle;
use crate::notification_queue::NotificationQueueArc;

use super::rw_shared::{ReaderCountGuard, ReaderSharedData, SharedBuffer};

/// Action to take when checking if reader should wait
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WaitAction {
    /// Reader should wait for more data
    Wait,
    /// Reader should not wait (data is available)
    DontWait,
    /// Reader should close (writer closed and no more data)
    /// Note: Closed implies `DontWait`
    Closed,
    /// Error occurred (own or writer error, use `get_error()` to retrieve)
    /// Note: Error implies `DontWait`
    Error,
}

/// Reader side of the memory pipe
///
/// Reads from the shared buffer at its own position. Waits for data
/// when position reaches end of buffer.
///
/// # Thread Safety
///
/// - **Thread-safe with Writer**: Reader safely accesses the Writer's shared buffer
///   concurrently via `Arc<parking_lot::Mutex>`. Multiple Readers can read from the
///   same Writer simultaneously, each maintaining its own read position.
/// - **NOT reentrant for `read()`**: The `read()` method takes `&mut self` and maintains
///   mutable state (position, closed flag, errno). Cannot call `read()` concurrently
///   on the same Reader instance - this is enforced at compile time by Rust's borrow
///   checker. Each Reader instance must be used from a single task/thread at a time.
/// - **Separate Readers are independent**: Different Reader instances can operate
///   concurrently without interfering with each other.
pub struct Reader {
    own_handle: Handle,
    buffer: Arc<Mutex<SharedBuffer>>,
    writer_handle: Handle,
    queue: NotificationQueueArc,
    pos: usize,
    own_closed: bool,
    own_errno: i32,
    guard: Option<ReaderCountGuard>,
}

impl Reader {
    #[must_use]
    pub fn new(handle: Handle, shared_data: ReaderSharedData, guard: ReaderCountGuard) -> Self {
        Self {
            own_handle: handle,
            buffer: shared_data.buffer,
            writer_handle: shared_data.writer_handle,
            queue: shared_data.queue,
            pos: 0,
            own_closed: false,
            own_errno: 0,
            guard: Some(guard),
        }
    }

    /// Get the reader's handle
    #[must_use]
    pub fn handle(&self) -> &Handle {
        &self.own_handle
    }

    /// Close the reader
    ///
    /// Returns `Ok(())` on success, `Err(EBADF)` if already closed.
    pub fn close(&mut self) -> Result<(), i32> {
        if self.own_closed {
            log::warn!("Reader::close() called on already closed reader: {self:?}");
            return Err(EBADF);
        }
        self.own_closed = true;
        self.guard.take();
        Ok(())
    }

    /// Get current error state (checks own error first, then writer error)
    ///
    /// Per `spec://errors#writer-to-reader`: when the writer closed with a non-zero
    /// errno, this reader always reports EPIPE (32) regardless of the writer's actual
    /// error code.
    #[must_use]
    pub fn get_error(&self) -> i32 {
        if self.own_errno != 0 {
            return self.own_errno;
        }
        let writer_errno = self.buffer.lock().errno;
        if writer_errno != 0 {
            EPIPE
        } else {
            0
        }
    }

    /// Set reader's own error state (does not notify)
    pub fn set_error(&mut self, errno: i32) {
        self.own_errno = errno;
    }

    /// Check if reader should wait for writer
    ///
    /// Returns action to take: Wait, `DontWait`, Closed, or Error
    ///
    /// Priority order:
    /// 1. Error - if reader has own error (regardless of data availability)
    /// 2. `DontWait` - if data is available, allow reader to catch up
    /// 3. Error - if caught up and writer has error
    /// 4. Closed - if caught up and writer is closed
    /// 5. Wait - if caught up but writer is still active
    fn should_wait_for_writer(&self) -> WaitAction {
        // Priority 1: Check reader's own error first
        if self.own_errno != 0 {
            return WaitAction::Error;
        }

        let shared = self.buffer.lock();
        let writer_pos = shared.buffer.len();

        // Priority 2: If data is available, allow reading it
        if self.pos < writer_pos {
            return WaitAction::DontWait;
        }

        // Reader is caught up with writer (pos >= writer_pos)
        // Priority 3: Check writer error
        if shared.errno != 0 {
            WaitAction::Error
        } else if shared.closed {
            WaitAction::Closed
        } else {
            WaitAction::Wait
        }
    }

    /// Wait for writer to provide more data
    ///
    /// See the `crate::notification_queue` documentation for the workflow explanation
    /// (check (in "read") - lock (here) - check again (here))
    async fn wait_for_writer(&self) {
        let queue_lock = self.queue.get_lock();

        match self.should_wait_for_writer() {
            WaitAction::Wait => {
                self.queue
                    .wait_async(self.writer_handle, "reader", queue_lock)
                    .await;
            }
            WaitAction::Closed | WaitAction::DontWait | WaitAction::Error => {
                drop(queue_lock);
            }
        }
    }

    /// Read data from the pipe.
    ///
    /// Reads available data from the buffer. If no data is available,
    /// waits for the writer to provide more data or close.
    ///
    /// # Returns
    ///
    /// - `Ok(n)` where `n > 0`: number of bytes read
    /// - `Ok(0)`: EOF (writer is closed and all data has been read)
    /// - `Err(errno)`: error occurred (errno is latched for subsequent calls)
    pub async fn read(&mut self, buf: &mut [u8]) -> Result<usize, i32> {
        while !self.own_closed {
            match self.should_wait_for_writer() {
                WaitAction::Wait => {
                    self.wait_for_writer().await;
                    continue; // restart the loop. A case of errors will be reported by "should_wait_for_writer"
                }
                WaitAction::Closed => {
                    return Ok(0);
                }
                WaitAction::Error => {
                    return Err(self.get_error());
                }
                WaitAction::DontWait => {
                    // Proceed to read
                }
            }

            // Read data from buffer
            let shared = self.buffer.lock();
            let bufferguard = shared.buffer.lock();
            let available = bufferguard.len().saturating_sub(self.pos);
            let to_read = available.min(buf.len());
            let end_pos = self.pos + to_read;

            // Use safe slice access with bounds checking
            // These should always succeed based on the calculations above, but we handle errors gracefully
            let Some(dest_slice) = buf.get_mut(..to_read) else {
                error!(
                    buf_len = buf.len(),
                    to_read = to_read,
                    "CRITICAL: destination buffer slice out of bounds"
                );
                // Explicit drops: shared borrows self.buffer, set_error needs &mut self
                drop(bufferguard);
                drop(shared);
                self.set_error(EIO);
                return Err(EIO);
            };
            let Some(src_slice) = bufferguard.get(self.pos..end_pos) else {
                error!(
                    buffer_len = bufferguard.len(),
                    pos = self.pos,
                    end_pos = end_pos,
                    "CRITICAL: source buffer slice out of bounds"
                );
                drop(bufferguard);
                drop(shared);
                self.set_error(EIO);
                return Err(EIO);
            };
            dest_slice.copy_from_slice(src_slice);
            self.pos = end_pos;

            return Ok(to_read);
        }

        Ok(0)
    }
}

impl fmt::Debug for Reader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Pipe.Reader(handle={:?}, pos={}, closed={}, errno={}, writer_handle={:?})",
            self.own_handle, self.pos, self.own_closed, self.own_errno, self.writer_handle
        )
    }
}

impl Drop for Reader {
    fn drop(&mut self) {
        trace!(handle = ?self.own_handle, "Reader: destroying (drop)");
        if !self.own_closed {
            if let Err(errno) = self.close() {
                warn!(handle = ?self.own_handle, errno, "Reader::drop: close failed");
            }
        }
    }
}
