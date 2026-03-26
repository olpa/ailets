//! Pipe allocation functions - bridges KV storage and pipe primitives
//!
//! Standalone functions for allocating pipes with backing storage from KV.

use parking_lot::Mutex;
use std::sync::Arc;

use crate::idgen::Handle;
use crate::notification_queue::NotificationQueueArc;
use crate::storage::{KVBuffers, KVError, OpenMode};

use super::reader::Reader;
use super::rw_shared::{ReaderSharedData, SharedBuffer};
use super::writer::Writer;

/// Create a writer with buffer allocated from KV storage
///
/// # Parameters
/// - `kv`: Key-value store for buffer allocation
/// - `notification_queue`: Queue for pipe data notifications
/// - `handle`: Handle for the writer
/// - `path`: Path in KV storage (naming determined by caller)
///
/// # Errors
/// Returns error if buffer allocation fails
pub async fn create_writer<K: KVBuffers>(
    kv: &K,
    notification_queue: NotificationQueueArc,
    handle: Handle,
    path: &str,
) -> Result<Writer, KVError> {
    let buffer = kv.open(path, OpenMode::Write).await?;
    Ok(Writer::new(handle, notification_queue, path, buffer))
}

/// Write data to KV storage as a completed buffer
///
/// Creates a new buffer at the given path, writes the data, and flushes it.
/// Used for value nodes that have their output ready at creation time.
///
/// # Errors
/// Returns error if buffer operations fail
pub async fn write_completed_buffer<K: KVBuffers>(
    kv: &K,
    path: &str,
    data: &[u8],
) -> Result<(), KVError> {
    let buffer = kv.open(path, OpenMode::Write).await?;
    buffer.append(data)?;
    kv.flush_buffer(&buffer).await?;
    Ok(())
}

/// Create a reader from completed KV storage (for terminated producers)
///
/// Opens a completed buffer from KV storage and constructs a reader
/// with a closed SharedBuffer. Used when the producer has terminated
/// and left data in KV storage.
///
/// # Parameters
/// - `kv`: Key-value store for buffer access
/// - `notification_queue`: Queue for pipe data notifications
/// - `reader_handle`: Handle for the reader
/// - `writer_handle`: Handle of the writer that produced the data
/// - `path`: Path in KV storage (naming determined by caller)
///
/// # Errors
/// Returns error if buffer doesn't exist or cannot be opened
pub async fn create_reader_from_completed<K: KVBuffers>(
    kv: &K,
    notification_queue: NotificationQueueArc,
    reader_handle: Handle,
    writer_handle: Handle,
    path: &str,
) -> Result<Reader, KVError> {
    let kv_buffer = kv.open(path, OpenMode::Read).await?;

    // Create a closed SharedBuffer with the KV data
    let shared_buffer = SharedBuffer {
        buffer: kv_buffer,
        errno: 0,
        closed: true, // Mark as closed since data is complete
    };

    // Create ReaderSharedData
    let shared_data = ReaderSharedData {
        buffer: Arc::new(Mutex::new(shared_buffer)),
        writer_handle,
        queue: notification_queue,
    };

    Ok(Reader::new(reader_handle, shared_data))
}
