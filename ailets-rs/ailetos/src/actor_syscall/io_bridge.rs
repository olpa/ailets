//! Async half of the actor syscall bridge.
//!
//! `IoBridge` is an async event loop that receives [`IoRequest`]s from actor threads,
//! dispatches them to pipes and storage, and sends replies. It is one half of the
//! actor syscall layer; the other half is `stub_actor_runtime`, which runs on each
//! actor's blocking thread and sends the requests.
//!
//! Supporting types live in sibling modules:
//! - [`super::sendable_buffer`] — zero-copy buffer pointer for read operations
//! - [`super::lifecycle_event`] — two-phase shutdown handshake with the executor
//!
//! # Concurrency model
//!
//! Each I/O operation is delegated to a dedicated Tokio task that owns its data and
//! sends the reply directly to the actor via a oneshot channel. `IoBridge` itself
//! stays responsive — it routes requests without waiting for any of them to complete.
//!
//! - **Reads** — `materialize_stdin` spawns one long-lived reader task per stdin
//!   channel. The task owns the [`MergeReader`] and services read requests one at a
//!   time via an mpsc channel. It exits when its request channel closes (on actor
//!   shutdown or channel close).
//! - **Writes / flushes** — `tokio::spawn` for each write or close operation; the
//!   task owns a cloned `Arc<PipePool>` and `Arc<K>` and sends the reply when done.
//!
//! ## Actor shutdown (two-phase)
//!
//! Shutdown is split into two phases to preserve ordering between I/O cleanup and
//! DAG state updates, which are owned by the executor:
//!
//! 1. **Terminating** — `IoBridge` sends `ActorLifecycleEvent::Terminating` and awaits
//!    the reply. The executor sets `NodeState::Terminating` and replies with the prior
//!    state. If the actor was already terminating (duplicate shutdown), `IoBridge` returns
//!    early and skips I/O cleanup.
//! 2. **I/O cleanup** — writers are closed, reader channels are dropped (which causes
//!    reader tasks to exit naturally when they finish their current operation).
//! 3. **Terminated** — `IoBridge` sends `ActorLifecycleEvent::Terminated` and awaits the
//!    reply. The executor sets `NodeState::Terminated`, records the exit code, and fires
//!    `notify` so the spawn loop can react.
//!
//! The reply channels enforce ordering: each phase waits for the executor to finish
//! its DAG update before the next phase begins.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{mpsc, oneshot, Notify};
use tracing::{debug, trace, warn};

use crate::attachments::{AttachmentConfig, AttachmentManager};
use crate::dag::{NodeState, OwnedDependencyIterator};
use crate::idgen::{Handle, IdGen};
use crate::pipe::{MergeReader, PipePool};
use crate::KVBuffers;
use super::lifecycle_event::ActorLifecycleEvent;
use super::sendable_buffer::SendableBuffer;

/// Global unique identifier for a pipe endpoint (reader or writer)
/// Used by `IoBridge` to identify channels across all actors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChannelHandle(pub isize);

/// A channel endpoint - either a reader or writer
pub(crate) enum Channel {
    /// Reader channel - routes requests to a dedicated reader task
    Reader {
        node_handle: Handle,
        request_tx: mpsc::UnboundedSender<ReadRequest>,
    },
    /// Writer channel - holds the actor's node handle and std handle for pipe lookup
    Writer {
        node_handle: Handle,
        std_handle: actor_runtime::StdHandle,
    },
}

/// I/O requests sent from `ActorRuntime` to `IoBridge`
pub enum IoRequest {
    /// Open a stream for reading (returns global `ChannelHandle`)
    OpenRead {
        node_handle: Handle,
        response: oneshot::Sender<ChannelHandle>,
    },
    /// Open a stream for writing (returns global `ChannelHandle`)
    OpenWrite {
        node_handle: Handle,
        response: oneshot::Sender<ChannelHandle>,
    },
    /// Read from a channel (async operation)
    /// SAFETY: The buffer pointer must remain valid until the response is sent.
    /// This is guaranteed because `aread()` blocks waiting for the response.
    /// Response is `(bytes_read, errno)`: errno is non-zero only when `bytes_read` < 0.
    Read {
        handle: ChannelHandle,
        buffer: SendableBuffer,
        response: oneshot::Sender<(isize, i32)>,
    },
    /// Write to an actor's output pipe (async operation)
    /// Uses `node_handle` + `std_handle` to write directly to `PipePool`
    Write {
        node_handle: Handle,
        std_handle: actor_runtime::StdHandle,
        data: Vec<u8>,
        response: oneshot::Sender<isize>,
    },
    /// Close a channel
    Close {
        handle: ChannelHandle,
        response: oneshot::Sender<isize>,
    },
    /// Actor shutdown - close all writers (realized and latent) for this actor.
    /// `exit_code`: 0 = clean termination, non-zero = POSIX errno.
    ActorShutdown { node_handle: Handle, exit_code: i32 },
    /// Materialize stdin reader for an actor (creates `MergeReader`, returns `ChannelHandle`)
    /// Called on first read from stdin. The iterator is pre-built by the executor at spawn time.
    MaterializeStdin {
        node_handle: Handle,
        dep_iterator: OwnedDependencyIterator,
        response: oneshot::Sender<ChannelHandle>,
    },
    /// Close a writer for an actor
    /// Uses `node_handle` + `std_handle` to find and close the writer in `PipePool`
    CloseWriter {
        node_handle: Handle,
        std_handle: actor_runtime::StdHandle,
        response: oneshot::Sender<isize>,
    },
}

/// A read request forwarded from `IoBridge` to a reader task.
pub(crate) struct ReadRequest {
    buffer: SendableBuffer,
    response: oneshot::Sender<(isize, i32)>,
}

/// Long-lived task that owns a `MergeReader` and services read requests for one channel.
///
/// Spawned by `materialize_stdin` when the actor first reads from stdin. The task exits
/// when its `request_rx` closes — i.e., when the channel entry is removed from
/// `ChannelTable` (on `Close` or actor shutdown).
async fn run_reader_task<K: KVBuffers>(
    mut reader: MergeReader<K>,
    mut request_rx: mpsc::UnboundedReceiver<ReadRequest>,
) {
    while let Some(ReadRequest { buffer, response }) = request_rx.recv().await {
        // SAFETY: Buffer remains valid because aread() blocks until response is sent
        let buf = unsafe { &mut *buffer.into_raw() };
        let bytes_read = reader.read(buf).await;
        let errno = if bytes_read < 0 { reader.get_error() } else { 0 };
        let _ = response.send((bytes_read, errno));
    }
}

/// Global table of open channel endpoints, analogous to the kernel's open-file-descriptions table.
///
/// Each entry maps a [`ChannelHandle`] (an opaque integer given to the actor) to its
/// underlying [`Channel`] — either a reader with a request channel to its task, or a
/// writer identified by `(node_handle, std_handle)`.
///
/// The per-actor fd → [`ChannelHandle`] mapping lives in [`super::fd_table::FdTable`];
/// this table holds the system-side objects those handles resolve to.
struct ChannelTable {
    map: HashMap<ChannelHandle, Channel>,
    next_id: isize,
}

impl ChannelTable {
    fn new() -> Self {
        Self { map: HashMap::new(), next_id: 0 }
    }

    /// Allocate a new handle without inserting a channel (used for dummy/unimplemented paths).
    fn alloc_handle(&mut self) -> ChannelHandle {
        let handle = ChannelHandle(self.next_id);
        self.next_id += 1;
        handle
    }

    /// Insert a new reader channel backed by a spawned reader task.
    fn insert_reader(&mut self, node_handle: Handle, request_tx: mpsc::UnboundedSender<ReadRequest>) -> ChannelHandle {
        let handle = self.alloc_handle();
        self.map.insert(handle, Channel::Reader { node_handle, request_tx });
        handle
    }

    /// Insert a new writer channel and return its handle.
    fn insert_writer(&mut self, node_handle: Handle, std_handle: actor_runtime::StdHandle) -> ChannelHandle {
        let handle = self.alloc_handle();
        self.map.insert(handle, Channel::Writer { node_handle, std_handle });
        handle
    }

    /// Return the sender to the reader task for this handle, or `None` if not found / not a reader.
    fn get_reader_tx(&self, handle: ChannelHandle) -> Option<&mpsc::UnboundedSender<ReadRequest>> {
        if let Some(Channel::Reader { request_tx, .. }) = self.map.get(&handle) {
            Some(request_tx)
        } else {
            None
        }
    }

    /// Remove and return a channel by handle (used by `Close`).
    fn remove(&mut self, handle: ChannelHandle) -> Option<Channel> {
        self.map.remove(&handle)
    }

    /// Drop all reader channels belonging to `node_handle` (called on actor shutdown).
    /// Dropping the sender causes the reader task to exit after its current operation.
    fn drop_actor_readers(&mut self, node_handle: Handle) {
        self.map.retain(|_, ch| {
            !matches!(ch, Channel::Reader { node_handle: h, .. } if *h == node_handle)
        });
    }
}

/// `IoBridge` manages all async I/O operations
/// Actors communicate with it via channels
pub struct IoBridge<K: KVBuffers> {
    /// Pool of output pipes (one per actor)
    pipe_pool: Arc<PipePool<K>>,
    /// Key-value store for pipe buffers
    kv: Arc<K>,
    /// Open channel endpoints: ChannelHandle → reader or writer
    channel_table: ChannelTable,
    /// Channel to send I/O requests to this runtime (None after `run()` starts)
    system_tx: Option<mpsc::UnboundedSender<IoRequest>>,
    /// Receives I/O requests from actors
    request_rx: mpsc::UnboundedReceiver<IoRequest>,
    /// ID generator for handles
    id_gen: Arc<IdGen>,
    /// Manages dynamic attachment of actor streams to host stdout/stderr
    attachment_manager: Arc<AttachmentManager>,
    /// Notifies the spawn loop when something changes that may affect node readiness.
    /// Use cases:
    /// - a pipe is realized
    /// - a writer is closed
    notify: Arc<Notify>,
    /// Signals the executor when an actor's I/O is fully torn down (pipes closed, channels dropped).
    /// Out-of-domain for the IO bridge: the executor owns actor lifecycle state, but the signal
    /// originates here because I/O cleanup must complete before the executor can mark the actor
    /// Terminated and update DAG state.
    actor_done_tx: mpsc::UnboundedSender<ActorLifecycleEvent>,
}

impl<K: KVBuffers + 'static> IoBridge<K> {
    pub fn new(
        kv: Arc<K>,
        id_gen: Arc<IdGen>,
        attachment_config: AttachmentConfig,
        pipe_pool: Arc<PipePool<K>>,
        notify: Arc<Notify>,
        actor_done_tx: mpsc::UnboundedSender<ActorLifecycleEvent>,
    ) -> Self {
        let (system_tx, request_rx) = mpsc::unbounded_channel();
        let attachment_manager = Arc::new(AttachmentManager::new(attachment_config));

        Self {
            pipe_pool,
            kv,
            channel_table: ChannelTable::new(),
            system_tx: Some(system_tx),
            request_rx,
            id_gen,
            attachment_manager,
            notify,
            actor_done_tx,
        }
    }

    /// Get the sender for creating actor runtimes.
    /// Returns `None` if called after `run()` has started (which consumes the sender).
    #[must_use]
    pub fn get_system_tx(&self) -> Option<mpsc::UnboundedSender<IoRequest>> {
        self.system_tx.clone()
    }

    /// Materialize stdin reader for an actor on first read.
    ///
    /// Creates a `MergeReader`, spawns a long-lived reader task that owns it, and
    /// stores the request sender in `ChannelTable`. Called lazily when the actor
    /// first reads from stdin.
    fn materialize_stdin(&mut self, node_handle: Handle, dep_iterator: OwnedDependencyIterator) -> ChannelHandle {
        debug!(actor = ?node_handle, "materializing stdin reader");
        let reader = MergeReader::new(
            dep_iterator,
            Arc::clone(&self.pipe_pool),
            Arc::clone(&self.kv),
            Arc::clone(&self.id_gen),
        );
        let (request_tx, request_rx) = mpsc::unbounded_channel::<ReadRequest>();
        tokio::spawn(run_reader_task(reader, request_rx));
        self.channel_table.insert_reader(node_handle, request_tx)
    }

    /// Handler for `ActorShutdown` requests
    async fn handle_actor_shutdown(&mut self, node_handle: Handle, exit_code: i32) {
        let (tx, rx) = oneshot::channel::<NodeState>();
        if self.actor_done_tx.send(ActorLifecycleEvent::Terminating {
            node_handle,
            reply: tx,
        }).is_err() {
            warn!(node = ?node_handle, "actor shutdown - executor gone, dropping");
            return;
        }

        let prior_state = rx.await.unwrap_or(NodeState::Terminating);
        if matches!(prior_state, NodeState::Terminating | NodeState::Terminated) {
            debug!(node = ?node_handle, "actor shutdown - already terminating/terminated, ignoring");
            return;
        }

        debug!(node = ?node_handle, exit_code, "actor shutdown - closing all writers");
        self.pipe_pool.close_actor_writers(node_handle, exit_code);

        debug!(node = ?node_handle, "actor shutdown - dropping reader channels");
        self.channel_table.drop_actor_readers(node_handle);

        let (tx2, rx2) = oneshot::channel::<NodeState>();
        if self.actor_done_tx.send(ActorLifecycleEvent::Terminated {
            node_handle,
            exit_code,
            reply: tx2,
        }).is_err() {
            warn!(node = ?node_handle, "actor shutdown - executor gone before Terminated");
            return;
        }
        if rx2.await.is_err() {
            warn!(node = ?node_handle, "actor shutdown - Terminated reply dropped");
        }
    }

    /// Handler for `OpenRead` requests
    /// Currently returns a dummy handle - dependency wiring not yet implemented
    fn handle_open_read(&mut self, node_handle: Handle, response: oneshot::Sender<ChannelHandle>) {
        debug!(node = ?node_handle, "processing OpenRead");
        let channel_handle = self.channel_table.alloc_handle();
        warn!(node = ?node_handle, channel = ?channel_handle, "OpenRead: no input configured, returning dummy");
        let _ = response.send(channel_handle);
    }

    /// Handler for `OpenWrite` — registers the writer channel and pre-creates the pipe in the background.
    fn handle_open_write(&mut self, node_handle: Handle, response: oneshot::Sender<ChannelHandle>) {
        debug!(node = ?node_handle, "processing OpenWrite");
        let std_handle = actor_runtime::StdHandle::Stdout;
        let channel_handle = self.channel_table.insert_writer(node_handle, std_handle);
        let _ = response.send(channel_handle);

        // Pre-create the writer pipe in the background; writes are also idempotent so
        // this is an optimisation only.
        let pipe_pool = Arc::clone(&self.pipe_pool);
        let id_gen = Arc::clone(&self.id_gen);
        let notify = Arc::clone(&self.notify);
        tokio::spawn(async move {
            match pipe_pool.touch_writer(node_handle, std_handle, &id_gen).await {
                Ok(_) => notify.notify_one(),
                Err(e) => warn!(node = ?node_handle, error = %e, "failed to pre-create writer"),
            }
        });
    }

    /// Handler for Read requests — routes the request to the channel's reader task.
    fn handle_read(&self, handle: ChannelHandle, buffer: SendableBuffer, response: oneshot::Sender<(isize, i32)>) {
        if let Some(tx) = self.channel_table.get_reader_tx(handle) {
            if tx.send(ReadRequest { buffer, response }).is_err() {
                warn!(channel = ?handle, "reader task has exited");
            }
        } else {
            warn!(channel = ?handle, "reader not available");
            let _ = response.send((0, 0));
        }
    }

    /// Handler for Write requests — spawns a task that writes to the actor's pipe.
    ///
    /// Writes directly to `PipePool` using `node_handle` + `std_handle` from the request.
    /// No channel table lookup needed — `PipePool` handles lazy pipe creation.
    /// When a writer is newly created, triggers attachment via `AttachmentManager`.
    fn handle_write(&self, node_handle: Handle, std_handle: actor_runtime::StdHandle, data: Vec<u8>, response: oneshot::Sender<isize>) {
        let pipe_pool = Arc::clone(&self.pipe_pool);
        let id_gen = Arc::clone(&self.id_gen);
        let attachment_manager = Arc::clone(&self.attachment_manager);
        let notify = Arc::clone(&self.notify);
        tokio::spawn(async move {
            let result = match pipe_pool.touch_writer(node_handle, std_handle, &id_gen).await {
                Ok((writer, is_new)) => {
                    if is_new {
                        attachment_manager.on_writer_realized(
                            node_handle, std_handle,
                            Arc::clone(&pipe_pool), Arc::clone(&id_gen),
                        );
                    }
                    let n = writer.write(&data);
                    if n < 0 {
                        let errno = writer.get_error();
                        if errno != 0 { -(errno as isize) } else { -1 }
                    } else { n }
                }
                Err(e) => {
                    warn!(node = ?node_handle, std = ?std_handle, error = %e, "failed to get writer");
                    -1
                }
            };
            notify.notify_one();
            let _ = response.send(result);
        });
    }

    /// Handler for Close requests.
    ///
    /// For a reader channel: removing the entry drops the sender, which causes the
    /// reader task to exit after its current operation.
    /// For a writer channel: spawns a task to close and flush the pipe.
    fn handle_close(&mut self, handle: ChannelHandle, response: oneshot::Sender<isize>) {
        match self.channel_table.remove(handle) {
            Some(Channel::Reader { .. }) => {
                let _ = response.send(0);
            }
            Some(Channel::Writer { node_handle, std_handle }) => {
                debug!(channel = ?handle, node = ?node_handle, std = ?std_handle, "closing writer channel");
                if let Some(writer) = self.pipe_pool.get_already_realized_writer((node_handle, std_handle)) {
                    writer.close();
                    debug!(channel = ?handle, node = ?node_handle, std = ?std_handle, "closed writer pipe");
                }
                let pipe_pool = Arc::clone(&self.pipe_pool);
                let kv = Arc::clone(&self.kv);
                let notify = Arc::clone(&self.notify);
                tokio::spawn(async move {
                    let result = if let Some(writer) = pipe_pool.get_already_realized_writer((node_handle, std_handle)) {
                        match kv.flush_buffer(&writer.buffer()).await {
                            Ok(()) => 0,
                            Err(e) => { warn!(error = ?e, "failed to flush buffer"); -1 }
                        }
                    } else {
                        warn!(node = ?node_handle, std = ?std_handle, "writer not found for flush");
                        -1
                    };
                    notify.notify_one();
                    let _ = response.send(result);
                });
            }
            None => {
                warn!(channel = ?handle, "channel not found");
                let _ = response.send(-1);
            }
        }
    }

    /// Handler for `CloseWriter` — closes a writer directly via `PipePool` and flushes.
    fn handle_close_writer(&self, node_handle: Handle, std_handle: actor_runtime::StdHandle, response: oneshot::Sender<isize>) {
        if let Some(writer) = self.pipe_pool.get_already_realized_writer((node_handle, std_handle)) {
            writer.close();
            debug!(node = ?node_handle, std = ?std_handle, "closed writer pipe");
        }
        let pipe_pool = Arc::clone(&self.pipe_pool);
        let kv = Arc::clone(&self.kv);
        let notify = Arc::clone(&self.notify);
        tokio::spawn(async move {
            let result = if let Some(writer) = pipe_pool.get_already_realized_writer((node_handle, std_handle)) {
                match kv.flush_buffer(&writer.buffer()).await {
                    Ok(()) => 0,
                    Err(e) => { warn!(error = ?e, "failed to flush buffer"); -1 }
                }
            } else {
                0 // writer was never created — succeed silently
            };
            notify.notify_one();
            let _ = response.send(result);
        });
    }

    /// Main event loop — receives requests and routes them to spawned tasks or handlers.
    pub async fn run(mut self) {
        trace!("IoBridge::run: entering request_rx loop");
        drop(self.system_tx.take());

        while let Some(request) = self.request_rx.recv().await {
            match request {
                IoRequest::OpenRead { node_handle, response } => {
                    self.handle_open_read(node_handle, response);
                }
                IoRequest::OpenWrite { node_handle, response } => {
                    self.handle_open_write(node_handle, response);
                }
                IoRequest::Read { handle, buffer, response } => {
                    self.handle_read(handle, buffer, response);
                }
                IoRequest::Write { node_handle, std_handle, data, response } => {
                    self.handle_write(node_handle, std_handle, data, response);
                }
                IoRequest::Close { handle, response } => {
                    self.handle_close(handle, response);
                }
                IoRequest::ActorShutdown { node_handle, exit_code } => {
                    self.handle_actor_shutdown(node_handle, exit_code).await;
                }
                IoRequest::MaterializeStdin { node_handle, dep_iterator, response } => {
                    let channel_handle = self.materialize_stdin(node_handle, dep_iterator);
                    let _ = response.send(channel_handle);
                }
                IoRequest::CloseWriter { node_handle, std_handle, response } => {
                    self.handle_close_writer(node_handle, std_handle, response);
                }
            }
        }

        trace!("IoBridge::run: exited request_rx loop");
        self.attachment_manager.waiting_shutdown().await;
        trace!("IoBridge: all attachments completed");
        trace!("IoBridge: destroying");
    }
}
