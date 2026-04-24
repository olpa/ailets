//! Shared state between Writer and Reader

use parking_lot::Mutex;
use std::sync::Arc;

use crate::idgen::Handle;
use crate::notification_queue::NotificationQueueArc;
use crate::storage::Buffer;

/// Shared state between Writer and Readers
pub(crate) struct SharedBuffer {
    pub(super) buffer: Buffer,
    pub(super) errno: i32,
    pub(super) closed: bool,
    /// Number of active readers still open.
    pub(super) reader_count: usize,
    /// True once at least one reader has been created.
    pub(super) had_readers: bool,
}

impl SharedBuffer {
    pub(super) fn new(buffer: Buffer) -> Self {
        Self {
            buffer,
            errno: 0,
            closed: false,
            reader_count: 0,
            had_readers: false,
        }
    }
}

/// Shared data passed from Writer to Reader.
///
/// This can be cloned to create multiple independent readers from the same source.
#[derive(Clone)]
pub struct ReaderSharedData {
    pub(crate) buffer: Arc<Mutex<SharedBuffer>>,
    pub(crate) writer_handle: Handle,
    pub(crate) queue: NotificationQueueArc,
}

/// Drop guard that decrements `reader_count` when a Reader is closed or dropped.
/// Created by `Writer::share_with_reader()`; owned exclusively by the Reader.
pub struct ReaderCountGuard(pub(crate) Arc<Mutex<SharedBuffer>>);

impl Drop for ReaderCountGuard {
    fn drop(&mut self) {
        let mut shared = self.0.lock();
        if shared.reader_count > 0 {
            shared.reader_count -= 1;
        }
    }
}
