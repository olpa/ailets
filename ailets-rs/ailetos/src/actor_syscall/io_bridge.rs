//! Async half of the actor syscall bridge.
//!
//! `IoBridge` is a shared object that actors call directly to perform I/O.
//! It is one half of the actor syscall layer; the other half is
//! `stub_actor_runtime`, which runs on each actor's blocking thread.
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
//!
//! # Actor shutdown (two-phase)
//!
//! `actor_shutdown` is called from `ShutdownHandle::drop` on the blocking thread.
//! It executes the two-phase lifecycle protocol synchronously using `blocking_recv`:
//!
//! 1. Send `ActorLifecycleEvent::Terminating`, block for executor reply.
//!    If already terminating/terminated, return early.
//! 2. Close writers, drop reader channels (reader tasks exit naturally).
//! 3. Send `ActorLifecycleEvent::Terminated`, block for executor reply.

use std::sync::Arc;
use std::collections::HashMap;

use parking_lot::Mutex;
use tokio::sync::{mpsc, oneshot, Notify};
use tracing::{debug, warn};

use crate::attachments::{AttachmentConfig, AttachmentManager};
use crate::dag::{NodeState, OwnedDependencyIterator};
use crate::idgen::{Handle, IdGen};
use crate::pipe::{MergeReader, PipePool};
use crate::storage::KVBuffers;
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

/// A read request forwarded from `IoBridge` to a reader task.
pub(crate) struct ReadRequest {
    pub buffer: SendableBuffer,
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
        let errno = if bytes_read < 0 { reader.get_error() } else { 0 };
        if response.send((bytes_read, errno)).is_err() {
            warn!(actor = ?node_handle, "reader task: reply receiver dropped, actor may have exited");
        }
    }
}

/// Global table of open channel endpoints, analogous to the kernel's open-file-descriptions table.
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

    fn alloc_handle(&mut self) -> ChannelHandle {
        let handle = ChannelHandle(self.next_id);
        self.next_id += 1;
        handle
    }

    fn insert_reader(&mut self, node_handle: Handle, request_tx: mpsc::UnboundedSender<ReadRequest>) -> ChannelHandle {
        let handle = self.alloc_handle();
        self.map.insert(handle, Channel::Reader { node_handle, request_tx });
        handle
    }

    fn insert_writer(&mut self, node_handle: Handle, std_handle: actor_runtime::StdHandle) -> ChannelHandle {
        let handle = self.alloc_handle();
        self.map.insert(handle, Channel::Writer { node_handle, std_handle });
        handle
    }

    fn get_reader_tx(&self, handle: ChannelHandle) -> Option<&mpsc::UnboundedSender<ReadRequest>> {
        if let Some(Channel::Reader { request_tx, .. }) = self.map.get(&handle) {
            Some(request_tx)
        } else {
            None
        }
    }

    fn remove(&mut self, handle: ChannelHandle) -> Option<Channel> {
        self.map.remove(&handle)
    }

    fn drop_actor_readers(&mut self, node_handle: Handle) {
        self.map.retain(|_, ch| {
            !matches!(ch, Channel::Reader { node_handle: h, .. } if *h == node_handle)
        });
    }
}

/// Directly-callable I/O bridge between actor threads and the async runtime.
///
/// Held as `Arc<IoBridge>` by each actor's `BlockingActorRuntime` and `ShutdownHandle`.
/// All methods are safe to call from a blocking thread.
pub struct IoBridge {
    pipe_pool: Arc<PipePool>,
    kv: Arc<dyn KVBuffers>,
    channel_table: Mutex<ChannelTable>,
    id_gen: Arc<IdGen>,
    attachment_manager: Arc<AttachmentManager>,
    /// Notifies the spawn loop when I/O state changes that may affect node readiness.
    /// Use cases:
    /// - a pipe is realized
    /// - a writer is closed
    notify: Arc<Notify>,
    /// Signals the executor when an actor's I/O is fully torn down.
    /// Out-of-domain for the IO bridge: the executor owns actor lifecycle state, but
    /// the signal originates here because I/O cleanup must complete before the executor
    /// can mark the actor Terminated and update DAG state.
    actor_done_tx: mpsc::UnboundedSender<ActorLifecycleEvent>,
}

impl IoBridge {
    #[must_use]
    pub fn new(
        kv: Arc<dyn KVBuffers>,
        id_gen: Arc<IdGen>,
        attachment_config: AttachmentConfig,
        // pipe_pool lives on Environment (system runtime); passed here until Step 4
        // when IoBridge will hold Arc<Environment> and access it directly.
        pipe_pool: Arc<PipePool>,
        notify: Arc<Notify>,
        actor_done_tx: mpsc::UnboundedSender<ActorLifecycleEvent>,
    ) -> Self {
        Self {
            pipe_pool,
            kv,
            channel_table: Mutex::new(ChannelTable::new()),
            id_gen,
            attachment_manager: Arc::new(AttachmentManager::new(attachment_config)),
            notify,
            actor_done_tx,
        }
    }

    /// Open a read channel. Currently returns a dummy handle (dependency wiring not implemented).
    pub fn open_read(&self, node_handle: Handle) -> ChannelHandle {
        let handle = self.channel_table.lock().alloc_handle();
        warn!(node = ?node_handle, channel = ?handle, "OpenRead: no input configured, returning dummy");
        handle
    }

    /// Register a writer channel and pre-create the pipe in the background.
    pub fn open_write(&self, node_handle: Handle) -> ChannelHandle {
        let std_handle = actor_runtime::StdHandle::Stdout;
        let handle = self.channel_table.lock().insert_writer(node_handle, std_handle);
        let pipe_pool = Arc::clone(&self.pipe_pool);
        let id_gen = Arc::clone(&self.id_gen);
        let notify = Arc::clone(&self.notify);
        tokio::spawn(async move {
            match pipe_pool.touch_writer(node_handle, std_handle, &id_gen).await {
                Ok(_) => notify.notify_one(),
                Err(e) => warn!(node = ?node_handle, error = %e, "failed to pre-create writer"),
            }
        });
        handle
    }

    /// Materialize stdin: create a `MergeReader`, spawn its reader task, return the channel handle.
    /// Called lazily on the actor's first read from stdin.
    pub fn materialize_stdin(&self, node_handle: Handle, dep_iterator: OwnedDependencyIterator) -> ChannelHandle {
        debug!(actor = ?node_handle, "materializing stdin reader");
        let reader = MergeReader::new(
            dep_iterator,
            Arc::clone(&self.pipe_pool),
            Arc::clone(&self.kv),
            Arc::clone(&self.id_gen),
        );
        let (request_tx, request_rx) = mpsc::unbounded_channel::<ReadRequest>();
        tokio::spawn(run_reader_task(node_handle, reader, request_rx));
        self.channel_table.lock().insert_reader(node_handle, request_tx)
    }

    /// Route a read request to the channel's reader task and block for the result.
    pub fn read(&self, handle: ChannelHandle, buffer: SendableBuffer) -> (isize, i32) {
        let tx = self.channel_table.lock().get_reader_tx(handle).cloned();
        if let Some(tx) = tx {
            let (resp_tx, resp_rx) = oneshot::channel();
            if tx.send(ReadRequest { buffer, response: resp_tx }).is_err() {
                warn!(channel = ?handle, "reader task has exited");
                return (0, 0);
            }
            resp_rx.blocking_recv().unwrap_or((0, 0))
        } else {
            warn!(channel = ?handle, "reader not available");
            (0, 0)
        }
    }

    /// Spawn an async write task and block for the result.
    pub fn write(&self, node_handle: Handle, std_handle: actor_runtime::StdHandle, data: Vec<u8>) -> isize {
        let (tx, rx) = oneshot::channel();
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
            if tx.send(result).is_err() {
                warn!(node = ?node_handle, std = ?std_handle, "write: reply receiver dropped");
            }
        });
        rx.blocking_recv().unwrap_or(-1)
    }

    /// Close a channel. For readers: drops the sender (reader task exits). For writers: flushes.
    pub fn close(&self, handle: ChannelHandle) -> isize {
        let channel = self.channel_table.lock().remove(handle);
        match channel {
            Some(Channel::Reader { .. }) => 0,
            Some(Channel::Writer { node_handle, std_handle }) => {
                debug!(channel = ?handle, node = ?node_handle, std = ?std_handle, "closing writer channel");
                if let Some(writer) = self.pipe_pool.get_already_realized_writer((node_handle, std_handle)) {
                    writer.close();
                }
                let (tx, rx) = oneshot::channel();
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
                    if tx.send(result).is_err() {
                        warn!(node = ?node_handle, std = ?std_handle, "close: reply receiver dropped");
                    }
                });
                rx.blocking_recv().unwrap_or(-1)
            }
            None => {
                warn!(channel = ?handle, "channel not found");
                -1
            }
        }
    }

    /// Close a writer directly via `PipePool` and flush.
    pub fn close_writer(&self, node_handle: Handle, std_handle: actor_runtime::StdHandle) -> isize {
        if let Some(writer) = self.pipe_pool.get_already_realized_writer((node_handle, std_handle)) {
            writer.close();
            debug!(node = ?node_handle, std = ?std_handle, "closed writer pipe");
        }
        let (tx, rx) = oneshot::channel();
        let pipe_pool = Arc::clone(&self.pipe_pool);
        let kv = Arc::clone(&self.kv);
        let notify = Arc::clone(&self.notify);
        tokio::spawn(async move {
            let result = if let Some(writer) = pipe_pool.get_already_realized_writer((node_handle, std_handle)) {
                match kv.flush_buffer(&writer.buffer()).await {
                    Ok(()) => 0,
                    Err(e) => { warn!(error = ?e, "failed to flush buffer"); -1 }
                }
            } else { 0 };
            notify.notify_one();
            if tx.send(result).is_err() {
                warn!(node = ?node_handle, std = ?std_handle, "close_writer: reply receiver dropped");
            }
        });
        rx.blocking_recv().unwrap_or(-1)
    }

    /// Execute two-phase actor shutdown from a blocking thread.
    ///
    /// Uses `blocking_recv` to wait for executor replies at each phase.
    /// Idempotent: if the actor is already Terminating or Terminated, returns early.
    pub fn actor_shutdown(&self, node_handle: Handle, exit_code: i32) {
        let (tx, rx) = oneshot::channel::<NodeState>();
        if self.actor_done_tx.send(ActorLifecycleEvent::Terminating {
            node_handle, reply: tx,
        }).is_err() {
            warn!(node = ?node_handle, "actor shutdown - executor gone, dropping");
            return;
        }

        let prior = rx.blocking_recv().unwrap_or(NodeState::Terminating);
        if matches!(prior, NodeState::Terminating | NodeState::Terminated) {
            debug!(node = ?node_handle, "actor shutdown - already terminating/terminated, ignoring");
            return;
        }

        debug!(node = ?node_handle, exit_code, "actor shutdown - closing all writers");
        self.pipe_pool.close_actor_writers(node_handle, exit_code);

        debug!(node = ?node_handle, "actor shutdown - dropping reader channels");
        self.channel_table.lock().drop_actor_readers(node_handle);

        let (tx2, rx2) = oneshot::channel::<NodeState>();
        if self.actor_done_tx.send(ActorLifecycleEvent::Terminated {
            node_handle, exit_code, reply: tx2,
        }).is_err() {
            warn!(node = ?node_handle, "actor shutdown - executor gone before Terminated");
            return;
        }
        if rx2.blocking_recv().is_err() {
            warn!(node = ?node_handle, "actor shutdown - Terminated reply dropped");
        }
    }

    /// Wait for all attachment tasks to complete. Call after all actors have shut down.
    pub async fn shutdown(&self) {
        self.attachment_manager.waiting_shutdown().await;
    }

    /// Inject a test reader task into the channel table.
    /// Returns the channel handle and the receiver end for the test to control.
    #[cfg(test)]
    pub fn inject_test_reader(
        &self,
        node_handle: Handle,
    ) -> (ChannelHandle, mpsc::UnboundedReceiver<ReadRequest>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let handle = self.channel_table.lock().insert_reader(node_handle, tx);
        (handle, rx)
    }
}
