//! Shared state between Writer and Reader

use parking_lot::Mutex;
use std::sync::Arc;

use crate::idgen::Handle;
use crate::storage::Buffer;

/// Shared state between Writer and Readers
pub(crate) struct SharedBuffer {
    pub(super) buffer: Buffer,
    pub(super) errno: i32,
    pub(super) closed: bool,
    /// True once at least one reader has been created.
    pub(super) had_readers: bool,
}

impl SharedBuffer {
    pub(super) fn new(buffer: Buffer) -> Self {
        Self {
            buffer,
            errno: 0,
            closed: false,
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
    pub(crate) watch_rx: tokio::sync::watch::Receiver<()>,
}

