//! Blocking `ActorRuntime` implementation
//!
//! This module provides a blocking `ActorRuntime` implementation — the user-space
//! side of the actor syscall layer. It routes all I/O operations to `IoBridge`
//! with the actor's node handle and fd, and tracks per-actor state: the exit code
//! and whether `shutdown()` has been called.
//!
//! `shutdown()` must be called explicitly before drop. It flushes writer buffers
//! and sends the two-phase Terminating/Terminated handshake to the executor.
//!
//! Among the consumers of this type is the WASM interface: `BlockingActorRuntime`
//! is threaded through FFI glue into `FfiActorRuntime`, which exposes it to WebAssembly actors.

use std::sync::Arc;

use actor_runtime::ActorRuntime;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error, warn};

use super::io_bridge::IoBridge;
use super::lifecycle_event::ActorLifecycleEvent;
use super::sendable_buffer::{SendableConstPtr, SendableMutPtr};
use crate::dag::NodeState;
use crate::env_service::EnvService;
use crate::errno::ENOSYS;
use crate::idgen::Handle;
use crate::suspension::SuspensionState;

/// Blocking `ActorRuntime` implementation.
///
/// Thin stateless wrapper around `IoBridge`. All I/O state lives in `IoBridge`.
///
/// **Important:** `shutdown()` MUST be called explicitly before drop to ensure
/// proper cleanup and data persistence. Drop will log an error if shutdown was not called.
pub struct BlockingActorRuntime {
    /// This actor's node handle (used as actor identifier)
    node_handle: Handle,
    io_bridge: Arc<IoBridge>,
    /// Shared suspension state (owned by Environment)
    suspension: Arc<SuspensionState>,
    /// Shared internal environment variables (owned by Environment)
    env_service: Arc<EnvService>,
    /// 0 = clean termination; non-zero = POSIX errno
    exit_code: i32,
    /// Channel to notify executor of lifecycle events (Terminating/Terminated)
    actor_done_tx: mpsc::UnboundedSender<ActorLifecycleEvent>,
    /// True if `shutdown()` was called (to detect improper drop)
    shutdown_called: bool,
}

impl Drop for BlockingActorRuntime {
    fn drop(&mut self) {
        if !self.shutdown_called {
            error!(
                node = ?self.node_handle,
                "BlockingActorRuntime dropped without calling shutdown() - \
                 buffered data will be LOST! This is a bug in the executor."
            );
        }
    }
}

impl BlockingActorRuntime {
    #[must_use]
    pub fn new(
        node_handle: Handle,
        io_bridge: Arc<IoBridge>,
        suspension: Arc<SuspensionState>,
        env_service: Arc<EnvService>,
        actor_done_tx: mpsc::UnboundedSender<ActorLifecycleEvent>,
    ) -> Self {
        Self {
            node_handle,
            io_bridge,
            suspension,
            env_service,
            exit_code: 0,
            actor_done_tx,
            shutdown_called: false,
        }
    }

    /// Shutdown the actor runtime, flushing all buffers to persistent storage.
    ///
    /// **MUST be called before drop** to ensure data persistence. The executor
    /// is responsible for calling this method before dropping the runtime.
    ///
    /// This method is async because it flushes writer buffers to storage,
    /// which may involve disk I/O for persistent backends like `SQLite`.
    ///
    /// # Errors
    ///
    /// Returns `Err` if:
    /// - Shutdown was already called
    /// - Executor is gone
    /// - Actor was already terminating/terminated
    pub async fn shutdown(&mut self) -> Result<(), String> {
        // Mark shutdown as called
        if self.shutdown_called {
            return Err("shutdown() already called".to_string());
        }
        self.shutdown_called = true;

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
            return Err("executor gone".to_string());
        }

        let prior = rx
            .await
            .map_err(|_| "Terminating reply dropped".to_string())?;
        if matches!(prior, NodeState::Terminating | NodeState::Terminated) {
            debug!(node = ?self.node_handle, "shutdown: already terminating/terminated");
            return Ok(());
        }

        // Async cleanup - flushes all writer buffers
        let exit_code = self.exit_code;
        self.suspension.deregister(self.node_handle);
        let cleanup_result = self
            .io_bridge
            .cleanup_actor_io(self.node_handle, exit_code)
            .await;

        // Notify executor we're terminated — always send, even if cleanup failed,
        // so the executor is never left waiting for an event that will never arrive.
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
            return Err("executor gone before Terminated".to_string());
        }

        rx2.await
            .map_err(|_| "Terminated reply dropped".to_string())?;
        cleanup_result
    }

    /// Set the exit code if not already set. First error wins.
    pub fn latch_errno(&mut self, errno: i32) {
        if self.exit_code == 0 {
            self.exit_code = errno;
        }
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
    /// Marks standard fds as allowed in `IoBridge`; actual channels are materialized lazily.
    pub fn register_std_fds(&self) {
        use actor_runtime::StdHandle;

        // Readers
        self.io_bridge
            .register_std_fd_reader(self.node_handle, StdHandle::Stdin as isize);
        self.io_bridge
            .register_std_fd_reader(self.node_handle, StdHandle::Env as isize);

        // Writers
        self.io_bridge
            .register_std_fd_writer(self.node_handle, StdHandle::Stdout as isize);
        self.io_bridge
            .register_std_fd_writer(self.node_handle, StdHandle::Log as isize);
        self.io_bridge
            .register_std_fd_writer(self.node_handle, StdHandle::Metrics as isize);
        self.io_bridge
            .register_std_fd_writer(self.node_handle, StdHandle::Trace as isize);
    }
}

impl ActorRuntime for BlockingActorRuntime {
    fn open_read(&self, name: &str) -> Result<isize, i32> {
        warn!(actor = ?self.node_handle, name = name, "open_read: dynamic fd allocation not supported");
        Err(ENOSYS)
    }

    fn open_write(&self, name: &str) -> Result<isize, i32> {
        warn!(actor = ?self.node_handle, name = name, "open_write: dynamic fd allocation not supported");
        Err(ENOSYS)
    }

    fn aread(&self, fd: isize, buffer: &mut [u8]) -> Result<usize, i32> {
        self.yield_if_suspended();
        // SAFETY: buffer is valid for the duration of blocking_recv inside bridge.read
        let buffer_ptr = unsafe { SendableMutPtr::new(buffer) };
        let result = self.io_bridge.read(self.node_handle, fd, buffer_ptr);
        self.yield_if_suspended();
        result
    }

    fn awrite(&self, fd: isize, buffer: &[u8]) -> Result<usize, i32> {
        self.yield_if_suspended();
        // SAFETY: buffer is valid for the duration of blocking_recv inside bridge.write
        let buffer_ptr = unsafe { SendableConstPtr::new(buffer) };
        let result = self.io_bridge.write(self.node_handle, fd, buffer_ptr);
        self.yield_if_suspended();
        result
    }

    fn aclose(&self, fd: isize) -> Result<(), i32> {
        self.yield_if_suspended();
        let result = self.io_bridge.close(self.node_handle, fd);
        self.yield_if_suspended();
        result
    }

    fn node_handle(&self) -> i64 {
        self.node_handle.id()
    }

    fn suspend_and_wait(&self) {
        self.suspension.self_suspend_and_wait(self.node_handle);
    }

    fn get_env(&self, key: &str) -> Option<String> {
        self.env_service.get(key)
    }
}
