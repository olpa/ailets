//! Blocking `ActorRuntime` implementation
//!
//! This module provides a blocking `ActorRuntime` implementation that bridges
//! synchronous actor code with the async `SystemRuntime`. It maintains per-actor
//! state (fd table) and proxies all I/O operations to `SystemRuntime`.

use std::sync::Arc;

use actor_runtime::ActorRuntime;
use tokio::sync::{mpsc, oneshot};
use tracing::{error, warn};

use crate::fd_table::{FdEntry, FdTable};
use crate::idgen::Handle;
use crate::suspension::SuspensionState;
use crate::system_runtime::{IoRequest, SendableBuffer};

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
    /// Shared suspension state (owned by Environment)
    suspension: Arc<SuspensionState>,
}

/// Lifecycle handle that notifies `SystemRuntime` when an actor is done.
///
/// Returned alongside `BlockingActorRuntime` from `BlockingActorRuntime::new`.
/// Call `shutdown()` explicitly at the normal exit point; `Drop` fires the same
/// cleanup automatically if the actor panics or returns early without calling it.
/// Sending `ActorShutdown` multiple times is safe — `SystemRuntime` is idempotent.
pub struct ShutdownHandle {
    node_handle: Handle,
    system_tx: mpsc::UnboundedSender<IoRequest>,
    suspension: Arc<SuspensionState>,
}

impl ShutdownHandle {
    fn do_shutdown(&self) {
        self.suspension.deregister(self.node_handle);
        if let Err(e) = self.system_tx.send(IoRequest::ActorShutdown {
            node_handle: self.node_handle,
        }) {
            error!(actor = ?self.node_handle, error = ?e, "shutdown: failed to send ActorShutdown notification");
        }
    }
}

impl Drop for ShutdownHandle {
    /// Fires shutdown unconditionally — safe to call after an explicit `shutdown()`.
    fn drop(&mut self) {
        self.do_shutdown();
    }
}

impl BlockingActorRuntime {
    /// Create a new `ActorRuntime` for the given node handle.
    ///
    /// Returns the runtime together with a `ShutdownHandle`. Call
    /// `ShutdownHandle::shutdown` at the normal exit point; the handle will also
    /// fire cleanup automatically on drop if `shutdown` is never called.
    #[must_use]
    pub fn new(
        node_handle: Handle,
        system_tx: mpsc::UnboundedSender<IoRequest>,
        suspension: Arc<SuspensionState>,
    ) -> (Self, ShutdownHandle) {
        let runtime = Self {
            node_handle,
            system_tx: system_tx.clone(),
            fd_table: std::sync::Mutex::new(FdTable::new()),
            suspension: Arc::clone(&suspension),
        };
        let shutdown = ShutdownHandle {
            node_handle,
            system_tx,
            suspension,
        };
        (runtime, shutdown)
    }

    /// Yield cooperatively if this actor has been suspended; blocks until resumed.
    fn yield_if_suspended(&self) {
        self.suspension.check_and_wait(self.node_handle);
    }

    /// Self-suspend: register this actor as suspended, then block until resumed.
    ///
    /// The caller (e.g. a debug actor) decides when to pause; an external party
    /// (e.g. the CLI `resume` command) calls `SuspensionState::resume` to unblock.
    pub fn suspend_and_wait(&self) {
        self.suspension.suspend(self.node_handle);
        self.suspension.check_and_wait(self.node_handle);
    }


    /// Get this actor's node handle
    #[must_use]
    pub fn node_handle(&self) -> Handle {
        self.node_handle
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
    }
}

impl ActorRuntime for BlockingActorRuntime {
    fn get_errno(&self) -> isize {
        0 // No error
    }

    fn open_read(&self, _name: &str) -> isize {
        // Send request to SystemRuntime and block for response
        let (tx, rx) = oneshot::channel();

        if let Err(e) = self.system_tx.send(IoRequest::OpenRead {
            node_handle: self.node_handle,
            response: tx,
        }) {
            error!(actor = ?self.node_handle, error = ?e, "open_read: failed to send request");
            return -1;
        }

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
        fd
    }

    fn open_write(&self, _name: &str) -> isize {
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
        fd
    }

    fn aread(&self, fd: isize, buffer: &mut [u8]) -> isize {
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

                    handle
                }
                Some(FdEntry::AllowedWriter | FdEntry::ActiveWriter { .. }) => {
                    warn!(actor = ?self.node_handle, fd = fd, "aread: cannot read from stdout");
                    return -1;
                }
                None => {
                    warn!(actor = ?self.node_handle, fd = fd, "aread: fd not found");
                    return -1;
                }
            }
        };

        // Yield if suspended before issuing the read
        self.yield_if_suspended();

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
        let result = match rx.blocking_recv() {
            Ok(n) => n,
            Err(e) => {
                error!(actor = ?self.node_handle, fd = fd, error = ?e, "aread: failed to receive response");
                return -1;
            }
        };

        // Yield if suspended after the read completes
        self.yield_if_suspended();
        result
    }

    fn awrite(&self, fd: isize, buffer: &[u8]) -> isize {
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
                Some(FdEntry::ActiveWriter {
                    node_handle,
                    std_handle,
                }) => (*node_handle, *std_handle),
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
                    (nh, sh)
                }
                Some(FdEntry::AllowedReader | FdEntry::ActiveReader(_)) => {
                    warn!(actor = ?self.node_handle, fd = fd, "awrite: cannot write to stdin");
                    return -1;
                }
                None => {
                    warn!(actor = ?self.node_handle, fd = fd, "awrite: fd not found");
                    return -1;
                }
            }
        };

        // Yield if suspended before issuing the write
        self.yield_if_suspended();

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

        let result = match rx.blocking_recv() {
            Ok(n) => n,
            Err(e) => {
                error!(actor = ?self.node_handle, fd = fd, error = ?e, "awrite: failed to receive response");
                return -1;
            }
        };

        // Yield if suspended after the write completes
        self.yield_if_suspended();
        result
    }

    fn aclose(&self, fd: isize) -> isize {
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
                0
            }
            FdEntry::ActiveReader(channel_handle) => {
                // Yield if suspended before issuing the close
                self.yield_if_suspended();

                // Close reader via SystemRuntime
                let (tx, rx) = oneshot::channel();

                if let Err(e) = self.system_tx.send(IoRequest::Close {
                    handle: channel_handle,
                    response: tx,
                }) {
                    error!(actor = ?self.node_handle, fd = fd, error = ?e, "aclose: failed to send Close request");
                    return -1;
                }

                let result = match rx.blocking_recv() {
                    Ok(n) => n,
                    Err(e) => {
                        error!(actor = ?self.node_handle, fd = fd, error = ?e, "aclose: failed to receive Close response");
                        return -1;
                    }
                };

                // Yield if suspended after the close completes
                self.yield_if_suspended();
                result
            }
            FdEntry::ActiveWriter {
                node_handle,
                std_handle,
            } => {
                // Yield if suspended before issuing the close
                self.yield_if_suspended();

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

                let result = match rx.blocking_recv() {
                    Ok(n) => n,
                    Err(e) => {
                        error!(actor = ?self.node_handle, fd = fd, error = ?e, "aclose: failed to receive CloseWriter response");
                        return -1;
                    }
                };

                // Yield if suspended after the close completes
                self.yield_if_suspended();
                result
            }
        }
    }
}
