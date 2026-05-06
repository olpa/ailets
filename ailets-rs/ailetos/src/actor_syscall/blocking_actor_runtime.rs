//! Blocking `ActorRuntime` implementation
//!
//! This module provides a blocking `ActorRuntime` implementation — the user-space
//! side of the actor syscall layer. It is a thin stateless wrapper that passes
//! all I/O operations to `IoBridge` with the actor's node handle and fd.
//!
//! Among the consumers of this type is the WASM interface: `BlockingActorRuntime`
//! is threaded through FFI glue into `FfiActorRuntime`, which exposes it to WebAssembly actors.

use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;

use actor_runtime::ActorRuntime;
use parking_lot::Mutex;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, warn};

use super::io_bridge::IoBridge;
use super::lifecycle_event::ActorLifecycleEvent;
use super::sendable_buffer::{SendableConstPtr, SendableMutPtr};
use crate::dag::{NodeState, OwnedDependencyIterator};
use crate::errno::EOWNERDEAD;
use crate::idgen::Handle;
use crate::suspension::SuspensionState;

/// Blocking `ActorRuntime` implementation.
///
/// Thin stateless wrapper around `IoBridge`. All I/O state lives in `IoBridge`.
/// Fires actor shutdown automatically on drop — including on panic or early return.
pub struct BlockingActorRuntime {
    /// This actor's node handle (used as actor identifier)
    node_handle: Handle,
    io_bridge: Arc<IoBridge>,
    /// Shared suspension state (owned by Environment)
    suspension: Arc<SuspensionState>,
    /// errno from the last failed syscall (0 = no error)
    last_errno: AtomicI32,
    /// 0 = clean termination; non-zero = POSIX errno
    exit_code: AtomicI32,
    /// Pre-built dependency iterator for stdin; consumed on first read
    stdin_dep_iterator: Mutex<Option<OwnedDependencyIterator>>,
    /// Channel to notify executor of lifecycle events (Terminating/Terminated)
    actor_done_tx: mpsc::UnboundedSender<ActorLifecycleEvent>,
}

impl Drop for BlockingActorRuntime {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl BlockingActorRuntime {
    #[must_use]
    pub fn new(
        node_handle: Handle,
        io_bridge: Arc<IoBridge>,
        suspension: Arc<SuspensionState>,
        stdin_dep_iterator: OwnedDependencyIterator,
        actor_done_tx: mpsc::UnboundedSender<ActorLifecycleEvent>,
    ) -> Self {
        Self {
            node_handle,
            io_bridge,
            suspension,
            last_errno: AtomicI32::new(0),
            exit_code: AtomicI32::new(0),
            stdin_dep_iterator: Mutex::new(Some(stdin_dep_iterator)),
            actor_done_tx,
        }
    }

    fn shutdown(&self) {
        // Notify executor we're terminating
        let (tx, rx) = oneshot::channel::<NodeState>();
        if self
            .actor_done_tx
            .send(ActorLifecycleEvent::Terminating {
                node_handle: self.node_handle,
                reply: tx,
            })
            .is_err()
        {
            warn!(node = ?self.node_handle, "shutdown: executor gone");
            return;
        }

        let prior = rx.blocking_recv().unwrap_or(NodeState::Terminating);
        if matches!(prior, NodeState::Terminating | NodeState::Terminated) {
            debug!(node = ?self.node_handle, "shutdown: already terminating/terminated");
            return;
        }

        // Cleanup
        let exit_code = self.exit_code.load(Ordering::Relaxed);
        self.suspension.deregister(self.node_handle);
        self.io_bridge.cleanup_actor_io(self.node_handle, exit_code);

        // Notify executor we're terminated
        let (tx2, rx2) = oneshot::channel::<NodeState>();
        if self
            .actor_done_tx
            .send(ActorLifecycleEvent::Terminated {
                node_handle: self.node_handle,
                exit_code,
                reply: tx2,
            })
            .is_err()
        {
            warn!(node = ?self.node_handle, "shutdown: executor gone before Terminated");
            return;
        }
        if rx2.blocking_recv().is_err() {
            warn!(node = ?self.node_handle, "shutdown: Terminated reply dropped");
        }
    }

    /// Mark the actor as failed.
    ///
    /// Uses the errno from the last failed syscall if set, otherwise falls back to EOWNERDEAD.
    pub fn mark_failed(&self) {
        let errno = self.last_errno.load(Ordering::Relaxed);
        let code = if errno != 0 { errno } else { EOWNERDEAD };
        self.exit_code.store(code, Ordering::Relaxed);
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
    /// Marks standard fds as allowed in IoBridge; actual channels are materialized lazily.
    pub fn register_std_fds(&self) {
        use actor_runtime::StdHandle;
        use super::io_bridge::FdState;

        // Readers
        self.io_bridge.register_std_fd(self.node_handle, FdState::AllowedReader(StdHandle::Stdin));
        self.io_bridge.register_std_fd(self.node_handle, FdState::AllowedReader(StdHandle::Env));

        // Writers
        self.io_bridge.register_std_fd(self.node_handle, FdState::AllowedWriter(StdHandle::Stdout));
        self.io_bridge.register_std_fd(self.node_handle, FdState::AllowedWriter(StdHandle::Log));
        self.io_bridge.register_std_fd(self.node_handle, FdState::AllowedWriter(StdHandle::Metrics));
        self.io_bridge.register_std_fd(self.node_handle, FdState::AllowedWriter(StdHandle::Trace));
    }

    fn bridged_io(&self, f: impl FnOnce() -> (isize, i32)) -> isize {
        self.yield_if_suspended();
        let (result, errno) = f();
        if result < 0 && errno != 0 {
            self.last_errno.store(errno, Ordering::Relaxed);
        }
        self.yield_if_suspended();
        result
    }
}

impl ActorRuntime for BlockingActorRuntime {
    fn get_errno(&self) -> isize {
        self.last_errno.load(Ordering::Relaxed) as isize
    }

    fn open_read(&self, _name: &str) -> isize {
        // Dynamic file opening not supported yet
        // Standard fds (0, 1, 2) are pre-opened
        warn!(actor = ?self.node_handle, name = _name, "open_read: dynamic fd allocation not supported");
        -1
    }

    fn open_write(&self, _name: &str) -> isize {
        // Dynamic file opening not supported yet
        // Standard fds (0, 1, 2) are pre-opened
        warn!(actor = ?self.node_handle, name = _name, "open_write: dynamic fd allocation not supported");
        -1
    }

    fn aread(&self, fd: isize, buffer: &mut [u8]) -> isize {
        use actor_runtime::StdHandle;

        // Only pass dep_iterator for stdin (fd=0)
        let dep_iterator = if fd == StdHandle::Stdin as isize {
            self.stdin_dep_iterator.lock().take()
        } else {
            None
        };

        // SAFETY: buffer is valid for the duration of blocking_recv inside bridge.read
        let buffer_ptr = unsafe { SendableMutPtr::new(buffer) };
        self.bridged_io(|| {
            self.io_bridge
                .read(self.node_handle, fd, buffer_ptr, dep_iterator)
        })
    }

    fn awrite(&self, fd: isize, buffer: &[u8]) -> isize {
        // SAFETY: buffer is valid for the duration of blocking_recv inside bridge.write
        let buffer_ptr = unsafe { SendableConstPtr::new(buffer) };
        self.bridged_io(|| self.io_bridge.write(self.node_handle, fd, buffer_ptr))
    }

    fn aclose(&self, fd: isize) -> isize {
        self.bridged_io(|| self.io_bridge.close(self.node_handle, fd))
    }

    fn node_handle(&self) -> i64 {
        self.node_handle.id()
    }

    fn suspend_and_wait(&self) {
        self.suspension.self_suspend_and_wait(self.node_handle);
    }
}
