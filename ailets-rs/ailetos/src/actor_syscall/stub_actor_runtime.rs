//! Blocking `ActorRuntime` implementation
//!
//! This module provides a blocking `ActorRuntime` implementation — the user-space
//! side of the actor syscall layer. It holds per-actor state (fd table) and calls
//! `IoBridge` methods directly for all I/O operations.

use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;

use actor_runtime::ActorRuntime;
use parking_lot::Mutex;
use tracing::{error, warn};

use crate::dag::OwnedDependencyIterator;
use crate::errno::{EBADF, EIO, EOWNERDEAD};
use crate::idgen::Handle;
use crate::suspension::SuspensionState;
use super::fd_table::{FdEntry, FdTable};
use super::io_bridge::{ChannelHandle, IoBridge};
use super::sendable_buffer::SendableBuffer;

/// Blocking `ActorRuntime` implementation.
///
/// Holds per-actor state and calls `IoBridge` directly for all I/O.
pub struct BlockingActorRuntime {
    /// This actor's node handle (used as actor identifier)
    node_handle: Handle,
    bridge: Arc<IoBridge>,
    /// Per-actor fd table (POSIX fd → global `ChannelHandle`)
    fd_table: Mutex<FdTable>,
    /// Shared suspension state (owned by Environment)
    suspension: Arc<SuspensionState>,
    /// errno from the last failed read (0 = no error); shared with `ShutdownHandle`
    last_errno: Arc<AtomicI32>,
    /// Pre-built dependency iterator for stdin; consumed on first read
    stdin_dep_iterator: Mutex<Option<OwnedDependencyIterator>>,
}

/// Lifecycle handle that calls `IoBridge::actor_shutdown` when dropped.
///
/// Returned alongside `BlockingActorRuntime` from `BlockingActorRuntime::new`.
/// Drop fires shutdown automatically — including on panic or early return.
/// Calling `actor_shutdown` multiple times is safe — `IoBridge` is idempotent.
pub struct ShutdownHandle {
    node_handle: Handle,
    bridge: Arc<IoBridge>,
    suspension: Arc<SuspensionState>,
    /// 0 = clean termination; non-zero = POSIX errno
    exit_code: Arc<AtomicI32>,
    /// errno from the last failed read; shared with `BlockingActorRuntime`
    last_errno: Arc<AtomicI32>,
}

impl ShutdownHandle {
    /// Mark the actor as failed.
    ///
    /// Uses the errno from the last failed read if set (per `<spec://errors#reader-to-actor>`),
    /// otherwise falls back to EOWNERDEAD.
    pub fn mark_failed(&self) {
        let read_errno = self.last_errno.load(Ordering::Relaxed);
        let code = if read_errno != 0 { read_errno } else { EOWNERDEAD };
        self.exit_code.store(code, Ordering::Relaxed);
    }
}

impl Drop for ShutdownHandle {
    /// Fires shutdown unconditionally — safe to call after an explicit shutdown().
    fn drop(&mut self) {
        self.suspension.deregister(self.node_handle);
        let exit_code = self.exit_code.load(Ordering::Relaxed);
        self.bridge.actor_shutdown(self.node_handle, exit_code);
    }
}

impl BlockingActorRuntime {
    /// Create a new `BlockingActorRuntime` for the given node handle.
    ///
    /// Returns the runtime together with a `ShutdownHandle`. The handle fires
    /// cleanup automatically on drop if never called explicitly.
    #[must_use]
    pub fn new(
        node_handle: Handle,
        bridge: Arc<IoBridge>,
        suspension: Arc<SuspensionState>,
        stdin_dep_iterator: OwnedDependencyIterator,
    ) -> (Self, ShutdownHandle) {
        let exit_code = Arc::new(AtomicI32::new(0));
        let last_errno = Arc::new(AtomicI32::new(0));
        let runtime = Self {
            node_handle,
            bridge: Arc::clone(&bridge),
            fd_table: Mutex::new(FdTable::new()),
            suspension: Arc::clone(&suspension),
            last_errno: Arc::clone(&last_errno),
            stdin_dep_iterator: Mutex::new(Some(stdin_dep_iterator)),
        };
        let shutdown = ShutdownHandle {
            node_handle,
            bridge,
            suspension,
            exit_code,
            last_errno,
        };
        (runtime, shutdown)
    }

    /// Yield cooperatively if this actor has been suspended; blocks until resumed.
    fn yield_if_suspended(&self) {
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

        let mut table = self.fd_table.lock();

        // Readers
        table.set(StdHandle::Stdin as isize, FdEntry::AllowedReader);
        table.set(StdHandle::Env as isize, FdEntry::AllowedReader);
        // Writers
        table.set(StdHandle::Stdout as isize, FdEntry::AllowedWriter);
        table.set(StdHandle::Log as isize, FdEntry::AllowedWriter);
        table.set(StdHandle::Metrics as isize, FdEntry::AllowedWriter);
        table.set(StdHandle::Trace as isize, FdEntry::AllowedWriter);
    }

    fn materialize_stdin_handle(&self, fd: isize) -> Option<ChannelHandle> {
        let dep_iterator = match self.stdin_dep_iterator.lock().take() {
            Some(it) => it,
            None => {
                error!(actor = ?self.node_handle, "aread: stdin iterator already consumed");
                return None;
            }
        };
        let handle = self.bridge.materialize_stdin(self.node_handle, dep_iterator);
        if let Some(entry) = self.fd_table.lock().get_mut(fd) {
            *entry = FdEntry::ActiveReader(handle);
        }
        Some(handle)
    }
}

impl ActorRuntime for BlockingActorRuntime {
    fn get_errno(&self) -> isize {
        self.last_errno.load(Ordering::Relaxed) as isize
    }

    fn open_read(&self, _name: &str) -> isize {
        let channel_handle = self.bridge.open_read(self.node_handle);
        let (fd, errno) = self.fd_table.lock().insert(FdEntry::ActiveReader(channel_handle));
        if fd < 0 {
            self.last_errno.store(errno, Ordering::Relaxed);
        }
        fd
    }

    fn open_write(&self, _name: &str) -> isize {
        // For now, open_write creates an ActiveWriter directly with Stdout.
        // TODO: Support named streams (map _name to the appropriate StdHandle).
        let (fd, errno) = self.fd_table.lock().insert(FdEntry::ActiveWriter {
            node_handle: self.node_handle,
            std_handle: actor_runtime::StdHandle::Stdout,
        });
        if fd < 0 {
            self.last_errno.store(errno, Ordering::Relaxed);
        }
        fd
    }

    fn aread(&self, fd: isize, buffer: &mut [u8]) -> isize {
        // Get the channel handle, materializing stdin if needed
        let channel_handle = {
            let guard = self.fd_table.lock();
            match guard.get(fd) {
                Some(FdEntry::ActiveReader(handle)) => *handle,
                Some(FdEntry::AllowedReader) => {
                    drop(guard);
                    match self.materialize_stdin_handle(fd) {
                        Some(h) => h,
                        None => {
                            self.last_errno.store(EIO, Ordering::Relaxed);
                            return -1;
                        }
                    }
                }
                Some(FdEntry::AllowedWriter | FdEntry::ActiveWriter { .. }) => {
                    warn!(actor = ?self.node_handle, fd = fd, "aread: cannot read from stdout");
                    self.last_errno.store(EBADF, Ordering::Relaxed);
                    return -1;
                }
                None => {
                    warn!(actor = ?self.node_handle, fd = fd, "aread: fd not found");
                    self.last_errno.store(EBADF, Ordering::Relaxed);
                    return -1;
                }
            }
        };

        // Yield if suspended before issuing the read
        self.yield_if_suspended();

        // SAFETY: buffer is valid for the duration of blocking_recv inside bridge.read
        let buffer_ptr = unsafe { SendableBuffer::new(buffer) };
        let (result, errno) = self.bridge.read(channel_handle, buffer_ptr);

        if result < 0 && errno != 0 {
            self.last_errno.store(errno, Ordering::Relaxed);
        }

        // Yield if suspended after the read completes
        self.yield_if_suspended();
        result
    }

    fn awrite(&self, fd: isize, buffer: &[u8]) -> isize {
        // Get the write info, upgrading AllowedWriter to ActiveWriter on first use
        let (node_handle, std_handle) = {
            let mut table = self.fd_table.lock();
            match table.get(fd) {
                Some(FdEntry::ActiveWriter { node_handle, std_handle }) => (*node_handle, *std_handle),
                Some(FdEntry::AllowedWriter) => {
                    // Upgrade to ActiveWriter
                    let nh = self.node_handle;
                    let sh = actor_runtime::StdHandle::Stdout;
                    if let Some(entry) = table.get_mut(fd) {
                        *entry = FdEntry::ActiveWriter { node_handle: nh, std_handle: sh };
                    }
                    (nh, sh)
                }
                Some(FdEntry::AllowedReader | FdEntry::ActiveReader(_)) => {
                    warn!(actor = ?self.node_handle, fd = fd, "awrite: cannot write to stdin");
                    self.last_errno.store(EBADF, Ordering::Relaxed);
                    return -1;
                }
                None => {
                    warn!(actor = ?self.node_handle, fd = fd, "awrite: fd not found");
                    self.last_errno.store(EBADF, Ordering::Relaxed);
                    return -1;
                }
            }
        };

        // Yield if suspended before issuing the write
        self.yield_if_suspended();
        let (result, errno) = self.bridge.write(node_handle, std_handle, buffer.to_vec());

        if result < 0 && errno != 0 {
            self.last_errno.store(errno, Ordering::Relaxed);
        }

        // Yield if suspended after the write completes
        self.yield_if_suspended();
        result
    }

    fn aclose(&self, fd: isize) -> isize {
        // Remove the fd entry
        let entry = match self.fd_table.lock().remove(fd) {
            Some(e) => e,
            None => {
                warn!(actor = ?self.node_handle, fd = fd, "aclose: fd not found");
                self.last_errno.store(EBADF, Ordering::Relaxed);
                return -1;
            }
        };

        // Handle based on entry type
        match entry {
            // Never materialized — nothing to close
            FdEntry::AllowedReader | FdEntry::AllowedWriter => 0,
            FdEntry::ActiveReader(channel_handle) => {
                self.yield_if_suspended();
                let (result, errno) = self.bridge.close(channel_handle);
                if result < 0 && errno != 0 {
                    self.last_errno.store(errno, Ordering::Relaxed);
                }
                self.yield_if_suspended();
                result
            }
            FdEntry::ActiveWriter { node_handle, std_handle } => {
                self.yield_if_suspended();
                let (result, errno) = self.bridge.close_writer(node_handle, std_handle);
                if result < 0 && errno != 0 {
                    self.last_errno.store(errno, Ordering::Relaxed);
                }
                self.yield_if_suspended();
                result
            }
        }
    }

    fn node_handle(&self) -> i64 {
        self.node_handle.id()
    }

    fn suspend_and_wait(&self) {
        self.suspension.self_suspend_and_wait(self.node_handle);
    }
}
