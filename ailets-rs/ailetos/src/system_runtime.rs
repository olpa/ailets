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
//! and `aclose()` (see `stub_actor_runtime.rs`). These functions:
//! 1. Send an `IoRequest` to `SystemRuntime` via an async channel
//! 2. Call `blocking_recv()` to wait for the response
//! 3. Return the result to the actor
//!
//! The actor thread is **blocked** while waiting for the async operation to complete.
//!
//! ## Why pending_ops?
//!
//! The `pending_ops` queue (a `FuturesUnordered`) allows `SystemRuntime` to:
//! 1. **Accept multiple requests concurrently** - While one actor is blocked waiting
//!    for a slow read operation, other actors can send their requests
//! 2. **Process I/O operations in parallel** - Multiple reads/writes can execute
//!    concurrently using `tokio::select!` to poll both new requests and pending operations
//! 3. **Maintain responsiveness** - The runtime doesn't block waiting for one operation
//!    to complete before accepting new requests
//!
//! ## Why Box::pin(async move { ... }) inside handlers?
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
use std::os::raw::c_int;
use std::pin::Pin;
use std::sync::Arc;

use futures::stream::{FuturesUnordered, StreamExt};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, trace, warn, info};

use crate::dag::{Dag, OwnedDependencyIterator};
use crate::idgen::{Handle, IdGen};
use crate::merge_reader::MergeReader;
use crate::notification_queue::NotificationQueueArc;
use crate::pipepool::PipePool;
use crate::KVBuffers;

/// Global unique identifier for a pipe endpoint (reader or writer)
/// Used by SystemRuntime to identify channels across all actors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChannelHandle(pub usize);

/// A channel endpoint - either a reader or writer
pub enum Channel<K: KVBuffers> {
    /// Reader channel - holds the MergeReader (None when in use during async read)
    Reader(Option<MergeReader<K>>),
    /// Writer channel - holds the actor's node handle for pipe lookup
    Writer { node_handle: Handle },
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
    pub fn into_raw(self) -> *mut [u8] {
        #[cfg(debug_assertions)]
        {
            let already_consumed = self
                .consumed
                .swap(true, std::sync::atomic::Ordering::SeqCst);
            assert!(
                !already_consumed,
                "SendableBuffer used twice - this violates the safety contract!"
            );
        }
        self.ptr
    }
}

// SAFETY: See SendableBuffer documentation above
unsafe impl Send for SendableBuffer {}

/// Standard handles pre-opened for an actor
#[derive(Debug, Clone, Copy)]
pub struct StdHandles {
    pub stdin: ChannelHandle,
    pub stdout: ChannelHandle,
}

/// I/O requests sent from `ActorRuntime` to `SystemRuntime`
pub enum IoRequest {
    /// Setup standard handles for an actor (returns stdin and stdout ChannelHandles).
    /// Dependencies are obtained from the DAG inside SystemRuntime.
    SetupStdHandles {
        node_handle: Handle,
        response: oneshot::Sender<StdHandles>,
    },
    /// Open a stream for reading (returns global ChannelHandle)
    OpenRead {
        node_handle: Handle,
        response: oneshot::Sender<ChannelHandle>,
    },
    /// Open a stream for writing (returns global ChannelHandle)
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
        response: oneshot::Sender<c_int>,
    },
    /// Write to a channel (async operation)
    Write {
        handle: ChannelHandle,
        data: Vec<u8>,
        response: oneshot::Sender<c_int>,
    },
    /// Close a channel
    Close {
        handle: ChannelHandle,
        response: oneshot::Sender<c_int>,
    },
}

/// Result of a completed I/O operation
pub enum IoEvent<K: KVBuffers> {
    /// Read completed - need to return reader to its slot
    ReadComplete {
        handle: ChannelHandle,
        reader: MergeReader<K>,
        bytes_read: isize,
        response: oneshot::Sender<c_int>,
    },
    /// Synchronous operation completed (write, open, close)
    SyncComplete {
        result: c_int,
        response: oneshot::Sender<c_int>,
    },
}

/// Type alias for I/O futures
pub type IoFuture<K> = Pin<Box<dyn Future<Output = IoEvent<K>> + Send>>;

/// `SystemRuntime` manages all async I/O operations
/// Actors communicate with it via channels
pub struct SystemRuntime<K: KVBuffers> {
    /// The DAG describing actor dependencies
    dag: Arc<Dag>,
    /// Pool of output pipes (one per actor)
    pipe_pool: Arc<PipePool<K>>,
    /// Global channel table: ChannelHandle → Channel (reader or writer endpoint)
    channels: HashMap<ChannelHandle, Channel<K>>,
    /// Next channel handle ID
    next_channel_id: usize,
    /// Channel to send I/O requests to this runtime (None after `run()` starts)
    system_tx: Option<mpsc::UnboundedSender<IoRequest>>,
    /// Receives I/O requests from actors
    request_rx: mpsc::UnboundedReceiver<IoRequest>,
    /// ID generator for handles
    id_gen: Arc<IdGen>,
}

impl<K: KVBuffers + 'static> SystemRuntime<K> {
    pub fn new(dag: Arc<Dag>, kv: K, id_gen: Arc<IdGen>) -> Self {
        let (system_tx, request_rx) = mpsc::unbounded_channel();
        let notification_queue = NotificationQueueArc::new();

        Self {
            dag,
            pipe_pool: Arc::new(PipePool::new(kv, notification_queue)),
            channels: HashMap::new(),
            next_channel_id: 0,
            system_tx: Some(system_tx),
            request_rx,
            id_gen,
        }
    }

    /// Pre-open standard handles (stdin, stdout) for an actor just before it runs.
    ///
    /// Always uses MergeReader for stdin, regardless of dependency count:
    /// - 0 dependencies: MergeReader with empty deps (returns EOF immediately)
    /// - 1 dependency: MergeReader with single dep
    /// - N dependencies: MergeReader reads from each sequentially
    ///
    /// Dependencies are obtained from the DAG using `OwnedDependencyIterator`.
    async fn preopen_std_handles(&mut self, node_handle: Handle) -> StdHandles {
        debug!(actor = ?node_handle, "setting up std handles");

        // Create OwnedDependencyIterator from DAG
        let dep_iterator = OwnedDependencyIterator::new(
            Arc::clone(&self.dag),
            node_handle,
        );

        // Create MergeReader with the dependency iterator
        let merge_reader = MergeReader::new(
            dep_iterator,
            Arc::clone(&self.pipe_pool),
            Arc::clone(&self.id_gen),
        );

        let stdin = self.alloc_channel_handle();
        self.channels
            .insert(stdin, Channel::Reader(Some(merge_reader)));
        trace!(actor = ?node_handle, channel = ?stdin, "stdin configured with MergeReader");

        // Pre-open stdout: create output pipe
        debug!(actor = ?node_handle, "opening stdout");

        if !self.pipe_pool.has_pipe(node_handle) {
            let pipe_name = format!("pipes/actor-{}", node_handle.id());
            let pipe_handle = self.pipe_pool
                .create_output_pipe(node_handle, &pipe_name, &self.id_gen)
                .await;
            debug!(actor = ?node_handle, pipe = ?pipe_handle, "created output pipe");
        }

        let stdout = self.alloc_channel_handle();
        self.channels.insert(stdout, Channel::Writer { node_handle });
        trace!(actor = ?node_handle, channel = ?stdout, "stdout configured");

        StdHandles { stdin, stdout }
    }

    /// Allocate a new global channel handle
    fn alloc_channel_handle(&mut self) -> ChannelHandle {
        let handle = ChannelHandle(self.next_channel_id);
        self.next_channel_id += 1;
        handle
    }

    /// Get the sender for creating actor runtimes
    #[allow(clippy::expect_used)]
    pub fn get_system_tx(&self) -> mpsc::UnboundedSender<IoRequest> {
        self.system_tx.as_ref().expect("system_tx taken").clone()
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

    /// Handler for `OpenWrite` requests - creates output pipe and returns ChannelHandle
    async fn handle_open_write(
        &mut self,
        node_handle: Handle,
        response: oneshot::Sender<ChannelHandle>,
    ) {
        debug!(node = ?node_handle, "processing OpenWrite");

        // Create output pipe for this actor if it doesn't exist yet
        if !self.pipe_pool.has_pipe(node_handle) {
            let pipe_name = format!("pipes/actor-{}", node_handle.id());
            self.pipe_pool
                .create_output_pipe(node_handle, &pipe_name, &self.id_gen)
                .await;
            debug!(node = ?node_handle, "created pipe");
        }

        let channel_handle = self.alloc_channel_handle();
        self.channels
            .insert(channel_handle, Channel::Writer { node_handle });
        trace!(node = ?node_handle, channel = ?channel_handle, "OpenWrite created");
        let _ = response.send(channel_handle);
    }

    /// Handler for Read requests - uses ChannelHandle to find reader
    ///
    /// Uses the Sync-to-Async Bridge pattern (see module-level docs:
    /// "ARCHITECTURE: Sync-to-Async Bridge Pattern")
    fn handle_read(
        &mut self,
        handle: ChannelHandle,
        buffer: SendableBuffer,
        response: oneshot::Sender<c_int>,
    ) -> IoFuture<K> {
        trace!(channel = ?handle, "processing Read");

        if let Some(Channel::Reader(reader_slot)) = self.channels.get_mut(&handle) {
            if let Some(mut reader) = reader_slot.take() {
                trace!(channel = ?handle, "spawning async read");
                // See: ARCHITECTURE: Sync-to-Async Bridge Pattern
                Box::pin(async move {
                    // SAFETY: Buffer remains valid because aread() blocks until response
                    let buf = unsafe { &mut *buffer.into_raw() };
                    let bytes_read = reader.read(buf).await;
                    trace!(channel = ?handle, bytes = bytes_read, "read completed");
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

    /// Handler for Write requests - writes to actor's pipe
    ///
    /// Uses the Sync-to-Async Bridge pattern (see module-level docs:
    /// "ARCHITECTURE: Sync-to-Async Bridge Pattern")
    fn handle_write(
        &self,
        handle: ChannelHandle,
        data: &[u8],
        response: oneshot::Sender<c_int>,
    ) -> IoFuture<K> {
        trace!(channel = ?handle, bytes = data.len(), "processing Write");

        let result = if let Some(Channel::Writer { node_handle }) = self.channels.get(&handle) {
            let node_handle = *node_handle;
            trace!(channel = ?handle, "writing to pipe");
            let pipe = self.pipe_pool.get_pipe(node_handle);
            let n = pipe.writer().write(data);
            trace!(channel = ?handle, bytes = n, "pipe write returned");
            #[allow(clippy::cast_possible_truncation)]
            {
                n as c_int
            }
        } else {
            warn!(channel = ?handle, "channel not found or not a writer");
            -1
        };
        trace!(channel = ?handle, "write completed");
        // See: ARCHITECTURE: Sync-to-Async Bridge Pattern
        Box::pin(async move { IoEvent::SyncComplete { result, response } })
    }

    /// Handler for Close requests - uses ChannelHandle
    ///
    /// Uses the Sync-to-Async Bridge pattern (see module-level docs:
    /// "ARCHITECTURE: Sync-to-Async Bridge Pattern")
    fn handle_close(
        &mut self,
        handle: ChannelHandle,
        response: oneshot::Sender<c_int>,
    ) -> IoFuture<K> {
        trace!(channel = ?handle, "processing Close");

        if let Some(channel) = self.channels.remove(&handle) {
            match channel {
                Channel::Reader(_) => {
                    trace!(channel = ?handle, "closed reader");
                    // See: ARCHITECTURE: Sync-to-Async Bridge Pattern
                    Box::pin(async move {
                        IoEvent::SyncComplete {
                            result: 0,
                            response,
                        }
                    })
                }
                Channel::Writer { node_handle } => {
                    self.pipe_pool.get_pipe(node_handle).writer().close();
                    trace!(channel = ?handle, "closed writer");

                    let pipe_pool = Arc::clone(&self.pipe_pool);
                    // See: ARCHITECTURE: Sync-to-Async Bridge Pattern
                    Box::pin(async move {
                        let result_code = match pipe_pool.flush_buffer(node_handle).await {
                            Ok(()) => 0,
                            Err(e) => {
                                warn!(error = ?e, "failed to flush buffer");
                                -1
                            }
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

    /// Handler for `ReadComplete` events
    fn handle_read_complete(
        &mut self,
        handle: ChannelHandle,
        reader: MergeReader<K>,
        bytes_read: isize,
        response: oneshot::Sender<c_int>,
    ) {
        trace!(channel = ?handle, bytes = bytes_read, "read completed");
        // Put reader back into the channel
        if let Some(Channel::Reader(slot)) = self.channels.get_mut(&handle) {
            *slot = Some(reader);
        }
        #[allow(clippy::cast_possible_truncation)]
        let _ = response.send(bytes_read as c_int);
    }

    /// Handler for `SyncComplete` events
    fn handle_sync_complete(result: c_int, response: oneshot::Sender<c_int>) {
        let _ = response.send(result);
    }

    /// Main event loop - processes I/O requests asynchronously
    pub async fn run(mut self) {
        // Drop our copy of the sender so channel closes when all actors finish
        drop(self.system_tx.take());

        let mut pending_ops: FuturesUnordered<IoFuture<K>> = FuturesUnordered::new();
        let mut request_rx_open = true;

        loop {
            // Exit when no more requests can come and no operations are pending
            if !request_rx_open && pending_ops.is_empty() {
                info!("no more work, exiting");
                break;
            }

            tokio::select! {
                // Handle new requests from actors
                request = self.request_rx.recv(), if request_rx_open => {
                    if let Some(request) = request {
                        trace!("received request");
                        match request {
                            IoRequest::SetupStdHandles { node_handle, response } => {
                                let handles = self.preopen_std_handles(node_handle).await;
                                let _ = response.send(handles);
                            }
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
                            IoRequest::Write { handle, data, response } => {
                                let fut = self.handle_write(handle, &data, response);
                                pending_ops.push(fut);
                            }
                            IoRequest::Close { handle, response } => {
                                let fut = self.handle_close(handle, response);
                                pending_ops.push(fut);
                            }
                        };
                    } else {
                        debug!("request channel closed");
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
    }
}

/// Per-actor file descriptor table
/// Maps POSIX-style fd numbers to global ChannelHandles
pub struct FdTable {
    /// fd → ChannelHandle mapping
    table: HashMap<c_int, ChannelHandle>,
    /// Next fd to allocate
    next_fd: c_int,
}

impl FdTable {
    pub fn new() -> Self {
        Self {
            table: HashMap::new(),
            next_fd: 0,
        }
    }

    /// Allocate a new fd and associate it with a ChannelHandle
    pub fn insert(&mut self, handle: ChannelHandle) -> c_int {
        let fd = self.next_fd;
        self.next_fd += 1;
        self.table.insert(fd, handle);
        fd
    }

    /// Look up the ChannelHandle for a given fd
    pub fn get(&self, fd: c_int) -> Option<ChannelHandle> {
        self.table.get(&fd).copied()
    }

    /// Remove an fd mapping
    pub fn remove(&mut self, fd: c_int) -> Option<ChannelHandle> {
        self.table.remove(&fd)
    }

    /// Get all open file descriptors
    pub fn keys(&self) -> impl Iterator<Item = &c_int> {
        self.table.keys()
    }
}

impl Default for FdTable {
    fn default() -> Self {
        Self::new()
    }
}
