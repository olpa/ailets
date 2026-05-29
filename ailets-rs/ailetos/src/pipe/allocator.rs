//! Pipe allocation functions - bridges KV storage and pipe primitives
//!
//! Standalone functions for allocating pipes with backing storage from KV.

use parking_lot::Mutex;
use std::sync::Arc;
use tracing::warn;

use crate::errno::EIO;
use crate::idgen::Handle;
use crate::storage::{KVBuffers, KVError, OpenMode};

use super::reader::Reader;
use super::rw_shared::{ReaderSharedData, SharedBuffer};
use super::writer::Writer;

/// Returns the KV path for an actor's pipe: `pipes/actor-{id}-{fd}`
#[must_use]
pub fn pipe_path(actor_handle: Handle, fd: isize) -> String {
    format!("pipes/actor-{}-{}", actor_handle.id(), fd)
}

/// Create a writer with buffer allocated from KV storage
///
/// # Parameters
/// - `kv`: Key-value store for buffer allocation
/// - `handle`: Handle for the writer
/// - `path`: Path in KV storage (naming determined by caller)
///
/// # Errors
/// Returns error if buffer allocation fails
pub async fn create_writer(
    kv: &dyn KVBuffers,
    handle: Handle,
    path: &str,
) -> Result<Writer, KVError> {
    let buffer = kv.open(path, OpenMode::Write).await?;
    Ok(Writer::new(handle, path, buffer))
}

/// Write data to KV storage as a completed buffer
///
/// Creates a new buffer at the given path, writes the data, and flushes it.
/// Used for value nodes that have their output ready at creation time.
///
/// # Errors
/// Returns error if buffer operations fail
pub async fn write_completed_buffer(
    kv: &dyn KVBuffers,
    path: &str,
    data: &[u8],
) -> Result<(), KVError> {
    let buffer = kv.open(path, OpenMode::Write).await?;
    buffer.append(data)?;
    kv.flush_buffer(&buffer).await?;
    Ok(())
}

/// Flush writer's buffer to storage and close the writer.
///
/// This function performs a two-step operation:
/// 1. Close the writer (marks it closed, notifies readers)
/// 2. Flush the buffer to persistent storage
///
/// # Why we force flush on close
///
/// The Writer's buffer lives in memory (`Arc<Mutex<Vec<u8>>>`). Without an explicit
/// flush, written data is never persisted to the KV storage backend (e.g., `SQLite`).
/// When using persistent storage, data would be lost without this flush.
///
/// The flush happens AFTER close because:
/// - `Writer::close()` stops new writes and notifies readers (immediate effect)
/// - `flush_buffer()` persists what was already written (can be slow, async)
/// - If flush fails, readers are already notified (consistent state)
///
/// # Parameters
/// - `kv`: Key-value store backend
/// - `writer`: The writer to close
/// - `log_context`: Context string for logging (e.g., "writer task", "actor shutdown")
///
/// # Errors
/// Returns `EBADF` if writer was already closed, `EIO` if flush failed.
pub async fn flush_and_close_writer(
    kv: &dyn KVBuffers,
    writer: &Writer,
    log_context: &str,
) -> Result<(), i32> {
    // Step 1: Close the writer (marks closed, notifies readers)
    if let Err(errno) = writer.close() {
        warn!(
            context = log_context,
            errno = errno,
            "writer already closed"
        );
        return Err(errno);
    }

    // Step 2: Flush buffer to persistent storage
    match kv.flush_buffer(&writer.buffer()).await {
        Ok(()) => Ok(()),
        Err(e) => {
            warn!(
                context = log_context,
                error = ?e,
                "flush failed after close"
            );
            Err(EIO)
        }
    }
}

/// Create a reader from completed KV storage (for terminated producers)
///
/// Opens a completed buffer from KV storage and constructs a reader
/// with a closed `SharedBuffer`. Used when the producer has terminated
/// and left data in KV storage.
///
/// The notification queue and writer handle are created as dummy values
/// internally since they're never used for completed (closed) buffers.
///
/// # Parameters
/// - `kv`: Key-value store for buffer access
/// - `reader_handle`: Handle for the reader
/// - `path`: Path in KV storage (naming determined by caller)
///
/// # Errors
/// Returns error if buffer doesn't exist or cannot be opened
pub async fn create_reader_from_completed(
    kv: &dyn KVBuffers,
    reader_handle: Handle,
    path: &str,
) -> Result<Reader, KVError> {
    let kv_buffer = kv.open(path, OpenMode::Read).await?;

    // Create a closed SharedBuffer with the KV data
    let shared_buffer = SharedBuffer {
        buffer: kv_buffer,
        errno: 0,
        closed: true, // Mark as closed since data is complete
        had_readers: false,
    };

    // Create dummy writer handle - unused since buffer is closed
    let writer_handle = Handle::new(-1);

    // Dummy watch channel: the Sender is dropped immediately, but since the buffer
    // is already marked closed, should_wait_for_writer() always returns Closed and
    // the watch receiver is never polled.
    let (_, watch_rx) = tokio::sync::watch::channel(());

    // Create ReaderSharedData
    let shared_data = ReaderSharedData {
        buffer: Arc::new(Mutex::new(shared_buffer)),
        writer_handle,
        watch_rx,
    };

    Ok(Reader::new(reader_handle, shared_data))
}
