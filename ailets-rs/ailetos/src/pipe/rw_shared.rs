//! Shared state between Writer and Reader

use std::sync::atomic::{AtomicBool, AtomicI32};
use std::sync::Arc;

use crate::idgen::Handle;
use crate::storage::Buffer;

/// Shared state between Writer and Readers.
///
/// No outer mutex: `errno`, `closed`, and `had_readers` are all monotonic
/// (set once, never cleared) so atomic loads/stores with Acquire/Release
/// ordering are sufficient. Readers that miss a transition by one iteration
/// catch it on the next pass through `should_wait_for_writer()` after the
/// watch channel wakes them.
///
/// `buffer` retains its own internal mutex because `flush_buffer()` is async
/// and must lock it independently across `.await` without holding any outer lock.
pub(crate) struct SharedBuffer {
    pub(super) buffer: Buffer,
    pub(super) errno: AtomicI32,
    pub(super) closed: AtomicBool,
    /// True once at least one reader has been created.
    pub(super) had_readers: AtomicBool,
}

impl SharedBuffer {
    pub(super) fn new(buffer: Buffer) -> Self {
        Self {
            buffer,
            errno: AtomicI32::new(0),
            closed: AtomicBool::new(false),
            had_readers: AtomicBool::new(false),
        }
    }

    pub(crate) fn new_closed(buffer: Buffer) -> Self {
        Self {
            buffer,
            errno: AtomicI32::new(0),
            closed: AtomicBool::new(true),
            had_readers: AtomicBool::new(false),
        }
    }
}

/// Shared data passed from Writer to Reader.
///
/// This can be cloned to create multiple independent readers from the same source.
#[derive(Clone)]
pub struct ReaderSharedData {
    pub(crate) buffer: Arc<SharedBuffer>,
    pub(crate) writer_handle: Handle,
    pub(crate) watch_rx: tokio::sync::watch::Receiver<()>,
}

