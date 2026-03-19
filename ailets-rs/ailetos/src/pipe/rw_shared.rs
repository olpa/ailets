//! Shared state between Writer and Reader

use parking_lot::Mutex;
use std::sync::Arc;

use crate::idgen::Handle;
use crate::storage::Buffer;
use crate::notification_queue::NotificationQueueArc;

/// Shared state between Writer and Readers
pub(crate) struct SharedBuffer {
    pub(super) buffer: Buffer,
    pub(super) errno: i32,
    pub(super) closed: bool,
}

impl SharedBuffer {
    pub(super) fn new(buffer: Buffer) -> Self {
        Self {
            buffer,
            errno: 0,
            closed: false,
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
