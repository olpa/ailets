//! Async half of the actor syscall bridge.
//!
//! `IoBridge` is a shared object that actors call directly to perform I/O.
//! It is one half of the actor syscall layer; the other half is
//! `blocking_actor_runtime`, which runs on each actor's blocking thread.
//!
//! Supporting types live in sibling modules:
//! - [`super::sendable_buffer`] — zero-copy buffer pointer for read operations
//! - [`super::lifecycle_event`] — two-phase shutdown handshake with the executor
//!
//! # Calling model
//!
//! All public methods are sync and callable from a blocking thread (spawned via
//! `tokio::task::spawn_blocking`). Methods that need async work spawn a Tokio task
//! and block the calling thread on a oneshot reply:
//!
//! ```text
//! actor thread (blocking)       Tokio runtime
//! ─────────────────────────     ─────────────────────────────
//! bridge.write(data)
//!   tokio::spawn ──────────→   async write task
//!   blocking_recv() …           → sends oneshot reply
//!   ← result                   task exits
//! ```
//!
//! Read operations go through a long-lived reader task (one per stdin channel)
//! that owns the [`MergeReader`]. The actor sends a [`ReadRequest`] and blocks:
//!
//! ```text
//! actor thread                  reader task (Tokio)
//! ─────────────────────────     ─────────────────────
//! bridge.read(handle, buf)
//!   → ReadRequest ──────────→  MergeReader::read(buf).await
//!   blocking_recv() …          → oneshot reply
//!   ← (bytes, errno)
//! ```

use std::sync::Arc;

use actor_runtime::StdHandle;
use parking_lot::Mutex;
use tokio::sync::{mpsc, oneshot, Notify};
use tracing::{debug, warn};

use super::sendable_buffer::{SendableConstPtr, SendableMutPtr};
use crate::attachments::AttachmentManager;
use crate::dag::OwnedDependencyIterator;
use crate::environment::Environment;
use crate::errno::{EBADF, EIO, EPIPE};
use crate::idgen::Handle;
use crate::pipe::MergeReader;


/// A read request forwarded from `IoBridge` to a reader task.
pub(crate) struct ReadRequest {
    pub buffer: SendableMutPtr,
    pub response: oneshot::Sender<(isize, i32)>,
}

/// A write request forwarded from `IoBridge` to a writer task.
pub(crate) struct WriteRequest {
    pub data: SendableConstPtr,
    pub response: oneshot::Sender<(isize, i32)>,
}

/// Long-lived task that owns a `MergeReader` and services read requests for one channel.
///
/// Spawned by `IoBridge::materialize_stdin`. Exits when its `request_rx` closes —
/// i.e., when the channel entry is removed from `ChannelTable` on close or shutdown.
async fn run_reader_task(
    node_handle: Handle,
    mut reader: MergeReader,
    mut request_rx: mpsc::UnboundedReceiver<ReadRequest>,
) {
    while let Some(ReadRequest { buffer, response }) = request_rx.recv().await {
        // SAFETY: Buffer remains valid because aread() blocks until response is sent
        let buf = unsafe { &mut *buffer.into_raw() };
        let bytes_read = reader.read(buf).await;
        let errno = if bytes_read < 0 {
            reader.get_error()
        } else {
            0
        };
        if response.send((bytes_read, errno)).is_err() {
            warn!(actor = ?node_handle, "reader task: reply receiver dropped, actor may have exited");
        }
    }
}

/// Long-lived task that owns access to a `Writer` and services write requests for one channel.
///
/// Spawned by `IoBridge::open_write`. Exits when its `request_rx` closes —
/// i.e., when the channel entry is removed from `ChannelTable` on close or shutdown.
async fn run_writer_task(
    node_handle: Handle,
    fd: isize,
    env: Arc<Environment>,
    attachment_manager: Arc<AttachmentManager>,
    notify: Arc<Notify>,
    mut request_rx: mpsc::UnboundedReceiver<WriteRequest>,
) {
    // Get/create writer once at task startup
    let writer = match env.pipe_pool.touch_writer(node_handle, fd, &env.idgen).await {
        Ok((writer, is_new)) => {
            if is_new {
                attachment_manager.on_writer_realized(
                    node_handle,
                    fd,
                    Arc::clone(&env.pipe_pool),
                    Arc::clone(&env.idgen),
                );
            }
            writer
        }
        Err(e) => {
            warn!(node = ?node_handle, fd = fd, error = %e, "writer task: failed to create writer");
            // Send EIO to all incoming requests
            while let Some(WriteRequest { response, .. }) = request_rx.recv().await {
                let _ = response.send((-1, EIO));
            }
            return;
        }
    };

    // Process write requests
    while let Some(WriteRequest { data, response }) = request_rx.recv().await {
        // SAFETY: Buffer remains valid because awrite() blocks until response is sent
        let data_slice = unsafe { &*data.into_raw() };
        let n = writer.write(data_slice);
        let result = if n < 0 {
            (-1, writer.get_error())
        } else {
            (n, 0)
        };
        notify.notify_one();
        if response.send(result).is_err() {
            warn!(node = ?node_handle, fd = fd, "writer task: reply receiver dropped, actor may have exited");
        }
    }
}

/// State of an fd for an actor
pub(crate) enum FdState {
    /// Reader fd allowed but not yet materialized
    AllowedReader,
    /// Writer fd allowed but not yet materialized
    AllowedWriter,
    /// Reader has been materialized with a reader task
    MaterializedReader {
        request_tx: mpsc::UnboundedSender<ReadRequest>,
    },
    /// Writer has been materialized with a writer task
    MaterializedWriter {
        request_tx: mpsc::UnboundedSender<WriteRequest>,
    },
}

/// Global table of open channel endpoints, analogous to the kernel's open-file-descriptions table.
///
/// Maps (node_handle, fd) → FdState using a Vec with linear search.
/// Efficient for small N (few fds per actor) with excellent cache locality.
struct ChannelTable {
    /// Vector of (node_handle, fd, state) tuples
    entries: Vec<(Handle, isize, FdState)>,
}

impl ChannelTable {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Get the state of an fd for an actor
    fn get_state(&self, node_handle: Handle, fd: isize) -> Option<&FdState> {
        self.entries
            .iter()
            .find(|(h, f, _)| (*h, *f) == (node_handle, fd))
            .map(|(_, _, state)| state)
    }

    /// Get mutable reference to the state of an fd for an actor
    fn get_state_mut(&mut self, node_handle: Handle, fd: isize) -> Option<&mut FdState> {
        self.entries
            .iter_mut()
            .find(|(h, f, _)| (*h, *f) == (node_handle, fd))
            .map(|(_, _, state)| state)
    }

    /// Get writer request sender for (node_handle, fd) if materialized
    fn get_writer_tx(
        &self,
        node_handle: Handle,
        fd: isize,
    ) -> Option<&mpsc::UnboundedSender<WriteRequest>> {
        match self.get_state(node_handle, fd)? {
            FdState::MaterializedWriter { request_tx, .. } => Some(request_tx),
            _ => None,
        }
    }

    /// Remove entry for (node_handle, fd), returning the state
    fn remove(&mut self, node_handle: Handle, fd: isize) -> Option<FdState> {
        let pos = self
            .entries
            .iter()
            .position(|(h, f, _)| (*h, *f) == (node_handle, fd))?;
        let (_, _, state) = self.entries.remove(pos);
        Some(state)
    }

    /// Drop all entries for a given actor
    fn drop_actor_entries(&mut self, node_handle: Handle) {
        self.entries.retain(|(h, _, _)| *h != node_handle);
    }
}

/// Directly-callable I/O bridge between actor threads and the async runtime.
///
/// Held as `Arc<IoBridge>` by each actor's `BlockingActorRuntime` and `ShutdownHandle`.
/// All methods are safe to call from a blocking thread.
pub struct IoBridge {
    env: Arc<Environment>,
    channel_table: Mutex<ChannelTable>,
    attachment_manager: Arc<AttachmentManager>,
    /// Notifies the spawn loop when I/O state changes that may affect node readiness.
    /// Use cases:
    /// - a pipe is realized
    /// - a writer is closed
    notify: Arc<Notify>,
}

impl IoBridge {
    #[must_use]
    pub fn new(env: Arc<Environment>, notify: Arc<Notify>) -> Self {
        let attachment_manager =
            Arc::new(AttachmentManager::new(env.attachment_config.read().clone()));
        Self {
            env,
            channel_table: Mutex::new(ChannelTable::new()),
            attachment_manager,
            notify,
        }
    }

    /// Register a reader fd. Task is spawned lazily on first use. No duplicate check.
    pub(crate) fn register_std_fd_reader(&self, node_handle: Handle, fd: isize) {
        self.channel_table
            .lock()
            .entries
            .push((node_handle, fd, FdState::AllowedReader));
    }

    /// Register a writer fd. Task is spawned lazily on first use. No duplicate check.
    pub(crate) fn register_std_fd_writer(&self, node_handle: Handle, fd: isize) {
        self.channel_table
            .lock()
            .entries
            .push((node_handle, fd, FdState::AllowedWriter));
    }

    /// Materialize a reader. Caller must have verified state is AllowedReader.
    /// For stdin, creates a MergeReader with dependencies from the DAG.
    /// Returns the request sender, or None if materialization fails.
    fn materialize_reader(
        &self,
        table_guard: &mut ChannelTable,
        node_handle: Handle,
        fd: isize,
    ) -> Option<mpsc::UnboundedSender<ReadRequest>> {
        // Only stdin is supported for now
        if fd != StdHandle::Stdin as isize {
            warn!(actor = ?node_handle, fd = fd, "reader materialization only supported for stdin");
            return None;
        }

        table_guard.remove(node_handle, fd);

        debug!(actor = ?node_handle, fd = fd, "materializing stdin reader");
        let dep_iterator = OwnedDependencyIterator::new(Arc::clone(&self.env.dag), node_handle);
        let reader = MergeReader::new(
            dep_iterator,
            Arc::clone(&self.env.pipe_pool),
            Arc::clone(&self.env.kv),
            Arc::clone(&self.env.idgen),
        );
        let (request_tx, request_rx) = mpsc::unbounded_channel::<ReadRequest>();
        tokio::spawn(run_reader_task(node_handle, reader, request_rx));

        let tx = request_tx.clone();
        table_guard
            .entries
            .push((node_handle, fd, FdState::MaterializedReader { request_tx }));
        Some(tx)
    }

    /// Ensure writer task is spawned for (node_handle, fd).
    /// Called lazily on the actor's first write to the fd.
    /// Returns true if successful, false if fd is not an allowed writer.
    fn ensure_writer_materialized(&self, node_handle: Handle, fd: isize) -> bool {
        let mut table = self.channel_table.lock();

        // Check current state
        match table.get_state(node_handle, fd) {
            Some(FdState::AllowedWriter) => {}
            Some(FdState::MaterializedWriter { .. }) => return true,
            Some(FdState::AllowedReader | FdState::MaterializedReader { .. }) | None => {
                return false;
            }
        }

        debug!(actor = ?node_handle, fd = fd, "materializing writer");
        let (request_tx, request_rx) = mpsc::unbounded_channel::<WriteRequest>();

        let env = Arc::clone(&self.env);
        let attachment_manager = Arc::clone(&self.attachment_manager);
        let notify = Arc::clone(&self.notify);
        tokio::spawn(run_writer_task(
            node_handle,
            fd,
            env,
            attachment_manager,
            notify,
            request_rx,
        ));

        if let Some(state) = table.get_state_mut(node_handle, fd) {
            *state = FdState::MaterializedWriter { request_tx };
        }

        true
    }

    /// Route a read request to the channel's reader task and block for the result.
    /// Materializes reader lazily on first call.
    pub fn read(&self, node_handle: Handle, fd: isize, buffer: SendableMutPtr) -> (isize, i32) {
        let tx = {
            let mut table_guard = self.channel_table.lock();
            match table_guard.get_state(node_handle, fd) {
                Some(FdState::MaterializedReader { request_tx }) => request_tx.clone(),
                Some(FdState::AllowedReader) => {
                    match self.materialize_reader(&mut table_guard, node_handle, fd) {
                        Some(tx) => tx,
                        None => return (-1, EBADF),
                    }
                }
                Some(FdState::AllowedWriter | FdState::MaterializedWriter { .. }) => {
                    warn!(node = ?node_handle, fd = fd, "read: cannot read from writer fd");
                    return (-1, EBADF);
                }
                None => {
                    warn!(node = ?node_handle, fd = fd, "read: fd not registered");
                    return (-1, EBADF);
                }
            }
        };

        let (resp_tx, resp_rx) = oneshot::channel();
        if tx
            .send(ReadRequest {
                buffer,
                response: resp_tx,
            })
            .is_err()
        {
            warn!(node = ?node_handle, fd = fd, "reader task has exited");
            return (-1, EPIPE);
        }
        resp_rx.blocking_recv().unwrap_or((-1, EPIPE))
    }

    /// Route a write request to the channel's writer task and block for the result.
    /// Materializes writer lazily on first call.
    pub fn write(&self, node_handle: Handle, fd: isize, data: SendableConstPtr) -> (isize, i32) {
        // Lazy materialization of writer (also validates it's a writer fd)
        if !self.ensure_writer_materialized(node_handle, fd) {
            warn!(node = ?node_handle, fd = fd, "write: fd not a writer or not registered");
            return (-1, EBADF);
        }

        let tx = self
            .channel_table
            .lock()
            .get_writer_tx(node_handle, fd)
            .cloned();
        if let Some(tx) = tx {
            let (resp_tx, resp_rx) = oneshot::channel();
            if tx
                .send(WriteRequest {
                    data,
                    response: resp_tx,
                })
                .is_err()
            {
                warn!(node = ?node_handle, fd = fd, "writer task has exited");
                return (-1, EPIPE);
            }
            resp_rx.blocking_recv().unwrap_or((-1, EPIPE))
        } else {
            warn!(node = ?node_handle, fd = fd, "writer not materialized");
            (-1, EBADF)
        }
    }

    /// Close a specific fd for an actor. Drops the channel task and flushes writers.
    pub fn close(&self, node_handle: Handle, fd: isize) -> (isize, i32) {
        let state = self.channel_table.lock().remove(node_handle, fd);
        match state {
            Some(FdState::AllowedReader) => {
                debug!(node = ?node_handle, fd = fd, "closed allowed reader (never materialized)");
                (0, 0)
            }
            Some(FdState::MaterializedReader { .. }) => {
                debug!(node = ?node_handle, fd = fd, "closed reader channel");
                (0, 0)
            }
            Some(FdState::AllowedWriter) => {
                debug!(node = ?node_handle, fd = fd, "closed allowed writer (never materialized)");
                (0, 0)
            }
            Some(FdState::MaterializedWriter { .. }) => {
                debug!(node = ?node_handle, fd = fd, "closing writer channel");
                if let Some(writer) = self
                    .env
                    .pipe_pool
                    .get_already_realized_writer((node_handle, fd))
                {
                    writer.close();
                }
                let (tx, rx) = oneshot::channel::<(isize, i32)>();
                let env = Arc::clone(&self.env);
                let notify = Arc::clone(&self.notify);
                tokio::spawn(async move {
                    let result: (isize, i32) = if let Some(writer) =
                        env.pipe_pool.get_already_realized_writer((node_handle, fd))
                    {
                        match env.kv.flush_buffer(&writer.buffer()).await {
                            Ok(()) => (0, 0),
                            Err(e) => {
                                warn!(error = ?e, "failed to flush buffer");
                                (-1, EIO)
                            }
                        }
                    } else {
                        warn!(node = ?node_handle, fd = fd, "writer not found for flush");
                        (-1, EIO)
                    };
                    notify.notify_one();
                    if tx.send(result).is_err() {
                        warn!(node = ?node_handle, fd = fd, "close: reply receiver dropped");
                    }
                });
                rx.blocking_recv().unwrap_or((-1, EIO))
            }
            None => {
                warn!(node = ?node_handle, fd = fd, "close: fd not found");
                (-1, EBADF)
            }
        }
    }

    pub fn cleanup_actor_io(&self, node_handle: Handle, exit_code: i32) {
        debug!(node = ?node_handle, exit_code, "cleanup_actor_io: closing writers");
        self.env
            .pipe_pool
            .close_actor_writers(node_handle, exit_code);

        debug!(node = ?node_handle, "cleanup_actor_io: dropping all entries");
        self.channel_table.lock().drop_actor_entries(node_handle);
    }

    /// Wait for all attachment tasks to complete. Call after all actors have shut down.
    pub async fn shutdown(&self) {
        self.attachment_manager.waiting_shutdown().await;
    }
}
