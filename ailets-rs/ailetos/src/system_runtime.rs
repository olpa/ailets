//! System runtime for managing actors and I/O operations
//!
//! This module provides the core runtime infrastructure for executing actors
//! in a multi-actor system. It handles:
//! - I/O request routing between actors and the system
//! - Channel management (reader/writer endpoints)
//! - File descriptor table management per actor
//! - Async I/O operations with sync-to-async bridging

use std::collections::HashMap;
use std::future::Future;
use std::os::raw::c_int;
use std::pin::Pin;
use std::sync::Arc;

use futures::stream::{FuturesUnordered, StreamExt};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, trace, warn, info};

use crate::idgen::{Handle, IdGen};
use crate::notification_queue::NotificationQueueArc;
use crate::pipe::Reader;
use crate::pipepool::PipePool;
use crate::KVBuffers;

/// Global unique identifier for a pipe endpoint (reader or writer)
/// Used by SystemRuntime to identify channels across all actors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChannelHandle(pub usize);

/// A channel endpoint - either a reader or writer
pub enum Channel {
    /// Reader channel - holds the Reader (None when in use during async read)
    Reader(Option<Reader>),
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
    /// Setup standard handles for an actor (returns stdin and stdout ChannelHandles)
    SetupStdHandles {
        node_handle: Handle,
        dependencies: Vec<Handle>,
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
pub enum IoEvent {
    /// Read completed - need to return reader to its slot
    ReadComplete {
        handle: ChannelHandle,
        reader: Reader,
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
pub type IoFuture = Pin<Box<dyn Future<Output = IoEvent> + Send>>;

/// `SystemRuntime` manages all async I/O operations
/// Actors communicate with it via channels
pub struct SystemRuntime<K: KVBuffers> {
    /// Pool of output pipes (one per actor)
    pipe_pool: PipePool<K>,
    /// Global channel table: ChannelHandle → Channel (reader or writer endpoint)
    channels: HashMap<ChannelHandle, Channel>,
    /// Next channel handle ID
    next_channel_id: usize,
    /// Channel to send I/O requests to this runtime (None after `run()` starts)
    system_tx: Option<mpsc::UnboundedSender<IoRequest>>,
    /// Receives I/O requests from actors
    request_rx: mpsc::UnboundedReceiver<IoRequest>,
    /// ID generator for handles
    id_gen: Arc<IdGen>,
}

impl<K: KVBuffers> SystemRuntime<K> {
    pub fn new(kv: K, id_gen: Arc<IdGen>) -> Self {
        let (system_tx, request_rx) = mpsc::unbounded_channel();
        let notification_queue = NotificationQueueArc::new();

        Self {
            pipe_pool: PipePool::new(kv, notification_queue),
            channels: HashMap::new(),
            next_channel_id: 0,
            system_tx: Some(system_tx),
            request_rx,
            id_gen,
        }
    }

    /// Pre-open standard handles (stdin, stdout) for an actor just before it runs.
    /// When an actor has multiple dependencies, their outputs are merged sequentially.
    async fn preopen_std_handles(
        &mut self,
        node_handle: Handle,
        dependencies: &[Handle],
    ) -> StdHandles {
        debug!(actor = ?node_handle, "setting up std handles");

        // Pre-open stdin: check dependencies
        let stdin = if dependencies.len() > 1 {
            debug!(actor = ?node_handle, deps = dependencies.len(), "multiple dependencies, creating merge pipe");

            let merge_name = format!("pipes/merge-stdin-{}", node_handle.id());
            let merge_writer = self.pipe_pool.create_merge_writer(&merge_name, &self.id_gen).await;

            // Collect readers for all dependencies
            let mut dep_readers = Vec::new();
            for &dep_handle in dependencies {
                if !self.pipe_pool.has_pipe(dep_handle) {
                    let dep_pipe_name = format!("pipes/actor-{}", dep_handle.id());
                    self.pipe_pool
                        .create_output_pipe(dep_handle, &dep_pipe_name, &self.id_gen)
                        .await;
                    debug!(actor = ?node_handle, dependency = ?dep_handle, "created dependency output pipe");
                }
                let reader_handle = Handle::new(self.id_gen.get_next());
                let reader = self.pipe_pool.get_pipe(dep_handle).get_reader(reader_handle);
                dep_readers.push(reader);
            }

            // Create actor's stdin reader from the merge writer's shared data
            let merge_reader_handle = Handle::new(self.id_gen.get_next());
            let merge_reader = Reader::new(merge_reader_handle, merge_writer.share_with_reader());

            // Spawn background task: reads each dep reader sequentially, writes to merge writer.
            // When the task finishes, merge_writer is dropped, closing the pipe and signalling EOF.
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                for mut reader in dep_readers {
                    loop {
                        let n = reader.read(&mut buf).await;
                        if n <= 0 {
                            break;
                        }
                        #[allow(clippy::cast_sign_loss)]
                        merge_writer.write(&buf[..n as usize]);
                    }
                }
                // merge_writer dropped here → auto-closes → notifies merge_reader EOF
            });

            let channel_handle = self.alloc_channel_handle();
            self.channels.insert(channel_handle, Channel::Reader(Some(merge_reader)));
            trace!(actor = ?node_handle, channel = ?channel_handle, "merge stdin configured");
            channel_handle
        } else if let Some(&dep_handle) = dependencies.first() {
            debug!(actor = ?node_handle, dependency = ?dep_handle, "opening stdin from dependency");

            // Ensure the dependency's output pipe exists (create if needed)
            if !self.pipe_pool.has_pipe(dep_handle) {
                let dep_pipe_name = format!("pipes/actor-{}", dep_handle.id());
                self.pipe_pool
                    .create_output_pipe(dep_handle, &dep_pipe_name, &self.id_gen)
                    .await;
                debug!(actor = ?node_handle, dependency = ?dep_handle, "created dependency output pipe");
            }

            // Create reader for the dependency's pipe
            let pipe = self.pipe_pool.get_pipe(dep_handle);
            // Generate a unique handle for this reader
            let reader_handle = Handle::new(self.id_gen.get_next());
            let reader = pipe.get_reader(reader_handle);

            let channel_handle = self.alloc_channel_handle();
            self.channels.insert(channel_handle, Channel::Reader(Some(reader)));
            trace!(actor = ?node_handle, channel = ?channel_handle, "stdin configured");
            channel_handle
        } else {
            debug!(actor = ?node_handle, "no dependencies, creating empty stdin");

            // Create an empty/closed pipe to provide an empty reader
            let empty_pipe_name = format!("pipes/empty-stdin-{}", node_handle.id());
            let empty_handle = Handle::new(self.id_gen.get_next());
            self.pipe_pool.create_output_pipe(empty_handle, &empty_pipe_name, &self.id_gen).await;

            // Immediately close the writer to signal EOF
            let empty_pipe = self.pipe_pool.get_pipe(empty_handle);
            empty_pipe.writer().close();

            // Create reader from the closed pipe
            let reader_handle = Handle::new(self.id_gen.get_next());
            let reader = empty_pipe.get_reader(reader_handle);

            let channel_handle = self.alloc_channel_handle();
            self.channels.insert(channel_handle, Channel::Reader(Some(reader)));
            trace!(actor = ?node_handle, channel = ?channel_handle, "empty stdin configured");
            channel_handle
        };

        // Pre-open stdout: create output pipe
        debug!(actor = ?node_handle, "opening stdout");

        if !self.pipe_pool.has_pipe(node_handle) {
            let pipe_name = format!("pipes/actor-{}", node_handle.id());
            self.pipe_pool
                .create_output_pipe(node_handle, &pipe_name, &self.id_gen)
                .await;
            debug!(actor = ?node_handle, "created output pipe");
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
    fn handle_read(
        &mut self,
        handle: ChannelHandle,
        buffer: SendableBuffer,
        response: oneshot::Sender<c_int>,
    ) -> IoFuture {
        trace!(channel = ?handle, "processing Read");

        if let Some(Channel::Reader(reader_slot)) = self.channels.get_mut(&handle) {
            if let Some(mut reader) = reader_slot.take() {
                trace!(channel = ?handle, "spawning async read");
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
                Box::pin(async move {
                    IoEvent::SyncComplete {
                        result: 0,
                        response,
                    }
                })
            }
        } else {
            warn!(channel = ?handle, "channel not found or not a reader");
            Box::pin(async move {
                IoEvent::SyncComplete {
                    result: 0,
                    response,
                }
            })
        }
    }

    /// Handler for Write requests - writes to actor's pipe
    fn handle_write(
        &self,
        handle: ChannelHandle,
        data: &[u8],
        response: oneshot::Sender<c_int>,
    ) -> IoFuture {
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
        Box::pin(async move { IoEvent::SyncComplete { result, response } })
    }

    /// Handler for Close requests - uses ChannelHandle
    fn handle_close(
        &mut self,
        handle: ChannelHandle,
        response: oneshot::Sender<c_int>,
    ) -> IoFuture {
        trace!(channel = ?handle, "processing Close");
        let mut result = 0;

        if let Some(channel) = self.channels.remove(&handle) {
            match channel {
                Channel::Reader(_) => {
                    trace!(channel = ?handle, "closed reader");
                }
                Channel::Writer { node_handle } => {
                    let pipe = self.pipe_pool.get_pipe(node_handle);
                    pipe.writer().close();
                    if let Err(e) = self.pipe_pool.flush_buffer(node_handle) {
                        warn!(error = ?e, "failed to flush buffer");
                        result = -1;
                    }
                    trace!(channel = ?handle, "closed writer");
                }
            }
        } else {
            warn!(channel = ?handle, "channel not found");
        }

        Box::pin(async move {
            IoEvent::SyncComplete {
                result,
                response,
            }
        })
    }

    /// Handler for `ReadComplete` events
    fn handle_read_complete(
        &mut self,
        handle: ChannelHandle,
        reader: Reader,
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

        let mut pending_ops: FuturesUnordered<IoFuture> = FuturesUnordered::new();
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
                            IoRequest::SetupStdHandles { node_handle, dependencies, response } => {
                                let handles = self.preopen_std_handles(node_handle, &dependencies).await;
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
