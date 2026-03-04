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
use tracing::error;

use crate::idgen::Handle;
use crate::io::Buffer;
use crate::notification_queue::NotificationQueueArc;

/// State of a pipe - latent, realized, or closed
pub enum PipeState {
    /// Pipe exists but writer hasn't connected yet
    /// Reads will block until pipe is realized or closed
    Latent {
        /// Name of the pipe (for buffer allocation later)
        name: String,
        /// Notification queue for the pipe
        notification_queue: NotificationQueueArc,
        /// Notifier for when pipe becomes realized or closed
        realized_notify: Arc<tokio::sync::Notify>,
    },

    /// Pipe is fully realized with writer and buffer
    Realized {
        /// The writer side of the pipe
        writer: Writer,
        /// The backing buffer
        buffer: Buffer,
    },

    /// Pipe was closed without ever being realized
    /// Actor closed its output without writing
    ClosedWithoutData,
}

/// Type alias for shared pipe state
pub type PipeStateArc = Arc<Mutex<PipeState>>;

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
    /// Pipe state - may be latent or realized
    pipe_state: Arc<Mutex<PipeState>>,
    /// Cached shared data (populated once pipe is realized)
    buffer: Option<Arc<Mutex<SharedBuffer>>>,
    writer_handle: Option<Handle>,
    queue: Option<NotificationQueueArc>,
    pos: usize,
    own_closed: bool,
    own_errno: i32,
}

impl Reader {
    /// Create a reader from pipe state (latent or realized)
    pub(crate) fn new(handle: Handle, pipe_state: Arc<Mutex<PipeState>>) -> Self {
        Self {
            own_handle: handle,
            pipe_state,
            buffer: None,
            writer_handle: None,
            queue: None,
            pos: 0,
            own_closed: false,
            own_errno: 0,
        }
    }

    /// Create a reader from shared data (compatibility constructor for realized pipes)
    #[allow(dead_code)]
    pub(crate) fn from_shared_data(handle: Handle, shared_data: ReaderSharedData) -> Self {
        Self {
            own_handle: handle,
            pipe_state: Arc::new(Mutex::new(PipeState::ClosedWithoutData)), // Dummy state
            buffer: Some(shared_data.buffer),
            writer_handle: Some(shared_data.writer_handle),
            queue: Some(shared_data.queue),
            pos: 0,
            own_closed: false,
            own_errno: 0,
        }
    }

    /// Ensure the reader has access to realized pipe data
    /// Waits if pipe is latent, returns true if realized, false if closed without data
    async fn ensure_realized(&mut self) -> bool {
        // If already cached, we're good
        if self.buffer.is_some() {
            return true;
        }

        loop {
            // Check state and extract what we need
            let action = {
                let state = self.pipe_state.lock();
                match &*state {
                    PipeState::Latent { realized_notify, .. } => {
                        // Clone notify handle before releasing lock
                        Some(Arc::clone(realized_notify))
                    }
                    PipeState::Realized { writer, .. } => {
                        // Cache the shared data while holding the lock
                        let shared_data = writer.share_with_reader();
                        self.buffer = Some(shared_data.buffer);
                        self.writer_handle = Some(shared_data.writer_handle);
                        self.queue = Some(shared_data.queue);
                        None // Signal we're done
                    }
                    PipeState::ClosedWithoutData => {
                        None // Will return false below
                    }
                }
            }; // Lock released here

            match action {
                Some(notify) => {
                    // Wait for realization
                    notify.notified().await;
                    // Loop back to check state again
                }
                None => {
                    // Either realized (buffer is Some) or closed without data
                    return self.buffer.is_some();
                }
            }
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
            return self.own_errno;
        }

        // Check cached buffer first
        if let Some(ref buffer) = self.buffer {
            return buffer.lock().errno;
        }

        // If not cached, check pipe state directly
        let state = self.pipe_state.lock();
        if let PipeState::Realized { writer, .. } = &*state {
            writer.get_error()
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

        // If buffer is not yet available (pipe not realized), wait
        let Some(ref buffer) = self.buffer else {
            return WaitAction::Wait;
        };

        let shared = buffer.lock();
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
        // If queue is not available, we can't wait on it
        // This shouldn't happen if ensure_realized() is called first
        let Some(ref queue) = self.queue else {
            return;
        };
        let Some(writer_handle) = self.writer_handle else {
            return;
        };

        let queue_lock = queue.get_lock();

        match self.should_wait_for_writer() {
            WaitAction::Wait => {
                queue.wait_async(writer_handle, "reader", queue_lock).await;
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
    /// If the pipe is latent, blocks until realized or closed.
    ///
    /// Returns:
    /// - Positive value: number of bytes read
    /// - 0: EOF (writer is closed and all data has been read, or pipe closed without data)
    /// - -1: error (check `get_error()` for error code)
    pub async fn read(&mut self, buf: &mut [u8]) -> isize {
        // Ensure pipe is realized (wait if latent)
        if !self.ensure_realized().await {
            // Pipe was closed without data
            return 0;
        }

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
            // Safety: buffer is guaranteed to be Some after ensure_realized() returns true
            let buffer = self.buffer.as_ref().expect("buffer should be populated");
            let shared = buffer.lock();
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
            "Pipe.Reader(handle={:?}, pos={}, closed={}, errno={}, writer_handle={:?}, realized={})",
            self.own_handle,
            self.pos,
            self.own_closed,
            self.own_errno,
            self.writer_handle,
            self.buffer.is_some()
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
    /// Writer handle (may be placeholder for latent pipes)
    writer_handle: Handle,
    /// Shared state - either latent or realized
    state: Arc<Mutex<PipeState>>,
}

impl Pipe {
    /// Create a new latent pipe (no writer, no buffer)
    #[must_use]
    pub fn new_latent(name: String, notification_queue: NotificationQueueArc) -> Self {
        Self {
            writer_handle: Handle::placeholder(), // Temporary handle
            state: Arc::new(Mutex::new(PipeState::Latent {
                name,
                notification_queue,
                realized_notify: Arc::new(tokio::sync::Notify::new()),
            })),
        }
    }

    /// Create a new realized pipe (with writer and buffer)
    #[must_use]
    pub fn new_realized(
        writer_handle: Handle,
        notification_queue: NotificationQueueArc,
        name: String,
        buffer: Buffer,
    ) -> Self {
        let writer = Writer::new(writer_handle, notification_queue.clone(), &name, buffer.clone());

        Self {
            writer_handle,
            state: Arc::new(Mutex::new(PipeState::Realized { writer, buffer })),
        }
    }

    /// Create a new Pipe with the provided buffer (compatibility method)
    ///
    /// This maintains compatibility with existing code that creates pipes eagerly.
    #[must_use]
    pub fn new(
        writer_handle: Handle,
        queue: NotificationQueueArc,
        hint: &str,
        buffer: Buffer,
    ) -> Self {
        Self::new_realized(writer_handle, queue, hint.to_string(), buffer)
    }

    /// Transition from latent to realized
    /// Allocates buffer and creates writer
    /// Wakes all readers waiting on this pipe (including attachments)
    pub fn realize(&mut self, writer_handle: Handle, buffer: Buffer) {
        let mut state = self.state.lock();

        // Clone the notify Arc before modifying state
        let notify_arc = if let PipeState::Latent {
            name,
            notification_queue,
            realized_notify,
        } = &*state
        {
            let writer = Writer::new(
                writer_handle,
                notification_queue.clone(),
                name,
                buffer.clone(),
            );

            let notify = Arc::clone(realized_notify);

            // Transition state
            *state = PipeState::Realized {
                writer,
                buffer: buffer.clone(),
            };

            // Update writer handle
            self.writer_handle = writer_handle;

            Some(notify)
        } else {
            None
        };

        // Drop the lock before notifying
        drop(state);

        // Wake all waiting readers (including attachments)
        if let Some(notify) = notify_arc {
            notify.notify_waiters();
        }
    }

    /// Check if pipe is realized
    #[must_use]
    pub fn is_realized(&self) -> bool {
        matches!(&*self.state.lock(), PipeState::Realized { .. })
    }

    /// Get the writer side (only for realized pipes)
    #[must_use]
    pub fn writer(&self) -> Option<&Writer> {
        // SAFETY: We cannot return a reference to the writer because it's behind a Mutex
        // This method signature is incompatible with the new design
        // We need to change callers to use a different pattern
        // For now, return None for latent pipes and panic with a helpful message
        let state = self.state.lock();
        match &*state {
            PipeState::Realized { .. } => {
                drop(state);
                // We can't return a reference because the lock would be dropped
                // This is a design issue that needs to be addressed
                panic!("writer() cannot return a reference with the new PipeState design. Use writer_for_operation() instead.");
            }
            _ => None,
        }
    }

    /// Get a reader for this pipe
    /// Works for both latent and realized pipes
    /// Reader will block on read if pipe is latent
    #[must_use]
    pub fn get_reader(&self, reader_handle: Handle) -> Reader {
        Reader::new(reader_handle, Arc::clone(&self.state))
    }

    /// Get access to the buffer (for flushing, etc.)
    #[must_use]
    pub fn buffer(&self) -> Option<Buffer> {
        let state = self.state.lock();
        match &*state {
            PipeState::Realized { buffer, .. } => Some(buffer.clone()),
            _ => None,
        }
    }

    /// Get the state for operations that need it
    #[must_use]
    pub fn state(&self) -> Arc<Mutex<PipeState>> {
        Arc::clone(&self.state)
    }

    /// Write data to the pipe (only works on realized pipes)
    ///
    /// Returns the number of bytes written, or -1 on error.
    /// Panics if pipe is not realized.
    #[must_use]
    pub fn write(&self, data: &[u8]) -> isize {
        let state = self.state.lock();
        if let PipeState::Realized { writer, .. } = &*state {
            writer.write(data)
        } else {
            panic!("write() called on non-realized pipe");
        }
    }

    /// Close the writer (only works on realized pipes)
    ///
    /// Panics if pipe is not realized.
    pub fn close_writer(&self) {
        let state = self.state.lock();
        if let PipeState::Realized { writer, .. } = &*state {
            writer.close();
        } else {
            panic!("close_writer() called on non-realized pipe");
        }
    }

    /// Get writer error (only works on realized pipes)
    ///
    /// Returns 0 if pipe is not realized.
    #[must_use]
    pub fn get_writer_error(&self) -> i32 {
        let state = self.state.lock();
        if let PipeState::Realized { writer, .. } = &*state {
            writer.get_error()
        } else {
            0
        }
    }

    /// Set writer error (only works on realized pipes)
    ///
    /// Panics if pipe is not realized.
    pub fn set_writer_error(&self, errno: i32) {
        let state = self.state.lock();
        if let PipeState::Realized { writer, .. } = &*state {
            writer.set_error(errno);
        } else {
            panic!("set_writer_error() called on non-realized pipe");
        }
    }
}

impl fmt::Debug for Pipe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = self.state.lock();
        match &*state {
            PipeState::Latent { name, .. } => {
                write!(f, "Pipe(Latent, name={})", name)
            }
            PipeState::Realized { writer, .. } => {
                write!(f, "Pipe(Realized, writer={:?})", writer)
            }
            PipeState::ClosedWithoutData => {
                write!(f, "Pipe(ClosedWithoutData)")
            }
        }
    }
}
