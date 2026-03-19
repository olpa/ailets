//! Reader side of the memory pipe

use parking_lot::Mutex;
use std::fmt;
use std::sync::Arc;
use tracing::{error, trace};

use crate::idgen::Handle;
use crate::notification_queue::NotificationQueueArc;

use super::rw_shared::{ReaderSharedData, SharedBuffer};

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
}

impl Reader {
    #[must_use]
    pub fn new(handle: Handle, shared_data: ReaderSharedData) -> Self {
        Self {
            own_handle: handle,
            buffer: shared_data.buffer,
            writer_handle: shared_data.writer_handle,
            queue: shared_data.queue,
            pos: 0,
            own_closed: false,
            own_errno: 0,
        }
    }

    /// Get the reader's handle
    #[must_use]
    pub fn handle(&self) -> &Handle {
        &self.own_handle
    }

    /// Close the reader
    pub fn close(&mut self) {
        if self.own_closed {
            log::warn!("Reader::close() called on already closed reader: {self:?}");
            return;
        }
        self.own_closed = true;
    }

    /// Check if reader is closed
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.own_closed
    }

    /// Get current error state (checks own error first, then writer error)
    #[must_use]
    pub fn get_error(&self) -> i32 {
        if self.own_errno != 0 {
            self.own_errno
        } else {
            self.buffer.lock().errno
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

    /// Read data from the pipe (POSIX-style)
    ///
    /// Reads available data from the buffer. If no data is available,
    /// waits for the writer to provide more data or close.
    ///
    /// Returns:
    /// - Positive value: number of bytes read
    /// - 0: EOF (writer is closed and all data has been read)
    /// - -1: error (check `get_error()` for error code)
    pub async fn read(&mut self, buf: &mut [u8]) -> isize {
        while !self.own_closed {
            match self.should_wait_for_writer() {
                WaitAction::Wait => {
                    self.wait_for_writer().await;
                    continue; // restart the loop. A case of errors will be reported by "should_wait_for_writer"
                }
                WaitAction::Closed => {
                    return 0;
                }
                WaitAction::Error => {
                    return -1;
                }
                WaitAction::DontWait => {
                    // Proceed to read
                }
            }

            // Read data from buffer
            let shared = self.buffer.lock();
            let buffer_guard = shared.buffer.lock();
            let available = buffer_guard.len().saturating_sub(self.pos);
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
                return -1;
            };
            let Some(src_slice) = buffer_guard.get(self.pos..end_pos) else {
                error!(
                    buffer_len = buffer_guard.len(),
                    pos = self.pos,
                    end_pos = end_pos,
                    "CRITICAL: source buffer slice out of bounds"
                );
                return -1;
            };
            dest_slice.copy_from_slice(src_slice);
            self.pos = end_pos;

            drop(buffer_guard);
            drop(shared);
            return to_read.cast_signed();
        }

        0
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
        if !self.is_closed() {
            self.close();
        }
    }
}
