//! Async half of the actor syscall bridge.
//!
//! `IoBridge` is a shared object that actors call directly to perform I/O.
//! It is one half of the actor syscall layer; the other half is
//! `blocking_actor_runtime`, which runs on each actor's blocking thread.
//!
//! Supporting types live in sibling modules:
//! - [`super::sendable_buffer`] — zero-copy buffer pointers for read/write operations
//! - [`super::lifecycle_event`] — two-phase shutdown handshake with the executor
//!
//! # Calling model
//!
//! Both reads and writes go through a long-lived per-channel Tokio task spawned
//! lazily on first use. The actor thread sends a command over an unbounded channel
//! and blocks on a oneshot reply. Tasks live until the channel entry is removed
//! from `ChannelTable` on close or actor shutdown.

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
use crate::pipe::{flush_and_close_writer, MergeReader};


/// A read request forwarded from `IoBridge` to a reader task.
pub(crate) struct ReadRequest {
    buffer: SendableMutPtr,
    response: oneshot::Sender<(isize, i32)>,
}

/// Command sent to a reader task.
pub(crate) enum ReaderCommand {
    Read(ReadRequest),
    Close { response: oneshot::Sender<(isize, i32)> },
}

/// A write request forwarded from `IoBridge` to a writer task.
pub(crate) struct WriteRequest {
    data: SendableConstPtr,
    response: oneshot::Sender<(isize, i32)>,
}

/// Command sent to a writer task.
pub(crate) enum WriterCommand {
    Write(WriteRequest),
    Close { response: oneshot::Sender<(isize, i32)> },
}

/// Long-lived task that owns a `MergeReader` and services read requests for one channel.
///
/// Spawned by `IoBridge::materialize_stdin`. Exits when its `request_rx` closes —
/// i.e., when the channel entry is removed from `ChannelTable` on close or shutdown.
async fn run_reader_task(
    node_handle: Handle,
    mut reader: MergeReader,
    mut request_rx: mpsc::UnboundedReceiver<ReaderCommand>,
) {
    while let Some(cmd) = request_rx.recv().await {
        match cmd {
            ReaderCommand::Read(ReadRequest { buffer, response }) => {
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
            ReaderCommand::Close { response } => {
                debug!(actor = ?node_handle, "reader task: received close command");
                let result = reader.close();
                if response.send(result).is_err() {
                    warn!(actor = ?node_handle, "reader task: close reply receiver dropped");
                }
                // No break: loop exits naturally when sender is dropped and recv() returns None.
                // This keeps the bridge mechanical—it forwards commands without special control flow.
            }
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
    mut request_rx: mpsc::UnboundedReceiver<WriterCommand>,
) {
    // Create writer at task startup
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
            // Send error to the first command, then exit
            if let Some(cmd) = request_rx.recv().await {
                let response = match cmd {
                    WriterCommand::Write(WriteRequest { response, .. }) => response,
                    WriterCommand::Close { response } => response,
                };
                if response.send((-1, EIO)).is_err() {
                    warn!(node = ?node_handle, fd = fd, "writer task: reply receiver dropped");
                }
            }
            return;
        }
    };

    // Process commands
    while let Some(cmd) = request_rx.recv().await {
        match cmd {
            WriterCommand::Write(WriteRequest { data, response }) => {
                // SAFETY: Buffer remains valid because awrite() blocks until response is sent
                let data_slice = unsafe { &*data.into_raw() };
                let n = writer.write(data_slice);
                let result = if n < 0 {
                    (-1, writer.get_error())
                } else {
                    (n, 0)
                };
                // Wake the spawn loop: the first write realizes the writer in the
                // pool, potentially unblocking downstream actors whose readiness
                // check (is_ready_to_spawn) waits for this dep's pipe to appear.
                notify.notify_one();
                if response.send(result).is_err() {
                    warn!(node = ?node_handle, fd = fd, "writer task: reply receiver dropped, actor may have exited");
                }
            }
            WriterCommand::Close { response } => {
                debug!(node = ?node_handle, fd = fd, "writer task: received close command");
                let result = flush_and_close_writer(&*env.kv, &writer, "writer task").await;
                // Wake the spawn loop: a closed writer is a state change that
                // may satisfy spawn readiness for downstream actors.
                notify.notify_one();
                if response.send(result).is_err() {
                    warn!(node = ?node_handle, fd = fd, "writer task: close reply receiver dropped");
                }
                // No break: loop exits naturally when sender is dropped and recv() returns None.
                // This keeps the bridge mechanical—it forwards commands without special control flow.
            }
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
        request_tx: mpsc::UnboundedSender<ReaderCommand>,
    },
    /// Writer has been materialized with a writer task
    MaterializedWriter {
        request_tx: mpsc::UnboundedSender<WriterCommand>,
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
    ) -> Option<mpsc::UnboundedSender<ReaderCommand>> {
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
        let (request_tx, request_rx) = mpsc::unbounded_channel::<ReaderCommand>();
        tokio::spawn(run_reader_task(node_handle, reader, request_rx));

        let tx = request_tx.clone();
        table_guard
            .entries
            .push((node_handle, fd, FdState::MaterializedReader { request_tx }));
        Some(tx)
    }

    /// Materialize a writer. Caller must have verified state is AllowedWriter.
    /// Returns the request sender.
    fn materialize_writer(
        &self,
        table_guard: &mut ChannelTable,
        node_handle: Handle,
        fd: isize,
    ) -> mpsc::UnboundedSender<WriterCommand> {
        debug!(actor = ?node_handle, fd = fd, "materializing writer");
        let (request_tx, request_rx) = mpsc::unbounded_channel::<WriterCommand>();

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

        let tx = request_tx.clone();
        if let Some(state) = table_guard.get_state_mut(node_handle, fd) {
            *state = FdState::MaterializedWriter { request_tx };
        }

        tx
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
            .send(ReaderCommand::Read(ReadRequest {
                buffer,
                response: resp_tx,
            }))
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
        let tx = {
            let mut table_guard = self.channel_table.lock();
            match table_guard.get_state(node_handle, fd) {
                Some(FdState::MaterializedWriter { request_tx }) => request_tx.clone(),
                Some(FdState::AllowedWriter) => {
                    self.materialize_writer(&mut table_guard, node_handle, fd)
                }
                Some(FdState::AllowedReader | FdState::MaterializedReader { .. }) => {
                    warn!(node = ?node_handle, fd = fd, "write: cannot write to reader fd");
                    return (-1, EBADF);
                }
                None => {
                    warn!(node = ?node_handle, fd = fd, "write: fd not registered");
                    return (-1, EBADF);
                }
            }
        };

        let (resp_tx, resp_rx) = oneshot::channel();
        if tx
            .send(WriterCommand::Write(WriteRequest {
                data,
                response: resp_tx,
            }))
            .is_err()
        {
            warn!(node = ?node_handle, fd = fd, "writer task has exited");
            return (-1, EPIPE);
        }
        resp_rx.blocking_recv().unwrap_or((-1, EPIPE))
    }

    /// Close a specific fd for an actor. Drops the channel task and flushes writers.
    pub fn close(&self, node_handle: Handle, fd: isize) -> (isize, i32) {
        let state = self.channel_table.lock().remove(node_handle, fd);
        match state {
            Some(FdState::AllowedReader) => {
                debug!(node = ?node_handle, fd = fd, "closed allowed reader (never materialized)");
                (0, 0)
            }
            Some(FdState::MaterializedReader { request_tx }) => {
                debug!(node = ?node_handle, fd = fd, "closing reader channel");
                let (resp_tx, resp_rx) = oneshot::channel();
                if request_tx.send(ReaderCommand::Close { response: resp_tx }).is_err() {
                    warn!(node = ?node_handle, fd = fd, "reader task has exited");
                    return (-1, EIO);
                }
                resp_rx.blocking_recv().unwrap_or((-1, EIO))
            }
            Some(FdState::AllowedWriter) => {
                debug!(node = ?node_handle, fd = fd, "closed allowed writer (never materialized)");
                (0, 0)
            }
            Some(FdState::MaterializedWriter { request_tx }) => {
                debug!(node = ?node_handle, fd = fd, "closing writer channel");
                let (resp_tx, resp_rx) = oneshot::channel();
                if request_tx.send(WriterCommand::Close { response: resp_tx }).is_err() {
                    warn!(node = ?node_handle, fd = fd, "writer task has exited");
                    return (-1, EIO);
                }
                resp_rx.blocking_recv().unwrap_or((-1, EIO))
            }
            None => {
                warn!(node = ?node_handle, fd = fd, "close: fd not found");
                (-1, EBADF)
            }
        }
    }

    pub async fn cleanup_actor_io(
        &self,
        node_handle: Handle,
        exit_code: i32,
    ) -> Result<(), String> {
        debug!(node = ?node_handle, exit_code, "cleanup_actor_io: flushing and closing writers");

        // Close writers at pipe layer (with flush)
        let result = self
            .env
            .pipe_pool
            .flush_close_actor_writers(node_handle, exit_code)
            .await;

        debug!(node = ?node_handle, "cleanup_actor_io: dropping all entries");
        self.channel_table.lock().drop_actor_entries(node_handle);

        result
    }

    /// Wait for all attachment tasks to complete. Call after all actors have shut down.
    pub async fn shutdown(&self) {
        self.attachment_manager.waiting_shutdown().await;
    }
}
