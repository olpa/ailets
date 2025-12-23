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
#[derive(Debug, thiserror::Error)]
pub enum MemPipeError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Writer is closed")]
    WriterClosed,

    #[error("Writer is in error state: {0}")]
    WriterError(i32),
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
            MemPipeError::Io(e) => e,
            MemPipeError::WriterClosed => io::Error::new(io::ErrorKind::BrokenPipe, e.to_string()),
            MemPipeError::WriterError(_) => io::Error::new(io::ErrorKind::Other, e.to_string()),
        }
    }
}

/// Shared state between Writer and Readers
struct SharedBuffer {
    buffer: Vec<u8>,
    error: Option<i32>,
    closed: bool,
}

impl SharedBuffer {
    fn new(external_buffer: Option<Vec<u8>>) -> Self {
        Self {
            buffer: external_buffer.unwrap_or_default(),
            error: None,
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
}

impl Writer {
    pub fn new(
        handle: Handle,
        queue: NotificationQueueArc,
        external_buffer: Option<Vec<u8>>,
    ) -> Self {
        // Register handle with queue (like Python's queue.whitelist)
        queue.whitelist(handle, "writer");

        Self {
            shared: Arc::new(Mutex::new(SharedBuffer::new(external_buffer))),
            handle,
            queue,
        }
    }

    /// Get the current position (bytes written)
    pub fn tell(&self) -> usize {
        self.shared.lock().unwrap().buffer.len()
    }

    /// Get current error state
    pub fn get_error(&self) -> Option<i32> {
        self.shared.lock().unwrap().error
    }

    /// Set error state and notify readers
    pub fn set_error(&self, errno: i32) -> Result<(), MemPipeError> {
        {
            let mut shared = self.shared.lock().unwrap();
            if shared.closed {
                return Ok(());
            }
            shared.error = Some(errno);
        }
        self.queue.notify(self.handle, errno);
        Ok(())
    }

    /// Check if writer is closed
    pub fn is_closed(&self) -> bool {
        self.shared.lock().unwrap().closed
    }

    /// Synchronous write
    pub fn write(&self, data: &[u8]) -> Result<usize, MemPipeError> {
        if data.is_empty() {
            return Ok(0);
        }

        let len = {
            let mut shared = self.shared.lock().unwrap();

            if shared.closed {
                return Err(MemPipeError::WriterClosed);
            }

            if let Some(errno) = shared.error {
                return Err(MemPipeError::WriterError(errno));
            }

            shared.buffer.extend_from_slice(data);
            data.len()
        };

        // Notify outside lock
        self.queue.notify(self.handle, len as i32);
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
    pub(crate) fn shared(&self) -> Arc<Mutex<SharedBuffer>> {
        Arc::clone(&self.shared)
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
        Writer::write(self, buf).map_err(|e| e.into())
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl Write for Writer {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        Writer::write(self, buf).map_err(|e| e.into())
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
    handle: Handle,  // Reader's own handle
    shared: Arc<Mutex<SharedBuffer>>,
    writer_handle: Handle,  // Writer's handle for notifications
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
    pub fn get_error(&self) -> Option<i32> {
        self.shared.lock().unwrap().error
    }

    /// Check if reader should wait for more data
    ///
    /// Returns (should_wait, writer_closed)
    fn should_wait(&self) -> (bool, bool) {
        let shared = self.shared.lock().unwrap();

        let writer_pos = shared.buffer.len();
        let should_wait = self.pos >= writer_pos;
        let writer_closed = shared.closed || shared.error.is_some();

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
            let writer_closed = shared.closed || shared.error.is_some();
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

        // Wait using wait_unsafe (lock is passed to wait_unsafe and released there)
        self.queue.wait_unsafe(self.writer_handle, "reader", queue_lock).await;
        Ok(())
    }

    /// Read available data from buffer
    fn read_from_buffer(&mut self, buf: &mut [u8]) -> Result<usize, MemPipeError> {
        if self.closed {
            return Ok(0);
        }

        let shared = self.shared.lock().unwrap();

        if let Some(errno) = shared.error {
            return Err(MemPipeError::WriterError(errno));
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
            self.wait_for_writer().await.map_err(|e| IoError::from(MemPipeError::from(e)))?;

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
        external_buffer: Option<Vec<u8>>,
    ) -> Self {
        let writer = Writer::new(
            writer_handle.clone(),
            queue.clone(),
            external_buffer,
        );

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
            self.writer.shared(),
            self.writer.handle,  // All readers wait on writer's handle
            self.queue.clone(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_write_read() {
        let queue = NotificationQueueArc::new();
        let writer_handle = Handle::new(1);

        let mut pipe = MemPipe::new(
            writer_handle,
            queue.clone(),
            None,
        );

        let mut reader = pipe.get_reader(Handle::new(2));
        let reader_handle = *reader.handle();

        // Write some data
        pipe.writer().write(b"Hello").unwrap();

        // Read it back
        let mut buf = [0u8; 10];
        let n = reader.read(&mut buf).await.unwrap();
        assert_eq!(n, 5);
        assert_eq!(&buf[..n], b"Hello");

        // Writer unregisters its handle on drop
    }

    #[tokio::test]
    async fn test_multiple_readers() {
        let queue = NotificationQueueArc::new();
        let writer_handle = Handle::new(1);

        let mut pipe = MemPipe::new(
            writer_handle,
            queue.clone(),
            None,
        );

        let mut reader1 = pipe.get_reader(Handle::new(2));
        let reader1_handle = *reader1.handle();
        let mut reader2 = pipe.get_reader(Handle::new(3));
        let reader2_handle = *reader2.handle();

        // Write data
        pipe.writer().write(b"Broadcast").unwrap();

        // Both readers should get the same data
        let mut buf1 = [0u8; 20];
        let mut buf2 = [0u8; 20];

        let n1 = reader1.read(&mut buf1).await.unwrap();
        let n2 = reader2.read(&mut buf2).await.unwrap();

        assert_eq!(n1, 9);
        assert_eq!(n2, 9);
        assert_eq!(&buf1[..n1], b"Broadcast");
        assert_eq!(&buf2[..n2], b"Broadcast");

        // Writer unregisters its handle on drop
    }

    #[tokio::test]
    async fn test_close_propagation() {
        let queue = NotificationQueueArc::new();
        let writer_handle = Handle::new(1);

        let mut pipe = MemPipe::new(
            writer_handle,
            queue.clone(),
            None,
        );

        let mut reader = pipe.get_reader(Handle::new(2));
        let reader_handle = *reader.handle();

        // Write and close
        pipe.writer().write(b"Data").unwrap();
        pipe.writer().close().unwrap();

        // Reader should get data
        let mut buf = [0u8; 10];
        let n = reader.read(&mut buf).await.unwrap();
        assert_eq!(n, 4);

        // Second read should get EOF
        let n = reader.read(&mut buf).await.unwrap();
        assert_eq!(n, 0);

        // Writer unregisters its handle on drop
    }
}
