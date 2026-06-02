use std::sync::atomic::{AtomicI64, Ordering};
use tracing::debug;

/// Kind of entity a handle identifies, used for tracing handle allocation
#[derive(Debug, Clone, Copy)]
pub enum HandleKind {
    Node,
    PipeWriter,
    PipeReader,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Handle {
    id: i64,
}

impl Handle {
    #[must_use]
    pub fn new(id: i64) -> Self {
        Self { id }
    }

    #[must_use]
    pub fn id(&self) -> i64 {
        self.id
    }
}

pub trait HandleType {
    type Id;
}

impl HandleType for Handle {
    type Id = i64;
}

/// Type that can be either a Handle id or an arbitrary signal value
pub type IntCanBeHandle = <Handle as HandleType>::Id;

/// Thread-safe ID generator
#[derive(Debug)]
pub struct IdGen {
    next_id: AtomicI64,
}

impl IdGen {
    #[must_use]
    pub fn new() -> Self {
        Self {
            next_id: AtomicI64::new(1),
        }
    }

    /// Get the next unique ID
    pub fn get_next(&self) -> <Handle as HandleType>::Id {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Get the next unique ID and log its allocation with kind and optional correlations
    pub fn get_next_traced(&self, kind: HandleKind, corr1: Handle, corr2: Option<Handle>) -> Handle {
        let handle = Handle::new(self.get_next());
        debug!(?handle, ?kind, ?corr1, ?corr2, "handle allocated");
        handle
    }
}

impl Default for IdGen {
    fn default() -> Self {
        Self::new()
    }
}
