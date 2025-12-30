//! In-memory pipe with async coordination via notification queue
//!
//! Implements a broadcast-style pipe where:
//! - One Writer appends to a shared buffer
//! - Multiple Readers can read from the buffer at their own positions
//! - Coordination via notification queue (wait when no data available)

use embedded_io_async::{ErrorType, Read, Write};
use std::fmt;
use std::io;
use std::sync::{Arc, Mutex};

use crate::notification_queue::{Handle, NotificationQueueArc};

/// Error type for mempipe operations
#[derive(Debug, Clone)]
pub enum MemPipeError {
    /// IO error from underlying operations
    Io(io::ErrorKind),

    /// Writer is closed
    WriterClosed,

    /// Writer is in error state with error code
    WriterError(i32),
}

impl fmt::Display for MemPipeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MemPipeError::Io(kind) => write!(f, "IO error: {kind}"),
            MemPipeError::WriterClosed => write!(f, "Writer is closed"),
            MemPipeError::WriterError(code) => write!(f, "Writer is in error state: {code}"),
        }
    }
}

impl std::error::Error for MemPipeError {}

impl From<io::Error> for MemPipeError {
    fn from(error: io::Error) -> Self {
        MemPipeError::Io(error.kind())
    }
}

/// Error type compatible with embedded_io
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoError {
    BrokenPipe,
    Other,
}

impl embedded_io::Error for IoError {
    fn kind(&self) -> embedded_io::ErrorKind {
        match self {
            IoError::BrokenPipe => embedded_io::ErrorKind::BrokenPipe,
            IoError::Other => embedded_io::ErrorKind::Other,
        }
    }
}

impl fmt::Display for IoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IoError::BrokenPipe => write!(f, "broken pipe"),
            IoError::Other => write!(f, "other error"),
        }
    }
}

impl std::error::Error for IoError {}

impl From<MemPipeError> for IoError {
    fn from(e: MemPipeError) -> Self {
        match e {
            MemPipeError::WriterClosed => IoError::BrokenPipe,
            _ => IoError::Other,
        }
    }
}

impl From<MemPipeError> for io::Error {
    fn from(e: MemPipeError) -> Self {
        match e {
            MemPipeError::Io(kind) => io::Error::new(kind, "IO error"),
            MemPipeError::WriterClosed => {
                io::Error::new(io::ErrorKind::BrokenPipe, "Writer is closed")
            }
            MemPipeError::WriterError(code) => io::Error::new(
                io::ErrorKind::Other,
                format!("Writer is in error state: {code}"),
            ),
        }
    }
}

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
        // Register handle with queue (like Python's queue.whitelist)
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
    pub fn set_error(&self, errno: i32) -> Result<(), MemPipeError> {
        {
            let mut shared = self.shared.lock().unwrap();
            if shared.closed {
                return Ok(());
            }
            shared.errno = errno;
        }
        self.queue.notify(self.handle, errno as i64);
        Ok(())
    }

    /// Check if writer is closed
    pub fn is_closed(&self) -> bool {
        self.shared.lock().unwrap().closed
    }

    /// Async write (calls write_sync)
    pub async fn write(&self, data: &[u8]) -> Result<usize, MemPipeError> {
        self.write_sync(data)
    }

    /// Synchronous write. Returns the number of bytes written, which is the size of `data`.
    ///
    /// # Important behavior
    ///
    /// - If the writer is closed, returns `WriterClosed` error even if data is empty
    /// - If errno is set, returns `WriterError` even if data is empty
    /// - If data is empty, returns `Ok(0)` WITHOUT notifying observers
    ///   (this avoids unnecessary wakeups of waiting readers)
    /// - If data is non-empty, appends to buffer and notifies all waiting observers
    pub fn write_sync(&self, data: &[u8]) -> Result<usize, MemPipeError> {
        let len = {
            let mut shared = self.shared.lock().unwrap();

            if shared.closed {
                return Err(MemPipeError::WriterClosed);
            }

            if shared.errno != 0 {
                return Err(MemPipeError::WriterError(shared.errno));
            }

            if data.is_empty() {
                // IMPORTANT: Return early without notifying observers.
                // Empty writes should not wake up waiting readers.
                return Ok(0);
            }

            shared.buffer.extend_from_slice(data);
            data.len()
        };

        // Notify outside lock
        self.queue.notify(self.handle, len as i64);
        Ok(len)
    }

    /// Close the writer and notify all readers
    pub fn close(&self) -> Result<(), MemPipeError> {
        {
            let mut shared = self.shared.lock().unwrap();
            shared.closed = true;
        }
        // Unregister handle from queue (like Python's queue.unlist)
        // This will notify with -1 and wake all waiters
        self.queue.unlist(self.handle);
        Ok(())
    }

    /// Get the handle for this writer
    pub fn handle(&self) -> &Handle {
        &self.handle
    }

    /// Get shared buffer for creating readers
    pub(crate) fn buffer(&self) -> Arc<Mutex<SharedBuffer>> {
        Arc::clone(&self.shared)
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

// Implement embedded_io Write traits
impl ErrorType for Writer {
    type Error = IoError;
}

impl embedded_io::Write for Writer {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        Writer::write_sync(self, buf).map_err(|e| e.into())
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl Write for Writer {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        Writer::write(self, buf).await.map_err(|e| e.into())
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Reader side of the memory pipe
///
/// Reads from the shared buffer at its own position. Waits for data
/// when position reaches end of buffer.
pub struct Reader {
    handle: Handle, // Reader's own handle
    shared: Arc<Mutex<SharedBuffer>>,
    writer_handle: Handle, // Writer's handle for notifications
    queue: NotificationQueueArc,
    pos: usize,
    closed: bool,
}

impl Reader {
    pub fn new(
        handle: Handle,
        shared: Arc<Mutex<SharedBuffer>>,
        writer_handle: Handle,
        queue: NotificationQueueArc,
    ) -> Self {
        Self {
            handle,
            shared,
            writer_handle,
            queue,
            pos: 0,
            closed: false,
        }
    }

    /// Get the reader's handle
    pub fn handle(&self) -> &Handle {
        &self.handle
    }

    /// Get current error state from writer
    pub fn get_error(&self) -> i32 {
        self.shared.lock().unwrap().errno
    }

    /// Check if reader should wait for more data
    ///
    /// Returns (should_wait, writer_closed)
    fn should_wait(&self) -> (bool, bool) {
        let shared = self.shared.lock().unwrap();

        let writer_pos = shared.buffer.len();
        let should_wait = self.pos >= writer_pos;
        let writer_closed = shared.closed || shared.errno != 0;

        drop(shared);

        // Auto-close logic: if we should wait but writer is closed, don't wait
        if should_wait && writer_closed {
            (false, true)
        } else {
            (should_wait, writer_closed)
        }
    }

    /// Check if reader should wait, and auto-close if writer closed
    fn should_wait_with_autoclose(&mut self) -> (bool, bool) {
        let (should_wait, writer_closed) = self.should_wait();
        if !should_wait && writer_closed {
            self.closed = true;
        }
        (should_wait, writer_closed)
    }

    /// Wait for writer to provide more data
    ///
    /// Uses atomic lock protocol: check condition and register waiter atomically
    /// under queue lock to prevent missing notifications.
    ///
    /// CRITICAL: Locks queue FIRST, then checks buffer condition while holding queue lock.
    /// This ensures the writer cannot notify between our condition check and waiter registration.
    async fn wait_for_writer(&mut self) -> Result<(), MemPipeError> {
        // Acquire queue lock first
        let queue_lock = self.queue.get_lock();

        // Check buffer condition while holding queue lock to ensure atomicity
        // Lock ordering: queue → buffer (writer uses: buffer → queue, no overlap)
        let (should_wait, writer_closed) = {
            let shared = self.shared.lock().unwrap();
            let writer_pos = shared.buffer.len();
            let should_wait = self.pos >= writer_pos;
            let writer_closed = shared.closed || shared.errno != 0;
            drop(shared); // Release buffer lock but keep queue lock

            // Auto-close logic
            if should_wait && writer_closed {
                (false, true)
            } else {
                (should_wait, writer_closed)
            }
        };

        if !should_wait {
            drop(queue_lock);
            if writer_closed {
                self.closed = true;
            }
            return Ok(());
        }

        // Wait using wait_async (lock is passed to wait_async and released there)
        self.queue
            .wait_async(self.writer_handle, "reader", queue_lock)
            .await;
        Ok(())
    }

    /// Read available data from buffer
    fn read_from_buffer(&mut self, buf: &mut [u8]) -> Result<usize, MemPipeError> {
        if self.closed {
            return Ok(0);
        }

        let shared = self.shared.lock().unwrap();

        if shared.errno != 0 {
            return Err(MemPipeError::WriterError(shared.errno));
        }

        let available = shared.buffer.len().saturating_sub(self.pos);
        if available == 0 {
            return Ok(0);
        }

        let to_read = available.min(buf.len());
        let end_pos = self.pos + to_read;

        buf[..to_read].copy_from_slice(&shared.buffer[self.pos..end_pos]);
        self.pos = end_pos;

        Ok(to_read)
    }

    /// Close the reader
    pub fn close(&mut self) {
        self.closed = true;
    }

    /// Check if reader is closed
    pub fn is_closed(&self) -> bool {
        self.closed
    }
}

impl fmt::Debug for Reader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "MemPipe.Reader(handle={:?}, pos={}, closed={}, writer_handle={:?})",
            self.handle, self.pos, self.closed, self.writer_handle
        )
    }
}

// Implement embedded_io Read traits
impl ErrorType for Reader {
    type Error = IoError;
}

impl Read for Reader {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        loop {
            // Try to read from buffer
            let n = self.read_from_buffer(buf).map_err(|e| IoError::from(e))?;
            if n > 0 {
                return Ok(n);
            }

            // Wait for more data (it will check condition atomically under lock)
            // Don't check condition here - would create race with writer closing
            self.wait_for_writer()
                .await
                .map_err(|e| IoError::from(MemPipeError::from(e)))?;

            // If wait_for_writer returns Ok, loop to try reading again
            // If writer closed, wait_for_writer auto-closes reader
            if self.closed {
                return Ok(0); // EOF
            }
        }
    }
}

/// In-memory pipe factory
pub struct MemPipe {
    writer: Writer,
    queue: NotificationQueueArc,
}

impl MemPipe {
    pub fn new(
        writer_handle: Handle,
        queue: NotificationQueueArc,
        hint: &str,
        external_buffer: Option<Vec<u8>>,
    ) -> Self {
        let writer = Writer::new(writer_handle.clone(), queue.clone(), hint, external_buffer);

        Self { writer, queue }
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
        Reader::new(
            reader_handle,
            self.writer.buffer(),
            self.writer.handle, // All readers wait on writer's handle
            self.queue.clone(),
        )
    }
}

impl fmt::Debug for MemPipe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MemPipe(writer={:?})", self.writer)
    }
}
