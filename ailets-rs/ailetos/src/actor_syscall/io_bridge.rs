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
use crate::dag::OwnedDependencyIterator;
use crate::environment::Environment;
use crate::errno::{EBADF, EIO, EPIPE};
use crate::idgen::Handle;
use crate::pipe::{flush_and_close_writer, MergeReader};

/// A read request forwarded from `IoBridge` to a reader task.
pub(crate) struct ReadRequest {
    buffer: SendableMutPtr,
    response: oneshot::Sender<Result<usize, i32>>,
}

/// Command sent to a reader task.
pub(crate) enum ReaderCommand {
    Read(ReadRequest),
    Close {
        response: oneshot::Sender<Result<(), i32>>,
    },
}

/// A write request forwarded from `IoBridge` to a writer task.
pub(crate) struct WriteRequest {
    data: SendableConstPtr,
    response: oneshot::Sender<Result<usize, i32>>,
}

/// Command sent to a writer task.
pub(crate) enum WriterCommand {
    Write(WriteRequest),
    Close {
        response: oneshot::Sender<Result<(), i32>>,
    },
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
                let result = reader.read(buf).await;
                if response.send(result).is_err() {
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
    async_runtime: tokio::runtime::Handle,
    env: Arc<Environment>,
    executor_wakeup: Arc<Notify>,
    mut request_rx: mpsc::UnboundedReceiver<WriterCommand>,
) {
    // Create writer at task startup
    let writer = match env
        .pipe_pool
        .touch_writer(node_handle, fd, &env.idgen)
        .await
    {
        Ok((writer, is_new)) => {
            if is_new {
                if let Ok(std_handle) = StdHandle::try_from(fd) {
                    match std_handle {
                        StdHandle::Log | StdHandle::Metrics | StdHandle::Trace => {
                            env.pipe_pool.spawn_reader_to(
                                &async_runtime,
                                &env.idgen,
                                (node_handle, fd),
                                std::io::stderr(),
                            );
                        }
                        StdHandle::Stdin | StdHandle::Stdout | StdHandle::Env | StdHandle::_Count => {}
                    }
                }
            }
            writer
        }
        Err(e) => {
            warn!(node = ?node_handle, fd = fd, error = %e, "writer task: failed to create writer");
            // Return error on each command until channel closes
            while let Some(cmd) = request_rx.recv().await {
                let send_failed = match cmd {
                    WriterCommand::Write(WriteRequest { response, .. }) => {
                        response.send(Err(EIO)).is_err()
                    }
                    WriterCommand::Close { response } => response.send(Err(EIO)).is_err(),
                };
                if send_failed {
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
                let result = writer.write(data_slice);
                if response.send(result).is_err() {
                    warn!(node = ?node_handle, fd = fd, "writer task: reply receiver dropped, actor may have exited");
                }
                // Wake the executor: the first write realizes the writer in the
                // pool, potentially unblocking downstream actors whose readiness
                // check (is_ready_to_spawn) waits for this dep's pipe to appear.
                executor_wakeup.notify_one();
            }
            WriterCommand::Close { response } => {
                debug!(node = ?node_handle, fd = fd, "writer task: received close command");
                let result = flush_and_close_writer(&*env.kv, &writer, "writer task").await;
                if response.send(result).is_err() {
                    warn!(node = ?node_handle, fd = fd, "writer task: close reply receiver dropped");
                }
                // Wake the executor: a closed writer is a state change that
                // may satisfy spawn readiness for downstream actors.
                executor_wakeup.notify_one();
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

/// Channel table entry type: (`node_handle`, fd, state)
type ChannelEntry = (Handle, isize, FdState);

/// Directly-callable I/O bridge between actor threads and the async runtime.
///
/// Held as `Arc<IoBridge>` by each actor's `BlockingActorRuntime` and `ShutdownHandle`.
/// All methods are safe to call from a blocking thread.
pub struct IoBridge {
    async_runtime: tokio::runtime::Handle,
    env: Arc<Environment>,
    /// Open channel endpoints: (`node_handle`, fd, state). Linear search, efficient for small N.
    channel_table: Mutex<Vec<ChannelEntry>>,
    /// Notifies the executor when I/O state changes that may affect node readiness.
    /// Use cases:
    /// - a pipe is realized
    /// - a writer is closed
    executor_wakeup: Arc<Notify>,
}

impl IoBridge {
    #[must_use]
    pub fn new(
        async_runtime: tokio::runtime::Handle,
        env: Arc<Environment>,
        executor_wakeup: Arc<Notify>,
    ) -> Self {
        Self {
            async_runtime,
            env,
            channel_table: Mutex::new(Vec::new()),
            executor_wakeup,
        }
    }

    /// Register a reader fd. Task is spawned lazily on first use. No duplicate check.
    pub(crate) fn register_std_fd_reader(&self, node_handle: Handle, fd: isize) {
        self.channel_table
            .lock()
            .push((node_handle, fd, FdState::AllowedReader));
    }

    /// Register a writer fd. Task is spawned lazily on first use. No duplicate check.
    pub(crate) fn register_std_fd_writer(&self, node_handle: Handle, fd: isize) {
        self.channel_table
            .lock()
            .push((node_handle, fd, FdState::AllowedWriter));
    }

    /// Create a reader task for stdin.
    fn spawn_reader(&self, node_handle: Handle) -> mpsc::UnboundedSender<ReaderCommand> {
        debug!(actor = ?node_handle, "spawning stdin reader");
        let dep_iterator = OwnedDependencyIterator::new(Arc::clone(&self.env.dag), node_handle);
        let reader = MergeReader::new(
            dep_iterator,
            Arc::clone(&self.env.pipe_pool),
            Arc::clone(&self.env.kv),
            Arc::clone(&self.env.idgen),
        );
        let (request_tx, request_rx) = mpsc::unbounded_channel::<ReaderCommand>();
        self.async_runtime.spawn(run_reader_task(node_handle, reader, request_rx));
        request_tx
    }

    /// Create a writer task.
    fn spawn_writer(&self, node_handle: Handle, fd: isize) -> mpsc::UnboundedSender<WriterCommand> {
        debug!(actor = ?node_handle, fd = fd, "spawning writer");
        let (request_tx, request_rx) = mpsc::unbounded_channel::<WriterCommand>();
        let async_runtime = self.async_runtime.clone();
        let env = Arc::clone(&self.env);
        let executor_wakeup = Arc::clone(&self.executor_wakeup);
        self.async_runtime.spawn(run_writer_task(
            node_handle,
            fd,
            async_runtime,
            env,
            executor_wakeup,
            request_rx,
        ));
        request_tx
    }

    /// Route a read request to the channel's reader task and block for the result.
    /// Materializes reader lazily on first call.
    ///
    /// # Errors
    /// Returns `EBADF` if fd not registered or wrong type, `EPIPE` if task exited.
    pub fn read(
        &self,
        node_handle: Handle,
        fd: isize,
        buffer: SendableMutPtr,
    ) -> Result<usize, i32> {
        let tx = {
            let mut table = self.channel_table.lock();
            let Some((_, _, state)) = table
                .iter_mut()
                .find(|(h, f, _)| (*h, *f) == (node_handle, fd))
            else {
                warn!(node = ?node_handle, fd = fd, "read: fd not registered");
                return Err(EBADF);
            };
            match state {
                FdState::MaterializedReader { request_tx } => request_tx.clone(),
                FdState::AllowedReader => {
                    // Only stdin is supported for now
                    if fd != StdHandle::Stdin as isize {
                        warn!(actor = ?node_handle, fd = fd, "reader materialization only supported for stdin");
                        return Err(EBADF);
                    }
                    let request_tx = self.spawn_reader(node_handle);
                    *state = FdState::MaterializedReader {
                        request_tx: request_tx.clone(),
                    };
                    request_tx
                }
                FdState::AllowedWriter | FdState::MaterializedWriter { .. } => {
                    warn!(node = ?node_handle, fd = fd, "read: cannot read from writer fd");
                    return Err(EBADF);
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
            return Err(EPIPE);
        }
        resp_rx.blocking_recv().unwrap_or(Err(EPIPE))
    }

    /// Route a write request to the channel's writer task and block for the result.
    /// Materializes writer lazily on first call.
    ///
    /// # Errors
    /// Returns `EBADF` if fd not registered or wrong type, `EPIPE` if task exited.
    pub fn write(
        &self,
        node_handle: Handle,
        fd: isize,
        data: SendableConstPtr,
    ) -> Result<usize, i32> {
        let tx = {
            let mut table = self.channel_table.lock();
            let Some((_, _, state)) = table
                .iter_mut()
                .find(|(h, f, _)| (*h, *f) == (node_handle, fd))
            else {
                warn!(node = ?node_handle, fd = fd, "write: fd not registered");
                return Err(EBADF);
            };
            match state {
                FdState::MaterializedWriter { request_tx } => request_tx.clone(),
                FdState::AllowedWriter => {
                    let request_tx = self.spawn_writer(node_handle, fd);
                    *state = FdState::MaterializedWriter {
                        request_tx: request_tx.clone(),
                    };
                    request_tx
                }
                FdState::AllowedReader | FdState::MaterializedReader { .. } => {
                    warn!(node = ?node_handle, fd = fd, "write: cannot write to reader fd");
                    return Err(EBADF);
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
            return Err(EPIPE);
        }
        resp_rx.blocking_recv().unwrap_or(Err(EPIPE))
    }

    /// Close a specific fd for an actor. Drops the channel task and flushes writers.
    ///
    /// # Errors
    /// Returns `EBADF` if fd not found, `EIO` if task exited or close failed.
    pub fn close(&self, node_handle: Handle, fd: isize) -> Result<(), i32> {
        let mut table = self.channel_table.lock();
        let state = if let Some(pos) = table
            .iter()
            .position(|(h, f, _)| (*h, *f) == (node_handle, fd))
        {
            let (_, _, state) = table.remove(pos);
            Some(state)
        } else {
            None
        };
        match state {
            Some(FdState::AllowedReader) => {
                debug!(node = ?node_handle, fd = fd, "closed allowed reader (never materialized)");
                Ok(())
            }
            Some(FdState::MaterializedReader { request_tx }) => {
                debug!(node = ?node_handle, fd = fd, "closing reader channel");
                let (resp_tx, resp_rx) = oneshot::channel();
                if request_tx
                    .send(ReaderCommand::Close { response: resp_tx })
                    .is_err()
                {
                    warn!(node = ?node_handle, fd = fd, "reader task has exited");
                    return Err(EIO);
                }
                resp_rx.blocking_recv().unwrap_or(Err(EIO))
            }
            Some(FdState::AllowedWriter) => {
                debug!(node = ?node_handle, fd = fd, "closed allowed writer (never materialized)");
                Ok(())
            }
            Some(FdState::MaterializedWriter { request_tx }) => {
                debug!(node = ?node_handle, fd = fd, "closing writer channel");
                let (resp_tx, resp_rx) = oneshot::channel();
                if request_tx
                    .send(WriterCommand::Close { response: resp_tx })
                    .is_err()
                {
                    warn!(node = ?node_handle, fd = fd, "writer task has exited");
                    return Err(EIO);
                }
                resp_rx.blocking_recv().unwrap_or(Err(EIO))
            }
            None => {
                warn!(node = ?node_handle, fd = fd, "close: fd not found");
                Err(EBADF)
            }
        }
    }

    /// Flush and close all I/O for an actor on shutdown.
    ///
    /// # Errors
    ///
    /// Returns `Err` if flushing writer buffers to storage fails.
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
        self.channel_table
            .lock()
            .retain(|(h, _, _)| *h != node_handle);

        result
    }

    /// Shutdown the IO bridge, flushing and closing any remaining actor I/O.
    /// Call after all actors have completed.
    ///
    /// # Errors
    ///
    /// Returns `Err` if flushing any writer buffers to storage fails.
    pub async fn shutdown(&self) -> Result<(), String> {
        let mut failed_count = 0;
        loop {
            let node_handle = self.channel_table.lock().first().map(|(h, _, _)| *h);
            let Some(node_handle) = node_handle else {
                break;
            };
            if let Err(e) = self.cleanup_actor_io(node_handle, 0).await {
                warn!(node = ?node_handle, error = %e, "shutdown: failed to cleanup actor io");
                failed_count += 1;
            }
        }
        if failed_count > 0 {
            Err(format!("io cleanup failed for {failed_count} actors"))
        } else {
            Ok(())
        }
    }
}
