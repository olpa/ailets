//! File descriptor table for actor runtime
//!
//! This module provides the per-actor file descriptor table that maintains
//! mappings between POSIX-style file descriptors (integers) and the underlying
//! channel handles used by the system runtime.

use crate::idgen::Handle;
use crate::system_runtime::ChannelHandle;

/// File descriptor entry - tracks the state of an fd.
///
/// Fds start as "allowed but not materialized" and transition to "active" on first use.
///
/// ## Why `ActiveReader` and `ActiveWriter` store different data
///
/// Writers go directly to `PipePool` using `(node_handle, std_handle)` as the key.
///
/// Readers need a level of indirection via `ChannelHandle` because `MergeReader`
/// (which aggregates multiple dependency pipes) must be stored in the channel table
/// and "taken" during async read operations.
#[derive(Debug, Clone)]
pub enum FdEntry {
    /// Reading allowed - reader will be created on first read
    AllowedReader,
    /// Writing allowed - writer will be created on first write
    AllowedWriter,
    /// Active reader - has a `ChannelHandle` for looking up `MergeReader` in channel table
    ActiveReader(ChannelHandle),
    /// Active writer - stores identifiers for direct `PipePool` access
    ActiveWriter {
        node_handle: Handle,
        std_handle: actor_runtime::StdHandle,
    },
}

/// Per-actor file descriptor table.
/// The fd is simply the index into the vector.
/// Pre-allocated with slots for all standard handles.
pub struct FdTable {
    table: Vec<Option<FdEntry>>,
}

impl FdTable {
    #[must_use]
    pub fn new() -> Self {
        let size = actor_runtime::StdHandle::_Count as usize;
        let mut table = Vec::with_capacity(size);
        table.resize(size, None);
        Self { table }
    }

    /// Set an entry at a specific fd (used for standard handles).
    pub fn set(&mut self, fd: isize, entry: FdEntry) {
        // Defensive: reject negative fds
        if fd < 0 {
            return;
        }
        #[allow(clippy::cast_sign_loss)] // Safe: fd >= 0 checked above
        let idx = fd as usize;
        if let Some(slot) = self.table.get_mut(idx) {
            *slot = Some(entry);
        }
    }

    /// Allocate a new fd and associate it with an `FdEntry`.
    /// Finds the first empty slot or appends. Returns the fd (index).
    /// Returns -1 on overflow.
    pub fn insert(&mut self, entry: FdEntry) -> isize {
        // Find first None slot
        if let Some(fd) = self.table.iter().position(std::option::Option::is_none) {
            // Defensive: check for isize overflow
            if fd > isize::MAX as usize {
                return -1;
            }
            if let Some(slot) = self.table.get_mut(fd) {
                *slot = Some(entry);
            }
            // Safe: overflow checked above
            #[allow(clippy::cast_possible_wrap)]
            {
                return fd as isize;
            }
        }
        // No empty slot, append
        let fd = self.table.len();
        // Defensive: check for isize overflow
        if fd > isize::MAX as usize {
            return -1;
        }
        self.table.push(Some(entry));
        // Safe: overflow checked above
        #[allow(clippy::cast_possible_wrap)]
        {
            fd as isize
        }
    }

    /// Look up the `FdEntry` for a given fd
    #[must_use]
    pub fn get(&self, fd: isize) -> Option<&FdEntry> {
        // Defensive: reject negative fds
        if fd < 0 {
            return None;
        }
        // Safe: fd >= 0 checked above
        #[allow(clippy::cast_sign_loss)]
        {
            self.table.get(fd as usize).and_then(|e| e.as_ref())
        }
    }

    /// Get mutable reference to an `FdEntry`
    #[must_use]
    pub fn get_mut(&mut self, fd: isize) -> Option<&mut FdEntry> {
        // Defensive: reject negative fds
        if fd < 0 {
            return None;
        }
        // Safe: fd >= 0 checked above
        #[allow(clippy::cast_sign_loss)]
        {
            self.table.get_mut(fd as usize).and_then(|e| e.as_mut())
        }
    }

    /// Remove an fd mapping and return the entry
    pub fn remove(&mut self, fd: isize) -> Option<FdEntry> {
        // Defensive: reject negative fds
        if fd < 0 {
            return None;
        }
        // Safe: fd >= 0 checked above
        #[allow(clippy::cast_sign_loss)]
        {
            self.table
                .get_mut(fd as usize)
                .and_then(std::option::Option::take)
        }
    }

    /// Clear all fd mappings (used during actor shutdown)
    pub fn clear(&mut self) {
        self.table.clear();
    }
}

impl Default for FdTable {
    fn default() -> Self {
        Self::new()
    }
}
