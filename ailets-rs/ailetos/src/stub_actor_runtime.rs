//! Blocking `ActorRuntime` implementation
//!
//! This module provides a blocking `ActorRuntime` implementation that bridges
//! synchronous actor code with the async `SystemRuntime`. It maintains per-actor
//! state (fd table) and proxies all I/O operations to `SystemRuntime`.

use actor_runtime::ActorRuntime;
use tokio::sync::{mpsc, oneshot};
use tracing::{error, trace, warn};

use crate::idgen::Handle;
use crate::system_runtime::{FdTable, IoRequest, SendableBuffer};

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

    /// Request `SystemRuntime` to set up standard handles before actor starts.
    /// This pre-opens stdin (fd 0) and stdout (fd 1) with the correct channel handles.
    /// Dependencies are obtained from the DAG inside `SystemRuntime`.
    ///
    /// # Panics
    /// Panics only if stdin/stdout are not assigned to fd 0/1 respectively.
    /// This indicates a programming error in the fd allocation logic.
    pub fn request_std_handles_setup(&self) {
        trace!(actor = ?self.node_handle, "requesting std handles setup");

        // Send request to SystemRuntime and block for response
        let (tx, rx) = oneshot::channel();

        if let Err(e) = self.system_tx.send(IoRequest::SetupStdHandles {
            node_handle: self.node_handle,
            response: tx,
        }) {
            error!(actor = ?self.node_handle, error = ?e, "request_std_handles_setup: failed to send request");
            return;
        }

        let std_handles = match rx.blocking_recv() {
            Ok(handles) => handles,
            Err(e) => {
                error!(actor = ?self.node_handle, error = ?e, "request_std_handles_setup: failed to receive response");
                return;
            }
        };

        // Map the pre-opened channel handles to fd 0 (stdin) and fd 1 (stdout)
        {
            let mut table = match self.fd_table.lock() {
                Ok(t) => t,
                Err(e) => {
                    error!(actor = ?self.node_handle, error = ?e, "request_std_handles_setup: fd_table lock poisoned");
                    return;
                }
            };

            // Insert stdin as fd 0
            let stdin_fd = table.insert(std_handles.stdin);
            if stdin_fd != 0 {
                error!(
                    actor = ?self.node_handle,
                    actual_fd = stdin_fd,
                    "CRITICAL: stdin assigned unexpected fd (expected 0)"
                );
            }

            // Insert stdout as fd 1
            let stdout_fd = table.insert(std_handles.stdout);
            if stdout_fd != 1 {
                error!(
                    actor = ?self.node_handle,
                    actual_fd = stdout_fd,
                    "CRITICAL: stdout assigned unexpected fd (expected 1)"
                );
            }
        }

        trace!(actor = ?self.node_handle, "std handles ready (stdin=0, stdout=1)");
    }

    /// Close all open handles when actor finishes.
    /// Closes in reverse order (highest fd first) to handle any dependencies.
    ///
    /// If the fd table lock is poisoned, logs an error and returns without closing handles.
    pub fn close_all_handles(&self) {
        trace!(actor = ?self.node_handle, "close_all_handles");

        // Get all open fds
        let fds: Vec<isize> = {
            match self.fd_table.lock() {
                Ok(table) => table.keys().copied().collect(),
                Err(e) => {
                    error!(actor = ?self.node_handle, error = ?e, "close_all_handles: fd_table lock poisoned");
                    return;
                }
            }
        };

        // Close in reverse order
        let mut fds = fds;
        fds.sort_unstable();
        fds.reverse();

        for fd in fds {
            trace!(actor = ?self.node_handle, fd = fd, "closing fd");
            let _ = self.aclose(fd);
        }

        // Signal actor shutdown to close any remaining latent pipes
        let _ = self.system_tx.send(IoRequest::ActorShutdown {
            node_handle: self.node_handle,
        });

        trace!(actor = ?self.node_handle, "all handles closed");
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

        // Allocate local fd and map to global channel handle
        let fd = match self.fd_table.lock() {
            Ok(mut table) => table.insert(channel_handle),
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
        // Send request to SystemRuntime and block for response
        let (tx, rx) = oneshot::channel();

        if let Err(e) = self.system_tx.send(IoRequest::OpenWrite {
            node_handle: self.node_handle,
            response: tx,
        }) {
            error!(actor = ?self.node_handle, error = ?e, "open_write: failed to send request");
            return -1;
        }

        trace!(actor = ?self.node_handle, "open_write: blocking_recv");
        let channel_handle = match rx.blocking_recv() {
            Ok(handle) => handle,
            Err(e) => {
                error!(actor = ?self.node_handle, error = ?e, "open_write: failed to receive response");
                return -1;
            }
        };

        // Allocate local fd and map to global channel handle
        let fd = match self.fd_table.lock() {
            Ok(mut table) => table.insert(channel_handle),
            Err(e) => {
                error!(actor = ?self.node_handle, error = ?e, "open_write: fd_table lock poisoned");
                return -1;
            }
        };
        trace!(actor = ?self.node_handle, fd = fd, channel = ?channel_handle, "open_write done");
        fd
    }

    fn aread(&self, fd: isize, buffer: &mut [u8]) -> isize {
        trace!(actor = ?self.node_handle, fd = fd, buflen = buffer.len(), "aread");

        // Look up the channel handle for this fd
        let channel_handle = match self.fd_table.lock() {
            Ok(table) => {
                if let Some(handle) = table.get(fd) {
                    handle
                } else {
                    warn!(actor = ?self.node_handle, fd = fd, "aread: fd not found");
                    return -1;
                }
            }
            Err(e) => {
                error!(actor = ?self.node_handle, fd = fd, error = ?e, "aread: fd_table lock poisoned");
                return -1;
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

        // Look up the channel handle for this fd
        let channel_handle = match self.fd_table.lock() {
            Ok(table) => {
                if let Some(handle) = table.get(fd) {
                    handle
                } else {
                    warn!(actor = ?self.node_handle, fd = fd, "awrite: fd not found");
                    return -1;
                }
            }
            Err(e) => {
                error!(actor = ?self.node_handle, fd = fd, error = ?e, "awrite: fd_table lock poisoned");
                return -1;
            }
        };

        // Send request to SystemRuntime and block for response
        let (tx, rx) = oneshot::channel();

        if let Err(e) = self.system_tx.send(IoRequest::Write {
            handle: channel_handle,
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

        // Look up and remove the channel handle for this fd
        let channel_handle = match self.fd_table.lock() {
            Ok(mut table) => {
                if let Some(handle) = table.remove(fd) {
                    handle
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

        // Send request to SystemRuntime and block for response
        let (tx, rx) = oneshot::channel();

        if let Err(e) = self.system_tx.send(IoRequest::Close {
            handle: channel_handle,
            response: tx,
        }) {
            error!(actor = ?self.node_handle, fd = fd, error = ?e, "aclose: failed to send request");
            return -1;
        }

        trace!(actor = ?self.node_handle, "aclose: blocking_recv");
        let result = match rx.blocking_recv() {
            Ok(n) => n,
            Err(e) => {
                error!(actor = ?self.node_handle, fd = fd, error = ?e, "aclose: failed to receive response");
                -1
            }
        };
        trace!(actor = ?self.node_handle, result = result, "aclose done");
        result
    }
}
