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
//! # Sync-to-Async Bridge
//!
//! Actors call blocking functions (`aread`, `awrite`, `aclose`) that:
//! 1. Send an [`IoRequest`] to `IoBridge` via an unbounded mpsc channel
//! 2. Call `blocking_recv()` on a oneshot channel to wait for the reply
//! 3. Return the result to the actor
//!
//! The actor thread is **blocked** the entire time. `IoBridge` runs concurrently
//! on the Tokio runtime and never blocks.
//!
//! ## Why `pending_ops`?
//!
//! The `pending_ops` queue (a `FuturesUnordered`) lets `IoBridge` handle multiple
//! actors at once:
//! 1. While one actor waits for a slow read, others can send their requests
//! 2. Reads, writes, and flushes execute concurrently via `tokio::select!`
//! 3. The bridge never stalls on one operation before accepting the next
//!
//! ## Why `Box::pin(async move { ... })` inside handlers?
//!
//! Handlers cannot be `async fn` because an async fn borrows `&mut self` for the
//! duration of the future — which conflicts with pushing that future into `pending_ops`
//! while still needing `self` in the select loop.
//!
//! Instead, each handler:
//! 1. Does synchronous setup with `&mut self` (e.g., removes a channel from the table)
//! 2. Clones any `Arc`s needed for the async work
//! 3. Returns `Box::pin(async move { ... })` that owns all its data
//!
//! The `&mut self` borrow ends when the handler returns; the boxed future goes into
//! `pending_ops` with no remaining borrow on `self`.
//!
//! (`Box::pin(self.handler())` at the call site does not help — it still borrows
//! `&mut self` for `'static` before the pin is created.)
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
//! 2. **I/O cleanup** — writers are closed, reader channels are dropped.
//! 3. **Terminated** — `IoBridge` sends `ActorLifecycleEvent::Terminated` and awaits the
//!    reply. The executor sets `NodeState::Terminated`, records the exit code, and fires
//!    `notify` so the spawn loop can react.
//!
//! The reply channels enforce ordering: each phase waits for the executor to finish
//! its DAG update before the next phase begins.
//!
//! ## Example flow (close operation)
//!
//! 1. Actor calls `aclose(fd)` — blocks on `rx.blocking_recv()`
//! 2. `stub_actor_runtime` sends `IoRequest::Close` on the mpsc channel
//! 3. `IoBridge::run` receives it in `tokio::select!`
//! 4. `handle_close()` removes the channel from `self.channels`, closes the pipe
//!    writer synchronously, clones `pipe_pool`, returns `Box::pin(async move { flush })`
//! 5. The `&mut self` borrow ends; the future is pushed to `pending_ops`
//! 6. `IoBridge` immediately loops back to accept new requests
//! 7. When the flush completes, `pending_ops.next()` yields the result
//! 8. The oneshot reply is sent; `blocking_recv()` on the actor thread returns

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use futures::stream::{FuturesUnordered, StreamExt};
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
pub enum Channel<K: KVBuffers> {
    /// Reader channel - holds the `MergeReader` (None when in use during async read)
    Reader {
        node_handle: Handle,
        reader: Option<MergeReader<K>>,
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

/// Result of a completed I/O operation
pub enum IoEvent<K: KVBuffers> {
    /// Read completed - need to return reader to its slot
    ReadComplete {
        handle: ChannelHandle,
        reader: MergeReader<K>,
        bytes_read: isize,
        /// errno when `bytes_read` < 0, else 0
        errno: i32,
        response: oneshot::Sender<(isize, i32)>,
    },
    /// Synchronous operation completed (write, open, close)
    SyncComplete {
        result: isize,
        response: oneshot::Sender<isize>,
    },
}

/// Type alias for I/O futures
pub type IoFuture<K> = Pin<Box<dyn Future<Output = IoEvent<K>> + Send>>;

/// `IoBridge` manages all async I/O operations
/// Actors communicate with it via channels
pub struct IoBridge<K: KVBuffers> {
    /// Pool of output pipes (one per actor)
    pipe_pool: Arc<PipePool<K>>,
    /// Key-value store for pipe buffers
    kv: Arc<K>,
    /// Global channel table: `ChannelHandle` → Channel (reader or writer endpoint)
    channels: HashMap<ChannelHandle, Channel<K>>,
    /// Next channel handle ID
    next_channel_id: isize,
    /// Channel to send I/O requests to this runtime (None after `run()` starts)
    system_tx: Option<mpsc::UnboundedSender<IoRequest>>,
    /// Receives I/O requests from actors
    request_rx: mpsc::UnboundedReceiver<IoRequest>,
    /// ID generator for handles
    id_gen: Arc<IdGen>,
    /// Manages dynamic attachment of actor streams to host stdout/stderr
    attachment_manager: Arc<AttachmentManager>,
    /// Notifies the spawn loop when something changes that may affect node readiness
    /// Fired when I/O state changes that may affect dependent actor readiness.
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

        // Create attachment manager
        let attachment_manager = Arc::new(AttachmentManager::new(attachment_config));

        Self {
            pipe_pool,
            kv,
            channels: HashMap::new(),
            next_channel_id: 0,
            system_tx: Some(system_tx),
            request_rx,
            id_gen,
            attachment_manager,
            notify,
            actor_done_tx,
        }
    }

    /// Materialize stdin reader for an actor on first read
    ///
    /// Creates a `MergeReader` from the pre-built iterator and returns a `ChannelHandle`.
    /// Called lazily when the actor first reads from stdin.
    fn materialize_stdin(&mut self, node_handle: Handle, dep_iterator: OwnedDependencyIterator) -> ChannelHandle {
        debug!(actor = ?node_handle, "materializing stdin reader");

        let merge_reader = MergeReader::new(
            dep_iterator,
            Arc::clone(&self.pipe_pool),
            Arc::clone(&self.kv),
            Arc::clone(&self.id_gen),
        );

        let stdin = self.alloc_channel_handle();
        self.channels.insert(
            stdin,
            Channel::Reader {
                node_handle,
                reader: Some(merge_reader),
            },
        );

        stdin
    }

    /// Allocate a new global channel handle
    fn alloc_channel_handle(&mut self) -> ChannelHandle {
        let handle = ChannelHandle(self.next_channel_id);
        self.next_channel_id += 1;
        handle
    }

    /// Get the sender for creating actor runtimes
    ///
    /// Returns `None` if called after `run()` has started (which consumes the sender).
    #[must_use]
    pub fn get_system_tx(&self) -> Option<mpsc::UnboundedSender<IoRequest>> {
        self.system_tx.clone()
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
        self.channels.retain(|_, ch| {
            !matches!(ch, Channel::Reader { node_handle: h, .. } if *h == node_handle)
        });

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
        // No dependency wiring for now - just return a dummy handle
        let channel_handle = self.alloc_channel_handle();
        warn!(node = ?node_handle, channel = ?channel_handle, "OpenRead: no input configured, returning dummy");
        let _ = response.send(channel_handle);
    }

    /// Handler for `OpenWrite` requests - creates output pipe and returns `ChannelHandle`
    async fn handle_open_write(
        &mut self,
        node_handle: Handle,
        response: oneshot::Sender<ChannelHandle>,
    ) {
        debug!(node = ?node_handle, "processing OpenWrite");

        // Ensure writer exists for this actor (idempotent)
        // TODO: OpenWrite should specify which StdHandle to open, for now default to Stdout
        let std_handle = actor_runtime::StdHandle::Stdout;
        match self
            .pipe_pool
            .touch_writer(node_handle, std_handle, &self.id_gen)
            .await
        {
            Ok(_) => {
                debug!(node = ?node_handle, "pipe ready");
                self.notify.notify_one();
            }
            Err(e) => {
                warn!(node = ?node_handle, error = %e, "failed to create writer");
            }
        }

        let channel_handle = self.alloc_channel_handle();
        self.channels.insert(
            channel_handle,
            Channel::Writer {
                node_handle,
                std_handle,
            },
        );
        let _ = response.send(channel_handle);
    }

    /// Handler for Read requests - uses `ChannelHandle` to find reader
    ///
    /// Uses the Sync-to-Async Bridge pattern (see module-level docs:
    /// "ARCHITECTURE: Sync-to-Async Bridge Pattern")
    fn handle_read(
        &mut self,
        handle: ChannelHandle,
        buffer: SendableBuffer,
        response: oneshot::Sender<(isize, i32)>,
    ) -> IoFuture<K> {
        if let Some(Channel::Reader {
            reader: reader_slot,
            ..
        }) = self.channels.get_mut(&handle)
        {
            if let Some(mut reader) = reader_slot.take() {
                // See: ARCHITECTURE: Sync-to-Async Bridge Pattern
                Box::pin(async move {
                    // SAFETY: Buffer remains valid because aread() blocks until response
                    let buf = unsafe { &mut *buffer.into_raw() };
                    let bytes_read = reader.read(buf).await;
                    let errno = if bytes_read < 0 {
                        reader.get_error()
                    } else {
                        0
                    };
                    IoEvent::ReadComplete {
                        handle,
                        reader,
                        bytes_read,
                        errno,
                        response,
                    }
                })
            } else {
                warn!(channel = ?handle, "reader not available (already in use?)");
                Box::pin(async move {
                    let _ = response.send((0, 0));
                    let (dummy_tx, _) = oneshot::channel();
                    IoEvent::SyncComplete {
                        result: 0,
                        response: dummy_tx,
                    }
                })
            }
        } else {
            warn!(channel = ?handle, "channel not found or not a reader");
            Box::pin(async move {
                let _ = response.send((0, 0));
                let (dummy_tx, _) = oneshot::channel();
                IoEvent::SyncComplete {
                    result: 0,
                    response: dummy_tx,
                }
            })
        }
    }

    /// Handler for Write requests - writes to actor's pipe (creates pipe lazily)
    ///
    /// Uses the Sync-to-Async Bridge pattern (see module-level docs:
    /// "ARCHITECTURE: Sync-to-Async Bridge Pattern")
    ///
    /// Writes directly to `PipePool` using `node_handle` + `std_handle` from the request.
    /// No Channel table lookup needed - `PipePool` handles lazy pipe creation.
    ///
    /// When a writer is newly created, triggers attachment via `AttachmentManager`.
    fn handle_write(
        &self,
        node_handle: Handle,
        std_handle: actor_runtime::StdHandle,
        data: &[u8],
        response: oneshot::Sender<isize>,
    ) -> IoFuture<K> {
        let pipe_pool = Arc::clone(&self.pipe_pool);
        let id_gen = Arc::clone(&self.id_gen);
        let attachment_manager = Arc::clone(&self.attachment_manager);
        let notify = Arc::clone(&self.notify);
        let data = data.to_vec();

        // See: ARCHITECTURE: Sync-to-Async Bridge Pattern
        Box::pin(async move {
            // Get or create writer (idempotent - handles latent->realized transition)
            let result = match pipe_pool
                .touch_writer(node_handle, std_handle, &id_gen)
                .await
            {
                Ok((writer, is_new)) => {
                    // Trigger attachment if this is a newly created writer
                    if is_new {
                        attachment_manager.on_writer_realized(
                            node_handle,
                            std_handle,
                            Arc::clone(&pipe_pool),
                            Arc::clone(&id_gen),
                        );
                    }
                    let n = writer.write(&data);
                    if n < 0 {
                        let errno = writer.get_error();
                        if errno != 0 {
                            -(errno as isize)
                        } else {
                            -1
                        }
                    } else {
                        n
                    }
                }
                Err(e) => {
                    warn!(node = ?node_handle, std = ?std_handle, error = %e, "failed to get writer");
                    -1
                }
            };

            notify.notify_one();
            IoEvent::SyncComplete { result, response }
        })
    }

    /// Handler for Close requests - uses `ChannelHandle`
    ///
    /// Uses the Sync-to-Async Bridge pattern (see module-level docs:
    /// "ARCHITECTURE: Sync-to-Async Bridge Pattern")
    fn handle_close(
        &mut self,
        handle: ChannelHandle,
        response: oneshot::Sender<isize>,
    ) -> IoFuture<K> {
        if let Some(channel) = self.channels.remove(&handle) {
            match channel {
                Channel::Reader { .. } => {
                    // See: ARCHITECTURE: Sync-to-Async Bridge Pattern
                    Box::pin(async move {
                        IoEvent::SyncComplete {
                            result: 0,
                            response,
                        }
                    })
                }
                Channel::Writer {
                    node_handle,
                    std_handle,
                } => {
                    debug!(channel = ?handle, node = ?node_handle, std = ?std_handle, "closing writer channel");

                    // Close the writer
                    if let Some(writer) = self
                        .pipe_pool
                        .get_already_realized_writer((node_handle, std_handle))
                    {
                        writer.close();
                        debug!(channel = ?handle, node = ?node_handle, std = ?std_handle, "closed writer pipe");
                    }

                    let pipe_pool = Arc::clone(&self.pipe_pool);
                    let kv = Arc::clone(&self.kv);
                    let notify = Arc::clone(&self.notify);
                    // See: ARCHITECTURE: Sync-to-Async Bridge Pattern
                    Box::pin(async move {
                        let result_code = if let Some(writer) =
                            pipe_pool.get_already_realized_writer((node_handle, std_handle))
                        {
                            match kv.flush_buffer(&writer.buffer()).await {
                                Ok(()) => 0,
                                Err(e) => {
                                    warn!(error = ?e, "failed to flush buffer");
                                    -1
                                }
                            }
                        } else {
                            warn!(node = ?node_handle, std = ?std_handle, "writer not found for flush");
                            -1
                        };

                        notify.notify_one();
                        IoEvent::SyncComplete {
                            result: result_code,
                            response,
                        }
                    })
                }
            }
        } else {
            warn!(channel = ?handle, "channel not found");
            // See: ARCHITECTURE: Sync-to-Async Bridge Pattern
            Box::pin(async move {
                IoEvent::SyncComplete {
                    result: -1,
                    response,
                }
            })
        }
    }

    /// Handler for `CloseWriter` requests - closes writer directly via `PipePool`
    ///
    /// Uses the Sync-to-Async Bridge pattern (see module-level docs:
    /// "ARCHITECTURE: Sync-to-Async Bridge Pattern")
    fn handle_close_writer(
        &self,
        node_handle: Handle,
        std_handle: actor_runtime::StdHandle,
        response: oneshot::Sender<isize>,
    ) -> IoFuture<K> {
        // Close the writer if it exists
        if let Some(writer) = self
            .pipe_pool
            .get_already_realized_writer((node_handle, std_handle))
        {
            writer.close();
            debug!(node = ?node_handle, std = ?std_handle, "closed writer pipe");
        }

        let pipe_pool = Arc::clone(&self.pipe_pool);
        let kv = Arc::clone(&self.kv);
        let notify = Arc::clone(&self.notify);

        // See: ARCHITECTURE: Sync-to-Async Bridge Pattern
        Box::pin(async move {
            let result_code = if let Some(writer) =
                pipe_pool.get_already_realized_writer((node_handle, std_handle))
            {
                match kv.flush_buffer(&writer.buffer()).await {
                    Ok(()) => 0,
                    Err(e) => {
                        warn!(error = ?e, "failed to flush buffer");
                        -1
                    }
                }
            } else {
                // Writer was never created - that's OK, just succeed
                0
            };

            notify.notify_one();
            IoEvent::SyncComplete {
                result: result_code,
                response,
            }
        })
    }

    /// Handler for `ReadComplete` events
    fn handle_read_complete(
        &mut self,
        handle: ChannelHandle,
        reader: MergeReader<K>,
        bytes_read: isize,
        errno: i32,
        response: oneshot::Sender<(isize, i32)>,
    ) {
        // Put reader back into the channel
        if let Some(Channel::Reader { reader: slot, .. }) = self.channels.get_mut(&handle) {
            *slot = Some(reader);
        }
        let _ = response.send((bytes_read, errno));
    }

    /// Handler for `SyncComplete` events
    fn handle_sync_complete(result: isize, response: oneshot::Sender<isize>) {
        let _ = response.send(result);
    }

    /// Main event loop - processes I/O requests asynchronously
    pub async fn run(mut self) {
        trace!("IoBridge::run: entering request_rx loop");
        // Drop our copy of the sender so channel closes when all actors finish
        drop(self.system_tx.take());

        let mut pending_ops: FuturesUnordered<IoFuture<K>> = FuturesUnordered::new();
        let mut request_rx_open = true;

        loop {
            // Exit when no more requests can come and no operations are pending
            if !request_rx_open && pending_ops.is_empty() {
                debug!("no more work, exiting");
                break;
            }

            tokio::select! {
                // Handle new requests from actors
                request = self.request_rx.recv(), if request_rx_open => {
                    if let Some(request) = request {
                        match request {
                            IoRequest::OpenRead { node_handle, response } => {
                                self.handle_open_read(node_handle, response);
                            }
                            IoRequest::OpenWrite { node_handle, response } => {
                                self.handle_open_write(node_handle, response).await;
                            }
                            IoRequest::Read { handle, buffer, response } => {
                                let fut = self.handle_read(handle, buffer, response);
                                pending_ops.push(fut);
                            }
                            IoRequest::Write { node_handle, std_handle, data, response } => {
                                let fut = self.handle_write(node_handle, std_handle, &data, response);
                                pending_ops.push(fut);
                            }
                            IoRequest::Close { handle, response } => {
                                let fut = self.handle_close(handle, response);
                                pending_ops.push(fut);
                            }
                            IoRequest::ActorShutdown { node_handle, exit_code } => {
                                self.handle_actor_shutdown(node_handle, exit_code).await;
                            }
                            IoRequest::MaterializeStdin { node_handle, dep_iterator, response } => {
                                let channel_handle = self.materialize_stdin(node_handle, dep_iterator);
                                let _ = response.send(channel_handle);
                            }
                            IoRequest::CloseWriter { node_handle, std_handle, response } => {
                                let fut = self.handle_close_writer(node_handle, std_handle, response);
                                pending_ops.push(fut);
                            }
                        }
                    } else {
                        debug!("request channel closed - all actors finished");
                        request_rx_open = false;
                    }
                }

                // Handle completed operations
                Some(event) = pending_ops.next(), if !pending_ops.is_empty() => {
                    match event {
                        IoEvent::ReadComplete { handle, reader, bytes_read, errno, response } => {
                            self.handle_read_complete(handle, reader, bytes_read, errno, response);
                        }
                        IoEvent::SyncComplete { result, response } => {
                            Self::handle_sync_complete(result, response);
                        }
                    }
                }
            }
        }

        trace!("IoBridge::run: exited request_rx loop");

        // Wait for all attachment tasks to complete
        // Attachment tasks read their pipes to EOF, then exit naturally when pipes close
        self.attachment_manager.waiting_shutdown().await;
        trace!("IoBridge: all attachments completed");

        trace!("IoBridge: destroying");
    }
}
