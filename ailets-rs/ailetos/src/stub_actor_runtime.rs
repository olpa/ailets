//! Blocking `ActorRuntime` implementation
//!
//! This module provides a blocking `ActorRuntime` implementation that bridges
//! synchronous actor code with the async `SystemRuntime`. It maintains per-actor
//! state (fd table) and proxies all I/O operations to `SystemRuntime`.

use actor_runtime::ActorRuntime;
use tokio::sync::{mpsc, oneshot};
use tracing::{error, trace, warn};

use crate::idgen::Handle;
use crate::system_runtime::{ChannelHandle, IoRequest, SendableBuffer};

/// File descriptor entry - tracks the state of an fd.
///
/// Fds start as "allowed but not materialized" and transition to "active" on first use.
///
/// ## Why ActiveReader and ActiveWriter store different data
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
        let idx = fd as usize;
        if idx < self.table.len() {
            self.table[idx] = Some(entry);
        }
    }

    /// Allocate a new fd and associate it with an `FdEntry`.
    /// Finds the first empty slot or appends. Returns the fd (index).
    pub fn insert(&mut self, entry: FdEntry) -> isize {
        // Find first None slot
        if let Some(fd) = self.table.iter().position(|e| e.is_none()) {
            self.table[fd] = Some(entry);
            return fd as isize;
        }
        // No empty slot, append
        let fd = self.table.len();
        self.table.push(Some(entry));
        fd as isize
    }

    /// Look up the `FdEntry` for a given fd
    #[must_use]
    pub fn get(&self, fd: isize) -> Option<&FdEntry> {
        self.table.get(fd as usize).and_then(|e| e.as_ref())
    }

    /// Get mutable reference to an `FdEntry`
    #[must_use]
    pub fn get_mut(&mut self, fd: isize) -> Option<&mut FdEntry> {
        self.table.get_mut(fd as usize).and_then(|e| e.as_mut())
    }

    /// Remove an fd mapping and return the entry
    pub fn remove(&mut self, fd: isize) -> Option<FdEntry> {
        self.table.get_mut(fd as usize).and_then(|e| e.take())
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

/// Blocking `ActorRuntime` implementation
/// Acts as a pure proxy to `SystemRuntime` for all I/O operations
/// Provides sync-to-async adapters (blocking on async operations)
/// Maintains a per-actor fd table for POSIX-style fd semantics
pub struct BlockingActorRuntime {
    /// This actor's node handle (used as actor identifier)
    node_handle: Handle,
    /// Channel to send async I/O requests to `SystemRuntime`
    system_tx: mpsc::UnboundedSender<IoRequest>,
    /// Per-actor fd table (POSIX fd → global `ChannelHandle`)
    fd_table: std::sync::Mutex<FdTable>,
}

impl Clone for BlockingActorRuntime {
    fn clone(&self) -> Self {
        Self {
            node_handle: self.node_handle,
            system_tx: self.system_tx.clone(),
            fd_table: std::sync::Mutex::new(FdTable::new()),
        }
    }
}

impl BlockingActorRuntime {
    /// Create a new `ActorRuntime` for the given node handle
    #[must_use]
    pub fn new(node_handle: Handle, system_tx: mpsc::UnboundedSender<IoRequest>) -> Self {
        Self {
            node_handle,
            system_tx,
            fd_table: std::sync::Mutex::new(FdTable::new()),
        }
    }

    /// Register all standard file descriptors for this actor.
    /// Actual readers/writers are created lazily on first read/write.
    pub fn register_std_fds(&self) {
        use actor_runtime::StdHandle;

        let mut table = match self.fd_table.lock() {
            Ok(t) => t,
            Err(e) => {
                error!(actor = ?self.node_handle, error = ?e, "register_std_fds: fd_table lock poisoned");
                return;
            }
        };

        // Readers
        table.set(StdHandle::Stdin as isize, FdEntry::AllowedReader);
        table.set(StdHandle::Env as isize, FdEntry::AllowedReader);

        // Writers
        table.set(StdHandle::Stdout as isize, FdEntry::AllowedWriter);
        table.set(StdHandle::Log as isize, FdEntry::AllowedWriter);
        table.set(StdHandle::Metrics as isize, FdEntry::AllowedWriter);
        table.set(StdHandle::Trace as isize, FdEntry::AllowedWriter);

        trace!(actor = ?self.node_handle, "std fds registered");
    }

    /// Shutdown this actor runtime and clean up local state.
    ///
    /// # Pipe Cleanup Responsibility
    ///
    /// At the moment, pipes (writers and readers) are indirectly created by `SystemRuntime`
    /// through the channel table and `PipePool`. Therefore, it is `SystemRuntime`'s
    /// responsibility to close pipes and clean up their resources, not the actor's.
    ///
    /// This function only clears the actor's local fd table mapping (actor fd → system
    /// `ChannelHandle`). It does NOT close individual file descriptors or send close requests
    /// to `SystemRuntime`. The actual pipe cleanup happens in `SystemRuntime` when it
    /// receives the `ActorShutdown` notification and calls `pipe_pool.close_actor_writers()`.
    ///
    /// # Design Rationale
    ///
    /// - **Ownership**: `SystemRuntime` owns the pipes via `PipePool`, so it should clean them up
    /// - **Centralized cleanup**: All pipe closure happens in one place (`PipePool::close_actor_writers`)
    /// - **Prevents double-close**: Actor doesn't close pipes that `SystemRuntime` will close
    /// - **Simpler shutdown**: Actor only needs to drop its local fd mapping
    ///
    /// After clearing the fd table, this function sends an `ActorShutdown` notification to
    /// `SystemRuntime` to trigger pipe cleanup.
    ///
    /// If the fd table lock is poisoned, logs an error and returns without clearing the table.
    pub fn shutdown(&self) {
        trace!(actor = ?self.node_handle, "actor shutdown - clearing fd table");

        // Clear the fd table without closing individual fds
        // The pipes themselves will be closed by SystemRuntime via PipePool
        match self.fd_table.lock() {
            Ok(mut table) => {
                table.clear();
                trace!(actor = ?self.node_handle, "fd table cleared");
            }
            Err(e) => {
                error!(actor = ?self.node_handle, error = ?e, "shutdown: fd_table lock poisoned");
                return;
            }
        }

        // Notify SystemRuntime to close pipes and cleanup resources
        if let Err(e) = self.system_tx.send(IoRequest::ActorShutdown {
            node_handle: self.node_handle,
        }) {
            error!(actor = ?self.node_handle, error = ?e, "shutdown: failed to send ActorShutdown notification");
        }

        trace!(actor = ?self.node_handle, "actor shutdown complete");
    }
}

#[allow(clippy::unwrap_used)] // Blocking implementation - panics on channel failures
impl ActorRuntime for BlockingActorRuntime {
    fn get_errno(&self) -> isize {
        trace!(actor = ?self.node_handle, "get_errno");
        0 // No error
    }

    fn open_read(&self, _name: &str) -> isize {
        trace!(actor = ?self.node_handle, "open_read");
        // Send request to SystemRuntime and block for response
        let (tx, rx) = oneshot::channel();

        if let Err(e) = self.system_tx.send(IoRequest::OpenRead {
            node_handle: self.node_handle,
            response: tx,
        }) {
            error!(actor = ?self.node_handle, error = ?e, "open_read: failed to send request");
            return -1;
        }

        trace!(actor = ?self.node_handle, "open_read: blocking_recv");
        let channel_handle = match rx.blocking_recv() {
            Ok(handle) => handle,
            Err(e) => {
                error!(actor = ?self.node_handle, error = ?e, "open_read: failed to receive response");
                return -1;
            }
        };

        // Allocate local fd and map to channel handle (wrapped as ActiveReader)
        let fd = match self.fd_table.lock() {
            Ok(mut table) => table.insert(FdEntry::ActiveReader(channel_handle)),
            Err(e) => {
                error!(actor = ?self.node_handle, error = ?e, "open_read: fd_table lock poisoned");
                return -1;
            }
        };
        trace!(actor = ?self.node_handle, fd = fd, channel = ?channel_handle, "open_read done");
        fd
    }

    fn open_write(&self, _name: &str) -> isize {
        trace!(actor = ?self.node_handle, "open_write");

        // For now, open_write creates an ActiveWriter that writes to Stdout
        // The actual pipe will be created lazily on first write via PipePool
        // TODO: Support named streams by parsing the name parameter

        let fd = match self.fd_table.lock() {
            Ok(mut table) => table.insert(FdEntry::ActiveWriter {
                node_handle: self.node_handle,
                std_handle: actor_runtime::StdHandle::Stdout,
            }),
            Err(e) => {
                error!(actor = ?self.node_handle, error = ?e, "open_write: fd_table lock poisoned");
                return -1;
            }
        };
        trace!(actor = ?self.node_handle, fd = fd, "open_write done (lazy writer)");
        fd
    }

    fn aread(&self, fd: isize, buffer: &mut [u8]) -> isize {
        trace!(actor = ?self.node_handle, fd = fd, buflen = buffer.len(), "aread");

        // Get the channel handle, materializing stdin if needed
        let channel_handle = {
            let table = match self.fd_table.lock() {
                Ok(t) => t,
                Err(e) => {
                    error!(actor = ?self.node_handle, fd = fd, error = ?e, "aread: fd_table lock poisoned");
                    return -1;
                }
            };

            match table.get(fd) {
                Some(FdEntry::ActiveReader(handle)) => *handle,
                Some(FdEntry::AllowedReader) => {
                    // Need to materialize stdin - release lock first
                    drop(table);

                    trace!(actor = ?self.node_handle, "aread: materializing stdin");
                    let (tx, rx) = oneshot::channel();

                    if let Err(e) = self.system_tx.send(IoRequest::MaterializeStdin {
                        node_handle: self.node_handle,
                        response: tx,
                    }) {
                        error!(actor = ?self.node_handle, error = ?e, "aread: failed to send MaterializeStdin");
                        return -1;
                    }

                    let handle = match rx.blocking_recv() {
                        Ok(h) => h,
                        Err(e) => {
                            error!(actor = ?self.node_handle, error = ?e, "aread: failed to receive MaterializeStdin response");
                            return -1;
                        }
                    };

                    // Update the fd entry to ActiveReader
                    let mut table = match self.fd_table.lock() {
                        Ok(t) => t,
                        Err(e) => {
                            error!(actor = ?self.node_handle, fd = fd, error = ?e, "aread: fd_table lock poisoned after materialize");
                            return -1;
                        }
                    };
                    if let Some(entry) = table.get_mut(fd) {
                        *entry = FdEntry::ActiveReader(handle);
                    }

                    trace!(actor = ?self.node_handle, channel = ?handle, "aread: stdin materialized");
                    handle
                }
                Some(FdEntry::AllowedWriter) | Some(FdEntry::ActiveWriter { .. }) => {
                    warn!(actor = ?self.node_handle, fd = fd, "aread: cannot read from stdout");
                    return -1;
                }
                None => {
                    warn!(actor = ?self.node_handle, fd = fd, "aread: fd not found");
                    return -1;
                }
            }
        };

        // Send request to SystemRuntime and block for response
        let (tx, rx) = oneshot::channel();

        // SAFETY: We're passing a raw pointer to our buffer and will block until
        // the handler finishes using it. The buffer remains valid because:
        // 1. Our stack frame stays alive (we block via blocking_recv)
        // 2. Only the handler accesses the buffer while we're blocked
        // 3. The channel ensures happens-before ordering
        // 4. The SendableBuffer is consumed exactly once in the handler
        let buffer_ptr = unsafe { SendableBuffer::new(buffer) };

        if let Err(e) = self.system_tx.send(IoRequest::Read {
            handle: channel_handle,
            buffer: buffer_ptr,
            response: tx,
        }) {
            error!(actor = ?self.node_handle, fd = fd, error = ?e, "aread: failed to send request");
            return -1;
        }

        // Block waiting for SystemRuntime to complete the async read
        trace!(actor = ?self.node_handle, "aread: blocking_recv");
        let bytes_read = match rx.blocking_recv() {
            Ok(n) => n,
            Err(e) => {
                error!(actor = ?self.node_handle, fd = fd, error = ?e, "aread: failed to receive response");
                -1
            }
        };
        trace!(actor = ?self.node_handle, bytes = bytes_read, "aread done");

        bytes_read
    }

    fn awrite(&self, fd: isize, buffer: &[u8]) -> isize {
        trace!(actor = ?self.node_handle, fd = fd, buflen = buffer.len(), "awrite");

        // Get the write info (node_handle + std_handle)
        let (node_handle, std_handle) = {
            let mut table = match self.fd_table.lock() {
                Ok(t) => t,
                Err(e) => {
                    error!(actor = ?self.node_handle, fd = fd, error = ?e, "awrite: fd_table lock poisoned");
                    return -1;
                }
            };

            match table.get(fd) {
                Some(FdEntry::ActiveWriter { node_handle, std_handle }) => (*node_handle, *std_handle),
                Some(FdEntry::AllowedWriter) => {
                    // Upgrade to ActiveWriter with default Stdout handle
                    let nh = self.node_handle;
                    let sh = actor_runtime::StdHandle::Stdout;
                    if let Some(entry) = table.get_mut(fd) {
                        *entry = FdEntry::ActiveWriter {
                            node_handle: nh,
                            std_handle: sh,
                        };
                    }
                    trace!(actor = ?self.node_handle, "awrite: upgraded to ActiveWriter");
                    (nh, sh)
                }
                Some(FdEntry::AllowedReader) | Some(FdEntry::ActiveReader(_)) => {
                    warn!(actor = ?self.node_handle, fd = fd, "awrite: cannot write to stdin");
                    return -1;
                }
                None => {
                    warn!(actor = ?self.node_handle, fd = fd, "awrite: fd not found");
                    return -1;
                }
            }
        };

        // Send request to SystemRuntime and block for response
        let (tx, rx) = oneshot::channel();

        if let Err(e) = self.system_tx.send(IoRequest::Write {
            node_handle,
            std_handle,
            data: buffer.to_vec(),
            response: tx,
        }) {
            error!(actor = ?self.node_handle, fd = fd, error = ?e, "awrite: failed to send request");
            return -1;
        }

        trace!(actor = ?self.node_handle, "awrite: blocking_recv");
        let result = match rx.blocking_recv() {
            Ok(n) => n,
            Err(e) => {
                error!(actor = ?self.node_handle, fd = fd, error = ?e, "awrite: failed to receive response");
                -1
            }
        };
        trace!(actor = ?self.node_handle, result = result, "awrite done");
        result
    }

    fn aclose(&self, fd: isize) -> isize {
        trace!(actor = ?self.node_handle, fd = fd, "aclose");

        // Remove the fd entry and get its state
        let entry = match self.fd_table.lock() {
            Ok(mut table) => {
                if let Some(entry) = table.remove(fd) {
                    entry
                } else {
                    warn!(actor = ?self.node_handle, fd = fd, "aclose: fd not found");
                    return -1;
                }
            }
            Err(e) => {
                error!(actor = ?self.node_handle, fd = fd, error = ?e, "aclose: fd_table lock poisoned");
                return -1;
            }
        };

        // Handle based on entry type
        match entry {
            FdEntry::AllowedReader | FdEntry::AllowedWriter => {
                // Never materialized - nothing to close
                trace!(actor = ?self.node_handle, fd = fd, "aclose: handle never materialized");
                0
            }
            FdEntry::ActiveReader(channel_handle) => {
                // Close reader via SystemRuntime
                let (tx, rx) = oneshot::channel();

                if let Err(e) = self.system_tx.send(IoRequest::Close {
                    handle: channel_handle,
                    response: tx,
                }) {
                    error!(actor = ?self.node_handle, fd = fd, error = ?e, "aclose: failed to send Close request");
                    return -1;
                }

                trace!(actor = ?self.node_handle, "aclose: blocking_recv for reader");
                match rx.blocking_recv() {
                    Ok(n) => {
                        trace!(actor = ?self.node_handle, result = n, "aclose reader done");
                        n
                    }
                    Err(e) => {
                        error!(actor = ?self.node_handle, fd = fd, error = ?e, "aclose: failed to receive Close response");
                        -1
                    }
                }
            }
            FdEntry::ActiveWriter { node_handle, std_handle } => {
                // Close writer via SystemRuntime
                let (tx, rx) = oneshot::channel();

                if let Err(e) = self.system_tx.send(IoRequest::CloseWriter {
                    node_handle,
                    std_handle,
                    response: tx,
                }) {
                    error!(actor = ?self.node_handle, fd = fd, error = ?e, "aclose: failed to send CloseWriter request");
                    return -1;
                }

                trace!(actor = ?self.node_handle, "aclose: blocking_recv for writer");
                match rx.blocking_recv() {
                    Ok(n) => {
                        trace!(actor = ?self.node_handle, result = n, "aclose writer done");
                        n
                    }
                    Err(e) => {
                        error!(actor = ?self.node_handle, fd = fd, error = ?e, "aclose: failed to receive CloseWriter response");
                        -1
                    }
                }
            }
        }
    }
}
