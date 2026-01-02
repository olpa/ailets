//! In-memory pipe with async coordination via notification queue
//!
//! Implements a broadcast-style pipe where:
//! - One Writer appends to a shared buffer
//! - Multiple Readers can read from the buffer at their own positions
//! - Coordination via notification queue (wait when no data available)

use std::fmt;
use std::sync::{Arc, Mutex};

use crate::notification_queue::{Handle, NotificationQueueArc};

/// Shared state between Writer and Readers
struct SharedBuffer {
    buffer: Vec<u8>,
    errno: i32,
    closed: bool,
}

impl SharedBuffer {
    fn new(external_buffer: Option<Vec<u8>>) -> Self {
        Self {
            buffer: external_buffer.unwrap_or_default(),
            errno: 0,
            closed: false,
        }
    }
}

/// Writer side of the memory pipe
///
/// Writes append to the shared buffer and notify waiting readers.
pub struct Writer {
    shared: Arc<Mutex<SharedBuffer>>,
    handle: Handle,
    queue: NotificationQueueArc,
    debug_hint: String,
}

impl Writer {
    pub fn new(
        handle: Handle,
        queue: NotificationQueueArc,
        debug_hint: &str,
        external_buffer: Option<Vec<u8>>,
    ) -> Self {
        queue.whitelist(handle, &format!("memPipe.writer {debug_hint}"));

        Self {
            shared: Arc::new(Mutex::new(SharedBuffer::new(external_buffer))),
            handle,
            queue,
            debug_hint: debug_hint.to_string(),
        }
    }

    /// Get the current position (bytes written)
    pub fn tell(&self) -> usize {
        self.shared.lock().unwrap().buffer.len()
    }

    /// Get current error state
    pub fn get_error(&self) -> i32 {
        self.shared.lock().unwrap().errno
    }

    /// Set error state and notify readers
    pub fn set_error(&self, errno: i32) {
        {
            let mut shared = self.shared.lock().unwrap();
            if shared.closed {
                return;
            }
            shared.errno = errno;
        }
        self.queue.notify(self.handle, errno as i64);
    }

    /// Check if writer is closed
    pub fn is_closed(&self) -> bool {
        self.shared.lock().unwrap().closed
    }

    /// Async write (calls write_sync)
    pub async fn write(&self, data: &[u8]) -> isize {
        self.write_sync(data)
    }

    /// Synchronous write (POSIX-style)
    ///
    /// Returns:
    /// - Positive value: number of bytes written
    /// - 0: empty write (no notification sent)
    /// - -1: error (writer closed or errno is set)
    ///
    /// # Important behavior
    ///
    /// - If the writer is closed, returns -1
    /// - If errno is set, returns -1
    /// - If data is empty, returns 0 WITHOUT notifying observers
    ///   (this avoids unnecessary wakeups of waiting readers)
    /// - If data is non-empty, appends to buffer and notifies all waiting observers
    pub fn write_sync(&self, data: &[u8]) -> isize {
        let len = {
            let mut shared = self.shared.lock().unwrap();

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

            shared.buffer.extend_from_slice(data);
            data.len()
        };

        // Notify outside lock
        self.queue.notify(self.handle, len as i64);
        len as isize
    }

    /// Close the writer and notify all readers
    pub fn close(&self) {
        {
            let mut shared = self.shared.lock().unwrap();
            shared.closed = true;
        }
        // Unregister handle from queue
        // This will notify with -1 and wake all waiters
        self.queue.unlist(self.handle);
    }

    /// Get the handle for this writer
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
        let shared = self.shared.lock().unwrap();
        write!(
            f,
            "MemPipe.Writer(handle={:?}, closed={}, tell={}, hint={})",
            self.handle, shared.closed, shared.buffer.len(), self.debug_hint
        )
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

/// Shared data passed from Writer to Reader
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
    /// Note: Closed implies DontWait
    Closed,
    /// Error occurred (own or writer error, use get_error() to retrieve)
    /// Note: Error implies DontWait
    Error,
}

/// Reader side of the memory pipe
///
/// Reads from the shared buffer at its own position. Waits for data
/// when position reaches end of buffer.
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
    pub fn handle(&self) -> &Handle {
        &self.own_handle
    }

    /// Close the reader
    pub fn close(&mut self) {
        self.own_closed = true;
    }

    /// Check if reader is closed
    pub fn is_closed(&self) -> bool {
        self.own_closed
    }

    /// Get current error state (checks own error first, then writer error)
    pub fn get_error(&self) -> i32 {
        if self.own_errno != 0 {
            self.own_errno
        } else {
            self.buffer.lock().unwrap().errno
        }
    }

    /// Set reader's own error state (does not notify)
    pub fn set_error(&mut self, errno: i32) {
        self.own_errno = errno;
    }

    /// Check if reader should wait for writer
    ///
    /// Returns action to take: Wait, DontWait, Closed, or Error
    ///
    /// Priority order:
    /// 1. Error - if reader has own error (regardless of data availability)
    /// 2. DontWait - if data is available, allow reader to catch up
    /// 3. Error - if caught up and writer has error
    /// 4. Closed - if caught up and writer is closed
    /// 5. Wait - if caught up but writer is still active
    fn should_wait_for_writer(&self) -> WaitAction {
        // Priority 1: Check reader's own error first
        if self.own_errno != 0 {
            return WaitAction::Error;
        }

        let shared = self.buffer.lock().unwrap();
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
    ///
    /// Returns true if reader should be closed, false otherwise.
    async fn wait_for_writer(&self) -> bool {
        let queue_lock = self.queue.get_lock();

        match self.should_wait_for_writer() {
            WaitAction::Wait => {
                self.queue
                    .wait_async(self.writer_handle, "reader", queue_lock)
                    .await;
                false
            }
            WaitAction::Closed => {
                drop(queue_lock);
                true // Signal caller to close
            }
            WaitAction::DontWait | WaitAction::Error => {
                drop(queue_lock);
                false
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
    /// - -1: error (check get_error() for error code)
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
            let shared = self.buffer.lock().unwrap();
            let available = shared.buffer.len().saturating_sub(self.pos);
            let to_read = available.min(buf.len());
            let end_pos = self.pos + to_read;

            buf[..to_read].copy_from_slice(&shared.buffer[self.pos..end_pos]);
            self.pos = end_pos;

            drop(shared);
            return to_read as isize;
        }

        0
    }
}

impl fmt::Debug for Reader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "MemPipe.Reader(handle={:?}, pos={}, closed={}, writer_handle={:?})",
            self.own_handle, self.pos, self.own_closed, self.writer_handle
        )
    }
}

/// In-memory pipe factory
pub struct MemPipe {
    writer: Writer,
}

impl MemPipe {
    pub fn new(
        writer_handle: Handle,
        queue: NotificationQueueArc,
        hint: &str,
        external_buffer: Option<Vec<u8>>,
    ) -> Self {
        let writer = Writer::new(writer_handle.clone(), queue.clone(), hint, external_buffer);

        Self { writer }
    }

    /// Get the writer side
    pub fn writer(&self) -> &Writer {
        &self.writer
    }

    /// Get a mutable reference to the writer
    pub fn writer_mut(&mut self) -> &mut Writer {
        &mut self.writer
    }

    /// Get a reader for this pipe with an explicit handle
    pub fn get_reader(&self, reader_handle: Handle) -> Reader {
        Reader::new(reader_handle, self.writer.share_with_reader())
    }
}

impl fmt::Debug for MemPipe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MemPipe(writer={:?})", self.writer)
    }
}
