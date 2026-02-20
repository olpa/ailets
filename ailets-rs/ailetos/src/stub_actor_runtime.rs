//! Stub implementation of ActorRuntime for CLI testing
//!
//! This module provides a blocking ActorRuntime implementation that bridges
//! synchronous actor code with the async SystemRuntime. It maintains per-actor
//! state (fd table) and proxies all I/O operations to SystemRuntime.

use std::os::raw::c_int;

use actor_runtime::ActorRuntime;
use tokio::sync::{mpsc, oneshot};
use tracing::{trace, warn};

use crate::idgen::Handle;
use crate::system_runtime::{FdTable, IoRequest, SendableBuffer};

/// Stub `ActorRuntime` implementation for CLI testing
/// Acts as a pure proxy to `SystemRuntime` for all I/O operations
/// Provides sync-to-async adapters (blocking on async operations)
/// Maintains a per-actor fd table for POSIX-style fd semantics
pub struct StubActorRuntime {
    /// This actor's node handle (used as actor identifier)
    node_handle: Handle,
    /// Channel to send async I/O requests to `SystemRuntime`
    system_tx: mpsc::UnboundedSender<IoRequest>,
    /// Per-actor fd table (POSIX fd → global ChannelHandle)
    fd_table: std::sync::Mutex<FdTable>,
}

impl Clone for StubActorRuntime {
    fn clone(&self) -> Self {
        Self {
            node_handle: self.node_handle,
            system_tx: self.system_tx.clone(),
            fd_table: std::sync::Mutex::new(FdTable::new()),
        }
    }
}

impl StubActorRuntime {
    /// Create a new `ActorRuntime` for the given node handle
    pub fn new(node_handle: Handle, system_tx: mpsc::UnboundedSender<IoRequest>) -> Self {
        Self {
            node_handle,
            system_tx,
            fd_table: std::sync::Mutex::new(FdTable::new()),
        }
    }

    /// Request SystemRuntime to set up standard handles before actor starts.
    /// This pre-opens stdin (fd 0) and stdout (fd 1) with the correct channel handles.
    /// Dependencies must be provided from the DAG.
    #[allow(clippy::unwrap_used)]
    pub fn request_std_handles_setup(&self, dependencies: Vec<Handle>) {
        trace!(actor = ?self.node_handle, deps = ?dependencies, "requesting std handles setup");

        // Send request to SystemRuntime and block for response
        let (tx, rx) = oneshot::channel();

        self.system_tx
            .send(IoRequest::SetupStdHandles {
                node_handle: self.node_handle,
                dependencies,
                response: tx,
            })
            .unwrap();

        let std_handles = rx.blocking_recv().unwrap();

        // Map the pre-opened channel handles to fd 0 (stdin) and fd 1 (stdout)
        {
            let mut table = self.fd_table.lock().unwrap();

            // Insert stdin as fd 0
            let stdin_fd = table.insert(std_handles.stdin);
            assert_eq!(stdin_fd, 0, "stdin should be fd 0");

            // Insert stdout as fd 1
            let stdout_fd = table.insert(std_handles.stdout);
            assert_eq!(stdout_fd, 1, "stdout should be fd 1");
        }

        trace!(actor = ?self.node_handle, "std handles ready (stdin=0, stdout=1)");
    }

    /// Close all open handles when actor finishes.
    /// Closes in reverse order (highest fd first) to handle any dependencies.
    #[allow(clippy::unwrap_used)]
    pub fn close_all_handles(&self) {
        trace!(actor = ?self.node_handle, "close_all_handles");

        // Get all open fds
        let fds: Vec<c_int> = {
            let table = self.fd_table.lock().unwrap();
            table.keys().copied().collect()
        };

        // Close in reverse order
        let mut fds = fds;
        fds.sort();
        fds.reverse();

        for fd in fds {
            trace!(actor = ?self.node_handle, fd = fd, "closing fd");
            let _ = self.aclose(fd);
        }

        trace!(actor = ?self.node_handle, "all handles closed");
    }
}

#[allow(clippy::unwrap_used)] // Stub implementation for testing - panics are acceptable
impl ActorRuntime for StubActorRuntime {
    fn get_errno(&self) -> c_int {
        trace!(actor = ?self.node_handle, "get_errno");
        0 // No error
    }

    fn open_read(&self, _name: &str) -> c_int {
        trace!(actor = ?self.node_handle, "open_read");
        // Send request to SystemRuntime and block for response
        let (tx, rx) = oneshot::channel();

        self.system_tx
            .send(IoRequest::OpenRead {
                node_handle: self.node_handle,
                response: tx,
            })
            .unwrap();

        trace!(actor = ?self.node_handle, "open_read: blocking_recv");
        let channel_handle = rx.blocking_recv().unwrap();

        // Allocate local fd and map to global channel handle
        let fd = self.fd_table.lock().unwrap().insert(channel_handle);
        trace!(actor = ?self.node_handle, fd = fd, channel = ?channel_handle, "open_read done");
        fd
    }

    fn open_write(&self, _name: &str) -> c_int {
        trace!(actor = ?self.node_handle, "open_write");
        // Send request to SystemRuntime and block for response
        let (tx, rx) = oneshot::channel();

        self.system_tx
            .send(IoRequest::OpenWrite {
                node_handle: self.node_handle,
                response: tx,
            })
            .unwrap();

        trace!(actor = ?self.node_handle, "open_write: blocking_recv");
        let channel_handle = rx.blocking_recv().unwrap();

        // Allocate local fd and map to global channel handle
        let fd = self.fd_table.lock().unwrap().insert(channel_handle);
        trace!(actor = ?self.node_handle, fd = fd, channel = ?channel_handle, "open_write done");
        fd
    }

    fn aread(&self, fd: c_int, buffer: &mut [u8]) -> c_int {
        trace!(actor = ?self.node_handle, fd = fd, buflen = buffer.len(), "aread");

        // Look up the channel handle for this fd
        let channel_handle = match self.fd_table.lock().unwrap().get(fd) {
            Some(h) => h,
            None => {
                warn!(actor = ?self.node_handle, fd = fd, "aread: fd not found");
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

        self.system_tx
            .send(IoRequest::Read {
                handle: channel_handle,
                buffer: buffer_ptr,
                response: tx,
            })
            .unwrap();

        // Block waiting for SystemRuntime to complete the async read
        trace!(actor = ?self.node_handle, "aread: blocking_recv");
        let bytes_read = rx.blocking_recv().unwrap();
        trace!(actor = ?self.node_handle, bytes = bytes_read, "aread done");

        bytes_read
    }

    fn awrite(&self, fd: c_int, buffer: &[u8]) -> c_int {
        trace!(actor = ?self.node_handle, fd = fd, buflen = buffer.len(), "awrite");

        // Look up the channel handle for this fd
        let channel_handle = match self.fd_table.lock().unwrap().get(fd) {
            Some(h) => h,
            None => {
                warn!(actor = ?self.node_handle, fd = fd, "awrite: fd not found");
                return -1;
            }
        };

        // Send request to SystemRuntime and block for response
        let (tx, rx) = oneshot::channel();

        self.system_tx
            .send(IoRequest::Write {
                handle: channel_handle,
                data: buffer.to_vec(),
                response: tx,
            })
            .unwrap();

        trace!(actor = ?self.node_handle, "awrite: blocking_recv");
        let result = rx.blocking_recv().unwrap();
        trace!(actor = ?self.node_handle, result = result, "awrite done");
        result
    }

    fn aclose(&self, fd: c_int) -> c_int {
        trace!(actor = ?self.node_handle, fd = fd, "aclose");

        // Look up and remove the channel handle for this fd
        let channel_handle = match self.fd_table.lock().unwrap().remove(fd) {
            Some(h) => h,
            None => {
                warn!(actor = ?self.node_handle, fd = fd, "aclose: fd not found");
                return -1;
            }
        };

        // Send request to SystemRuntime and block for response
        let (tx, rx) = oneshot::channel();

        self.system_tx
            .send(IoRequest::Close {
                handle: channel_handle,
                response: tx,
            })
            .unwrap();

        trace!(actor = ?self.node_handle, "aclose: blocking_recv");
        let result = rx.blocking_recv().unwrap();
        trace!(actor = ?self.node_handle, result = result, "aclose done");
        result
    }
}
