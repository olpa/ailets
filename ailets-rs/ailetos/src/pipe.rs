//! In-memory pipe with async coordination via notification queue
//!
//! Implements a broadcast-style pipe where:
//! - One Writer appends to a shared buffer
//! - Multiple Readers can read from the buffer at their own positions
//! - Coordination via notification queue (wait when no data available)

use parking_lot::Mutex;
use std::cmp::Ordering;
use std::fmt;
use std::sync::Arc;

use crate::idgen::Handle;
use crate::io::Buffer;
use crate::notification_queue::NotificationQueueArc;

/// Shared state between Writer and Readers
struct SharedBuffer {
    buffer: Buffer,
    errno: i32,
    closed: bool,
}

impl SharedBuffer {
    fn new(buffer: Buffer) -> Self {
        Self {
            buffer,
            errno: 0,
            closed: false,
        }
    }
}

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
                #[allow(clippy::cast_possible_wrap)]
                {
                    data.len() as isize
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
    pub(crate) fn share_with_reader(&self) -> ReaderSharedData {
        ReaderSharedData {
            buffer: Arc::clone(&self.shared),
            writer_handle: self.handle,
            queue: self.queue.clone(),
        }
    }
}

impl fmt::Debug for Writer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let shared = self.shared.lock();
        write!(
            f,
            "Pipe.Writer(handle={:?}, closed={}, tell={}, errno={}, hint={})",
            self.handle,
            shared.closed,
            shared.buffer.len(),
            shared.errno,
            self.debug_hint
        )
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        if !self.is_closed() {
            self.close();
        }
    }
}

/// Shared data passed from Writer to Reader.
///
/// This can be cloned to create multiple independent readers from the same source.
#[derive(Clone)]
pub(crate) struct ReaderSharedData {
    buffer: Arc<Mutex<SharedBuffer>>,
    writer_handle: Handle,
    queue: NotificationQueueArc,
}

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
    pub(crate) fn new(handle: Handle, shared_data: ReaderSharedData) -> Self {
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

            // SAFETY: The bounds are checked above:
            // - to_read <= available <= (buffer.len() - self.pos)
            // - to_read <= buf.len()
            // - end_pos = self.pos + to_read, so end_pos <= buffer.len()
            // Therefore both slices are within bounds
            #[allow(clippy::indexing_slicing)]
            {
                buf[..to_read].copy_from_slice(&buffer_guard[self.pos..end_pos]);
            }
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
        if !self.is_closed() {
            self.close();
        }
    }
}

/// In-memory pipe factory
pub struct Pipe {
    writer: Writer,
}

impl Pipe {
    /// Create a new Pipe with the provided buffer
    #[must_use]
    pub fn new(
        writer_handle: Handle,
        queue: NotificationQueueArc,
        hint: &str,
        buffer: Buffer,
    ) -> Self {
        let writer = Writer::new(writer_handle, queue, hint, buffer);

        Self { writer }
    }

    /// Get the writer side
    #[must_use]
    pub fn writer(&self) -> &Writer {
        &self.writer
    }

    /// Get a mutable reference to the writer
    pub fn writer_mut(&mut self) -> &mut Writer {
        &mut self.writer
    }

    /// Get a reader for this pipe with an explicit handle
    #[must_use]
    pub fn get_reader(&self, reader_handle: Handle) -> Reader {
        Reader::new(reader_handle, self.writer.share_with_reader())
    }
}

impl fmt::Debug for Pipe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Pipe(writer={:?})", self.writer)
    }
}
