//! Pipe allocator - bridges KV storage and pipe primitives
//!
//! Provides an intermediate layer between `PipePool` (coordination) and
//! `Reader`/`Writer` (primitives). Handles buffer allocation from KV storage
//! and pipe construction.

use parking_lot::Mutex;
use std::sync::Arc;

use crate::idgen::Handle;
use crate::notification_queue::NotificationQueueArc;
use crate::storage::{KVBuffers, KVError, OpenMode};

use super::reader::Reader;
use super::rw_shared::{ReaderSharedData, SharedBuffer};
use super::writer::Writer;

/// Allocates pipes with their backing storage
///
/// Responsibilities:
/// - Buffer allocation from KV storage
/// - Pipe construction (Reader/Writer)
/// - Bridging storage layer and pipe primitives
///
/// Does NOT handle:
/// - Pipe lifecycle coordination (latent/realized states)
/// - Path naming conventions
/// - Notification of waiters
pub struct Allocator<K: KVBuffers> {
    kv: Arc<K>,
    notification_queue: NotificationQueueArc,
}

impl<K: KVBuffers> Allocator<K> {
    /// Create a new allocator
    #[must_use]
    pub fn new(kv: Arc<K>, notification_queue: NotificationQueueArc) -> Self {
        Self {
            kv,
            notification_queue,
        }
    }

    /// Create a writer with buffer allocated from KV storage
    ///
    /// # Parameters
    /// - `handle`: Handle for the writer
    /// - `path`: Path in KV storage (naming determined by caller)
    ///
    /// # Errors
    /// Returns error if buffer allocation fails
    pub async fn create_writer(
        &self,
        handle: Handle,
        path: &str,
    ) -> Result<Writer, KVError> {
        let buffer = self.kv.open(path, OpenMode::Write).await?;
        Ok(Writer::new(
            handle,
            self.notification_queue.clone(),
            path,
            buffer,
        ))
    }

    /// Create a reader from completed KV storage (for terminated producers)
    ///
    /// Opens a completed buffer from KV storage and constructs a reader
    /// with a closed SharedBuffer. Used when the producer has terminated
    /// and left data in KV storage.
    ///
    /// # Parameters
    /// - `reader_handle`: Handle for the reader
    /// - `writer_handle`: Handle of the writer that produced the data
    /// - `path`: Path in KV storage (naming determined by caller)
    ///
    /// # Errors
    /// Returns error if buffer doesn't exist or cannot be opened
    pub async fn create_reader_from_completed(
        &self,
        reader_handle: Handle,
        writer_handle: Handle,
        path: &str,
    ) -> Result<Reader, KVError> {
        let kv_buffer = self.kv.open(path, OpenMode::Read).await?;

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
            queue: self.notification_queue.clone(),
        };

        Ok(Reader::new(reader_handle, shared_data))
    }
}
