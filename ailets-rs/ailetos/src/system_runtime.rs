//! System runtime for managing actors and I/O operations
//!
//! This module provides the core runtime infrastructure for executing actors
//! in a multi-actor system. It handles:
//! - I/O request routing between actors and the system
//! - Channel management (reader/writer endpoints)
//! - File descriptor table management per actor
//! - Async I/O operations with sync-to-async bridging
//!
//! # ARCHITECTURE: Sync-to-Async Bridge Pattern
//!
//! This module uses a specific pattern `Box::pin(async move { ... })` in handlers
//! like `handle_close`, `handle_read`, and `handle_write`. This pattern is essential
//! for bridging synchronous actor code with asynchronous I/O operations.
//!
//! ## The Sync-to-Async Bridge
//!
//! Actors run synchronously and call blocking functions like `aread()`, `awrite()`,
//! and `aclose()` (see `BlockingActorRuntime`). These functions:
//! 1. Send an `IoRequest` to `SystemRuntime` via an async channel
//! 2. Call `blocking_recv()` to wait for the response
//! 3. Return the result to the actor
//!
//! The actor thread is **blocked** while waiting for the async operation to complete.
//!
//! ## Why `pending_ops`?
//!
//! The `pending_ops` queue (a `FuturesUnordered`) allows `SystemRuntime` to:
//! 1. **Accept multiple requests concurrently** - While one actor is blocked waiting
//!    for a slow read operation, other actors can send their requests
//! 2. **Process I/O operations in parallel** - Multiple reads/writes can execute
//!    concurrently using `tokio::select!` to poll both new requests and pending operations
//! 3. **Maintain responsiveness** - The runtime doesn't block waiting for one operation
//!    to complete before accepting new requests
//!
//! ## Why `Box::pin(async` move { ... }) inside handlers?
//!
//! Handlers cannot be `async fn` because:
//! 1. An `async fn` returns a future that captures `&mut self` by reference
//! 2. This borrow would need a `'static` lifetime for `pending_ops`
//! 3. This prevents any other use of `self` in the `tokio::select!` loop
//!
//! Instead, handlers:
//! 1. Perform synchronous setup with `&mut self` (e.g., remove channel)
//! 2. Clone any Arc references needed for async work
//! 3. Return `Box::pin(async move { ... })` that owns all its data
//! 4. The `&mut self` borrow ends when the handler returns
//! 5. The returned future can be pushed to `pending_ops` without borrowing issues
//!
//! **Important**: We cannot move `Box::pin` to the call site because even
//! `Box::pin(self.async_handler(...))` still borrows `&mut self` for `'static`.
//!
//! ## Example Flow (Close operation)
//!
//! 1. Actor calls `aclose(fd)` (blocking)
//! 2. `aclose` sends `IoRequest::Close` and blocks on `rx.blocking_recv()`
//! 3. `SystemRuntime::run` receives the request in `tokio::select!`
//! 4. Calls `handle_close()` which:
//!    - Removes the channel from `self.channels` (synchronous)
//!    - Closes the writer pipe (synchronous)
//!    - Clones `pipe_pool` Arc
//!    - Returns `Box::pin(async move { pipe_pool.flush_buffer().await })`
//! 5. The `&mut self` borrow ends, future is pushed to `pending_ops`
//! 6. `SystemRuntime` continues processing new requests immediately
//! 7. When the flush completes, `pending_ops.next()` yields the result
//! 8. Response is sent back to the actor via oneshot channel
//! 9. Actor's `blocking_recv()` returns, unblocking the actor thread
//!
//! This architecture bridges synchronous actor code with async I/O operations
//! while maintaining concurrency across multiple actors.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use futures::stream::{FuturesUnordered, StreamExt};
use parking_lot::RwLock;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error, trace, warn};

use crate::attachments::{AttachmentConfig, AttachmentManager};
use crate::dag::{Dag, NodeState, OwnedDependencyIterator};
use crate::idgen::{Handle, IdGen};
use crate::merge_reader::MergeReader;
use crate::notification_queue::NotificationQueueArc;
use crate::pipepool::PipePool;
use crate::KVBuffers;

/// Global unique identifier for a pipe endpoint (reader or writer)
/// Used by `SystemRuntime` to identify channels across all actors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChannelHandle(pub isize);

/// A channel endpoint - either a reader or writer
pub enum Channel<K: KVBuffers> {
    /// Reader channel - holds the `MergeReader` (None when in use during async read)
    Reader(Option<MergeReader<K>>),
    /// Writer channel - holds the actor's node handle and std handle for pipe lookup
    Writer {
        node_handle: Handle,
        std_handle: actor_runtime::StdHandle,
    },
}

/// A wrapper around a raw mutable slice pointer that can be sent between threads.
/// SAFETY: This is only safe because the sender (aread) blocks until the receiver
/// (`SystemRuntime` handler) sends a response, ensuring:
/// 1. The buffer remains valid (stack frame doesn't unwind)
/// 2. No concurrent access (sender is blocked)
/// 3. Proper synchronization (channel enforces happens-before)
pub struct SendableBuffer {
    ptr: *mut [u8],
    #[cfg(debug_assertions)]
    consumed: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl SendableBuffer {
    /// Create a new `SendableBuffer` from a mutable slice reference.
    ///
    /// # Safety
    ///
    /// Caller must ensure:
    /// 1. The pointer remains valid until consumed via `into_raw()`
    /// 2. The caller will block waiting for a response before the buffer goes out of scope
    /// 3. No other references to this buffer exist during the async operation
    /// 4. The `SendableBuffer` is consumed exactly once via `into_raw()`
    pub unsafe fn new(buffer: &mut [u8]) -> Self {
        Self {
            ptr: std::ptr::from_mut::<[u8]>(buffer),
            #[cfg(debug_assertions)]
            consumed: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Consume the `SendableBuffer` and return the raw pointer.
    /// This prevents accidental reuse of the same buffer.
    ///
    /// If the buffer has already been consumed, logs a critical error but still returns
    /// the pointer. This violates the safety contract and indicates a serious programming error.
    #[must_use]
    pub fn into_raw(self) -> *mut [u8] {
        let already_consumed = self
            .consumed
            .swap(true, std::sync::atomic::Ordering::SeqCst);
        if already_consumed {
            error!(
                "CRITICAL: SendableBuffer used twice - safety contract violated! \
                 This may lead to use-after-free or double-free bugs."
            );
        }
        self.ptr
    }
}

// SAFETY: See SendableBuffer documentation above
unsafe impl Send for SendableBuffer {}

/// I/O requests sent from `ActorRuntime` to `SystemRuntime`
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
    Read {
        handle: ChannelHandle,
        buffer: SendableBuffer,
        response: oneshot::Sender<isize>,
    },
    /// Write to an actor's output pipe (async operation)
    /// Uses node_handle + std_handle to write directly to PipePool
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
    /// Actor shutdown - close all writers (realized and latent) for this actor
    ActorShutdown { node_handle: Handle },
    /// Materialize stdin reader for an actor (creates MergeReader, returns ChannelHandle)
    /// Called on first read from stdin
    MaterializeStdin {
        node_handle: Handle,
        response: oneshot::Sender<ChannelHandle>,
    },
    /// Close a writer for an actor
    /// Uses node_handle + std_handle to find and close the writer in PipePool
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
        response: oneshot::Sender<isize>,
    },
    /// Synchronous operation completed (write, open, close)
    SyncComplete {
        result: isize,
        response: oneshot::Sender<isize>,
    },
}

/// Type alias for I/O futures
pub type IoFuture<K> = Pin<Box<dyn Future<Output = IoEvent<K>> + Send>>;

/// `SystemRuntime` manages all async I/O operations
/// Actors communicate with it via channels
pub struct SystemRuntime<K: KVBuffers> {
    /// The DAG describing actor dependencies (wrapped in `RwLock` for state updates)
    dag: Arc<RwLock<Dag>>,
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
}

impl<K: KVBuffers + 'static> SystemRuntime<K> {
    pub fn new(
        dag: Arc<RwLock<Dag>>,
        kv: Arc<K>,
        id_gen: Arc<IdGen>,
        attachment_config: AttachmentConfig,
    ) -> Self {
        let (system_tx, request_rx) = mpsc::unbounded_channel();
        let notification_queue = NotificationQueueArc::new();

        // Create pipe pool
        let pipe_pool = Arc::new(PipePool::new(Arc::clone(&kv), notification_queue));

        // Create attachment manager
        let attachment_manager = Arc::new(AttachmentManager::new(attachment_config));

        // Set up callback - captures pipe_pool, id_gen, and attachment_manager
        let callback_pipe_pool = Arc::clone(&pipe_pool);
        let callback_id_gen = Arc::clone(&id_gen);
        let callback_attachment = Arc::clone(&attachment_manager);
        let callback: crate::pipepool::WriterRealizedCallback = Arc::new(move |node_handle, std_handle| {
            let pipe_pool = Arc::clone(&callback_pipe_pool);
            let id_gen = Arc::clone(&callback_id_gen);
            let attachment = Arc::clone(&callback_attachment);
            Box::pin(async move {
                attachment
                    .on_writer_realized(node_handle, std_handle, pipe_pool, id_gen)
                    .await;
            })
        });
        pipe_pool.set_writer_realized_callback(callback);

        Self {
            dag,
            pipe_pool,
            kv,
            channels: HashMap::new(),
            next_channel_id: 0,
            system_tx: Some(system_tx),
            request_rx,
            id_gen,
            attachment_manager,
        }
    }


    /// Materialize stdin reader for an actor on first read
    ///
    /// Creates a `MergeReader` with dependencies from the DAG and returns a `ChannelHandle`.
    /// Called lazily when the actor first reads from stdin.
    fn materialize_stdin(&mut self, node_handle: Handle) -> ChannelHandle {
        debug!(actor = ?node_handle, "materializing stdin reader");

        // Create OwnedDependencyIterator from DAG
        let dep_iterator = OwnedDependencyIterator::new(Arc::clone(&self.dag), node_handle);

        // Create MergeReader with the dependency iterator
        let merge_reader = MergeReader::new(
            dep_iterator,
            Arc::clone(&self.pipe_pool),
            Arc::clone(&self.id_gen),
        );

        let stdin = self.alloc_channel_handle();
        self.channels
            .insert(stdin, Channel::Reader(Some(merge_reader)));

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
        response: oneshot::Sender<isize>,
    ) -> IoFuture<K> {
        if let Some(Channel::Reader(reader_slot)) = self.channels.get_mut(&handle) {
            if let Some(mut reader) = reader_slot.take() {
                // See: ARCHITECTURE: Sync-to-Async Bridge Pattern
                Box::pin(async move {
                    // SAFETY: Buffer remains valid because aread() blocks until response
                    let buf = unsafe { &mut *buffer.into_raw() };
                    let bytes_read = reader.read(buf).await;
                    IoEvent::ReadComplete {
                        handle,
                        reader,
                        bytes_read,
                        response,
                    }
                })
            } else {
                warn!(channel = ?handle, "reader not available (already in use?)");
                // See: ARCHITECTURE: Sync-to-Async Bridge Pattern
                Box::pin(async move {
                    IoEvent::SyncComplete {
                        result: 0,
                        response,
                    }
                })
            }
        } else {
            warn!(channel = ?handle, "channel not found or not a reader");
            // See: ARCHITECTURE: Sync-to-Async Bridge Pattern
            Box::pin(async move {
                IoEvent::SyncComplete {
                    result: 0,
                    response,
                }
            })
        }
    }

    /// Handler for Write requests - writes to actor's pipe (creates pipe lazily)
    ///
    /// Uses the Sync-to-Async Bridge pattern (see module-level docs:
    /// "ARCHITECTURE: Sync-to-Async Bridge Pattern")
    ///
    /// Writes directly to PipePool using node_handle + std_handle from the request.
    /// No Channel table lookup needed - PipePool handles lazy pipe creation.
    fn handle_write(
        &self,
        node_handle: Handle,
        std_handle: actor_runtime::StdHandle,
        data: &[u8],
        response: oneshot::Sender<isize>,
    ) -> IoFuture<K> {
        let pipe_pool = Arc::clone(&self.pipe_pool);
        let id_gen = Arc::clone(&self.id_gen);
        let data = data.to_vec();

        // See: ARCHITECTURE: Sync-to-Async Bridge Pattern
        Box::pin(async move {
            // Get or create writer (idempotent - handles latent->realized transition)
            let result = match pipe_pool
                .touch_writer(node_handle, std_handle, &id_gen)
                .await
            {
                Ok(writer) => writer.write(&data),
                Err(e) => {
                    warn!(node = ?node_handle, std = ?std_handle, error = %e, "failed to get writer");
                    -1
                }
            };

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
                Channel::Reader(_) => {
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

    /// Handler for CloseWriter requests - closes writer directly via PipePool
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
        response: oneshot::Sender<isize>,
    ) {
        // Put reader back into the channel
        if let Some(Channel::Reader(slot)) = self.channels.get_mut(&handle) {
            *slot = Some(reader);
        }
        let _ = response.send(bytes_read);
    }

    /// Handler for `SyncComplete` events
    fn handle_sync_complete(result: isize, response: oneshot::Sender<isize>) {
        let _ = response.send(result);
    }

    /// Main event loop - processes I/O requests asynchronously
    pub async fn run(mut self) {
        trace!("SystemRuntime::run: entering request_rx loop");
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
                            IoRequest::ActorShutdown { node_handle } => {
                                debug!(node = ?node_handle, "actor shutdown - setting state to Terminating");
                                self.dag.write().set_state(node_handle, NodeState::Terminating);

                                debug!(node = ?node_handle, "actor shutdown - closing all writers");
                                self.pipe_pool.close_actor_writers(node_handle);

                                debug!(node = ?node_handle, "actor shutdown - setting state to Terminated");
                                self.dag.write().set_state(node_handle, NodeState::Terminated);
                            }
                            IoRequest::MaterializeStdin { node_handle, response } => {
                                let channel_handle = self.materialize_stdin(node_handle);
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
                        IoEvent::ReadComplete { handle, reader, bytes_read, response } => {
                            self.handle_read_complete(handle, reader, bytes_read, response);
                        }
                        IoEvent::SyncComplete { result, response } => {
                            Self::handle_sync_complete(result, response);
                        }
                    }
                }
            }
        }

        trace!("SystemRuntime::run: exited request_rx loop");

        // Wait for all attachment tasks to complete
        self.attachment_manager.waiting_shutdown().await;

        // Clear the callback to break circular reference (callback captures Arc<PipePool>)
        self.pipe_pool.clear_writer_realized_callback();
        trace!("SystemRuntime: destroying");
    }
}

