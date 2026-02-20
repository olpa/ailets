mod scheduler;
mod sqlitekv;
mod stdin_source;
mod val;

use std::sync::Arc;

use actor_io::{AReader, AWriter};
use actor_runtime::{ActorRuntime, StdHandle};
use ailetos::dag::{Dag, DependsOn, For, NodeKind};
use ailetos::idgen::{Handle, IdGen};
use ailetos::notification_queue::NotificationQueueArc;
use ailetos::pipe::Reader;
use ailetos::pipepool::PipePool;
use ailetos::KVBuffers;
use futures::stream::{FuturesUnordered, StreamExt};
use scheduler::Scheduler;
use sqlitekv::SqliteKV;
use std::collections::HashMap;
use std::future::Future;
use std::os::raw::c_int;
use std::pin::Pin;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, trace, info, warn};

/// Global unique identifier for a pipe endpoint (reader or writer)
/// Used by SystemRuntime to identify channels across all actors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ChannelHandle(usize);

/// A channel endpoint - either a reader or writer
enum Channel {
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
struct SendableBuffer {
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
    unsafe fn new(buffer: &mut [u8]) -> Self {
        Self {
            ptr: std::ptr::from_mut::<[u8]>(buffer),
            #[cfg(debug_assertions)]
            consumed: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Consume the `SendableBuffer` and return the raw pointer.
    /// This prevents accidental reuse of the same buffer.
    fn into_raw(self) -> *mut [u8] {
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
struct StdHandles {
    stdin: ChannelHandle,
    stdout: ChannelHandle,
}

/// I/O requests sent from `ActorRuntime` to `SystemRuntime`
enum IoRequest {
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
enum IoEvent {
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
type IoFuture = Pin<Box<dyn Future<Output = IoEvent> + Send>>;

/// `SystemRuntime` manages all async I/O operations
/// Actors communicate with it via channels
struct SystemRuntime<K: KVBuffers> {
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
    fn new(kv: K, id_gen: Arc<IdGen>) -> Self {
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
    /// Expects at most one dependency for stdin, otherwise panics.
    async fn preopen_std_handles(
        &mut self,
        node_handle: Handle,
        dependencies: &[Handle],
    ) -> StdHandles {
        debug!(actor = ?node_handle, "setting up std handles");

        // Pre-open stdin: check dependencies
        if dependencies.len() > 1 {
            panic!(
                "Actor {:?} has {} dependencies, expected at most 1 for stdin",
                node_handle,
                dependencies.len()
            );
        }

        let stdin = if let Some(&dep_handle) = dependencies.first() {
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
    fn get_system_tx(&self) -> mpsc::UnboundedSender<IoRequest> {
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
    async fn run(mut self) {
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
struct FdTable {
    /// fd → ChannelHandle mapping
    table: HashMap<c_int, ChannelHandle>,
    /// Next fd to allocate
    next_fd: c_int,
}

impl FdTable {
    fn new() -> Self {
        Self {
            table: HashMap::new(),
            next_fd: 0,
        }
    }

    /// Allocate a new fd and associate it with a ChannelHandle
    fn insert(&mut self, handle: ChannelHandle) -> c_int {
        let fd = self.next_fd;
        self.next_fd += 1;
        self.table.insert(fd, handle);
        fd
    }

    /// Look up the ChannelHandle for a given fd
    fn get(&self, fd: c_int) -> Option<ChannelHandle> {
        self.table.get(&fd).copied()
    }

    /// Remove an fd mapping
    fn remove(&mut self, fd: c_int) -> Option<ChannelHandle> {
        self.table.remove(&fd)
    }
}

/// Stub `ActorRuntime` implementation for CLI testing
/// Acts as a pure proxy to `SystemRuntime` for all I/O operations
/// Provides sync-to-async adapters (blocking on async operations)
/// Maintains a per-actor fd table for POSIX-style fd semantics
pub struct StubActorRuntime {
    /// This actor's node handle (used as actor identifier)
    node_handle: Handle,
    /// Channel to send async I/O requests to `SystemRuntime`
    system_tx: mpsc::UnboundedSender<IoRequest>,
    /// Per-actor fd table (POSIX fd → global ChannelHandle)
    fd_table: std::sync::Mutex<FdTable>,
}

impl Clone for StubActorRuntime {
    fn clone(&self) -> Self {
        Self {
            node_handle: self.node_handle,
            system_tx: self.system_tx.clone(),
            fd_table: std::sync::Mutex::new(FdTable::new()),
        }
    }
}

impl StubActorRuntime {
    /// Create a new `ActorRuntime` for the given node handle
    fn new(node_handle: Handle, system_tx: mpsc::UnboundedSender<IoRequest>) -> Self {
        Self {
            node_handle,
            system_tx,
            fd_table: std::sync::Mutex::new(FdTable::new()),
        }
    }

    /// Request SystemRuntime to set up standard handles before actor starts.
    /// This pre-opens stdin (fd 0) and stdout (fd 1) with the correct channel handles.
    /// Dependencies must be provided from the DAG.
    #[allow(clippy::unwrap_used)]
    pub fn request_std_handles_setup(&self, dependencies: Vec<Handle>) {
        trace!(actor = ?self.node_handle, deps = ?dependencies, "requesting std handles setup");

        // Send request to SystemRuntime and block for response
        let (tx, rx) = oneshot::channel();

        self.system_tx
            .send(IoRequest::SetupStdHandles {
                node_handle: self.node_handle,
                dependencies,
                response: tx,
            })
            .unwrap();

        let std_handles = rx.blocking_recv().unwrap();

        // Map the pre-opened channel handles to fd 0 (stdin) and fd 1 (stdout)
        {
            let mut table = self.fd_table.lock().unwrap();

            // Insert stdin as fd 0
            let stdin_fd = table.insert(std_handles.stdin);
            assert_eq!(stdin_fd, 0, "stdin should be fd 0");

            // Insert stdout as fd 1
            let stdout_fd = table.insert(std_handles.stdout);
            assert_eq!(stdout_fd, 1, "stdout should be fd 1");
        }

        trace!(actor = ?self.node_handle, "std handles ready (stdin=0, stdout=1)");
    }

    /// Close all open handles when actor finishes.
    /// Closes in reverse order (highest fd first) to handle any dependencies.
    #[allow(clippy::unwrap_used)]
    pub fn close_all_handles(&self) {
        trace!(actor = ?self.node_handle, "close_all_handles");

        // Get all open fds
        let fds: Vec<c_int> = {
            let table = self.fd_table.lock().unwrap();
            table.table.keys().copied().collect()
        };

        // Close in reverse order
        let mut fds = fds;
        fds.sort();
        fds.reverse();

        for fd in fds {
            trace!(actor = ?self.node_handle, fd = fd, "closing fd");
            let _ = self.aclose(fd);
        }

        trace!(actor = ?self.node_handle, "all handles closed");
    }
}

#[allow(clippy::unwrap_used)] // Stub implementation for testing - panics are acceptable
impl ActorRuntime for StubActorRuntime {
    fn get_errno(&self) -> c_int {
        trace!(actor = ?self.node_handle, "get_errno");
        0 // No error
    }

    fn open_read(&self, _name: &str) -> c_int {
        trace!(actor = ?self.node_handle, "open_read");
        // Send request to SystemRuntime and block for response
        let (tx, rx) = oneshot::channel();

        self.system_tx
            .send(IoRequest::OpenRead {
                node_handle: self.node_handle,
                response: tx,
            })
            .unwrap();

        trace!(actor = ?self.node_handle, "open_read: blocking_recv");
        let channel_handle = rx.blocking_recv().unwrap();

        // Allocate local fd and map to global channel handle
        let fd = self.fd_table.lock().unwrap().insert(channel_handle);
        trace!(actor = ?self.node_handle, fd = fd, channel = ?channel_handle, "open_read done");
        fd
    }

    fn open_write(&self, _name: &str) -> c_int {
        trace!(actor = ?self.node_handle, "open_write");
        // Send request to SystemRuntime and block for response
        let (tx, rx) = oneshot::channel();

        self.system_tx
            .send(IoRequest::OpenWrite {
                node_handle: self.node_handle,
                response: tx,
            })
            .unwrap();

        trace!(actor = ?self.node_handle, "open_write: blocking_recv");
        let channel_handle = rx.blocking_recv().unwrap();

        // Allocate local fd and map to global channel handle
        let fd = self.fd_table.lock().unwrap().insert(channel_handle);
        trace!(actor = ?self.node_handle, fd = fd, channel = ?channel_handle, "open_write done");
        fd
    }

    fn aread(&self, fd: c_int, buffer: &mut [u8]) -> c_int {
        trace!(actor = ?self.node_handle, fd = fd, buflen = buffer.len(), "aread");

        // Look up the channel handle for this fd
        let channel_handle = match self.fd_table.lock().unwrap().get(fd) {
            Some(h) => h,
            None => {
                warn!(actor = ?self.node_handle, fd = fd, "aread: fd not found");
                return -1;
            }
        };

        // Send request to SystemRuntime and block for response
        let (tx, rx) = oneshot::channel();

        // SAFETY: We're passing a raw pointer to our buffer and will block until
        // the handler finishes using it. The buffer remains valid because:
        // 1. Our stack frame stays alive (we block via blocking_recv)
        // 2. Only the handler accesses the buffer while we're blocked
        // 3. The channel ensures happens-before ordering
        // 4. The SendableBuffer is consumed exactly once in the handler
        let buffer_ptr = unsafe { SendableBuffer::new(buffer) };

        self.system_tx
            .send(IoRequest::Read {
                handle: channel_handle,
                buffer: buffer_ptr,
                response: tx,
            })
            .unwrap();

        // Block waiting for SystemRuntime to complete the async read
        trace!(actor = ?self.node_handle, "aread: blocking_recv");
        let bytes_read = rx.blocking_recv().unwrap();
        trace!(actor = ?self.node_handle, bytes = bytes_read, "aread done");

        bytes_read
    }

    fn awrite(&self, fd: c_int, buffer: &[u8]) -> c_int {
        trace!(actor = ?self.node_handle, fd = fd, buflen = buffer.len(), "awrite");

        // Look up the channel handle for this fd
        let channel_handle = match self.fd_table.lock().unwrap().get(fd) {
            Some(h) => h,
            None => {
                warn!(actor = ?self.node_handle, fd = fd, "awrite: fd not found");
                return -1;
            }
        };

        // Send request to SystemRuntime and block for response
        let (tx, rx) = oneshot::channel();

        self.system_tx
            .send(IoRequest::Write {
                handle: channel_handle,
                data: buffer.to_vec(),
                response: tx,
            })
            .unwrap();

        trace!(actor = ?self.node_handle, "awrite: blocking_recv");
        let result = rx.blocking_recv().unwrap();
        trace!(actor = ?self.node_handle, result = result, "awrite done");
        result
    }

    fn aclose(&self, fd: c_int) -> c_int {
        trace!(actor = ?self.node_handle, fd = fd, "aclose");

        // Look up and remove the channel handle for this fd
        let channel_handle = match self.fd_table.lock().unwrap().remove(fd) {
            Some(h) => h,
            None => {
                warn!(actor = ?self.node_handle, fd = fd, "aclose: fd not found");
                return -1;
            }
        };

        // Send request to SystemRuntime and block for response
        let (tx, rx) = oneshot::channel();

        self.system_tx
            .send(IoRequest::Close {
                handle: channel_handle,
                response: tx,
            })
            .unwrap();

        trace!(actor = ?self.node_handle, "aclose: blocking_recv");
        let result = rx.blocking_recv().unwrap();
        trace!(actor = ?self.node_handle, result = result, "aclose done");
        result
    }
}

/// Spawn actor tasks for each node in the system
fn spawn_actor_tasks(
    dag: &Dag,
    target: Handle,
    system_tx: mpsc::UnboundedSender<IoRequest>,
) -> Vec<tokio::task::JoinHandle<()>> {
    let scheduler = Scheduler::new(dag, target);
    let mut tasks = Vec::new();

    for node_handle in scheduler.iter() {
        let node = dag.get_node(node_handle).expect("node exists");
        let idname = node.idname.clone();
        debug!(node = ?node_handle, name = %idname, "spawning actor task");

        // Get dependencies for this node
        let dependencies: Vec<Handle> = dag.get_direct_dependencies(node_handle).collect();

        // Create runtime for this actor
        let runtime = StubActorRuntime::new(node_handle, system_tx.clone());

        let task = tokio::task::spawn_blocking(move || {
            debug!(node = ?node_handle, name = %idname, "task starting");

            // Request SystemRuntime to setup std handles before actor runs
            runtime.request_std_handles_setup(dependencies);

            // Create reader and writer unconditionally
            let areader = AReader::new_from_std(&runtime, StdHandle::Stdin);
            let awriter = AWriter::new_from_std(&runtime, StdHandle::Stdout);

            // Execute the appropriate actor based on idname
            let result = match idname.as_str() {
                "val" => val::execute(areader, awriter),
                "stdin" => stdin_source::execute(areader, awriter),
                _ => cat::execute(areader, awriter),
            };

            match result {
                Ok(()) => debug!(node = ?node_handle, name = %idname, "task completed"),
                Err(e) => warn!(node = ?node_handle, name = %idname, error = %e, "task error"),
            }

            // Close all handles after actor finishes
            runtime.close_all_handles();

            debug!(node = ?node_handle, name = %idname, "task done");
        });
        tasks.push(task);
    }

    tasks
}

/// Run the system: spawn system runtime and actor tasks, wait for completion
async fn run_system<K: KVBuffers + 'static>(
    system_runtime: SystemRuntime<K>,
    dag: &Dag,
    target: Handle,
) {
    // Get sender before moving system_runtime
    let system_tx = system_runtime.get_system_tx();

    // Spawn SystemRuntime task
    let system_task = tokio::spawn(async move {
        system_runtime.run().await;
    });

    // Spawn actor tasks
    let actor_tasks = spawn_actor_tasks(dag, target, system_tx);

    // Wait for system runtime
    if let Err(e) = system_task.await {
        warn!(error = %e, "SystemRuntime task failed");
    }

    // Wait for all actor tasks
    for task in actor_tasks {
        if let Err(e) = task.await {
            warn!(error = %e, "actor task failed");
        }
    }
}

fn build_flow(dag: &mut Dag) -> Handle {
    // val: value node (pre-filled with "(mee too)")
    let val = dag.add_node("val".into(), NodeKind::Concrete);

    // stdin: reads from stdin
    // TODO: implement actual OS stdin reading, currently simulated with pre-filled pipe
    let stdin = dag.add_node("stdin".into(), NodeKind::Concrete);

    // foo: copies from stdin
    let foo = dag.add_node("foo".into(), NodeKind::Concrete);
    dag.add_dependency(For(foo), DependsOn(stdin));

    // bar: copies from val
    // TODO: bar should also depend on foo, but multiple inputs are not yet supported.
    // For now, we only read from val and ignore foo.
    let bar = dag.add_node("bar".into(), NodeKind::Concrete);
    dag.add_dependency(For(bar), DependsOn(val));

    // baz: copies from bar
    let baz = dag.add_node("baz".into(), NodeKind::Concrete);
    dag.add_dependency(For(baz), DependsOn(bar));

    // .end alias to baz
    let end = dag.add_node(".end".into(), NodeKind::Alias);
    dag.add_dependency(For(end), DependsOn(baz));

    end
}

#[tokio::main]
async fn main() {
    // Initialize tracing subscriber
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
        )
        .init();

    // Create DAG and build flow
    let idgen = Arc::new(IdGen::new());
    let mut dag = Dag::new(Arc::clone(&idgen));
    let end_node = build_flow(&mut dag);

    // Print dependency tree
    info!("Dependency tree:\n{}", dag.dump(end_node));

    // Create key-value store for pipe buffers
    let _ = std::fs::remove_file("example.db");
    let kv = SqliteKV::new("example.db").expect("Failed to create SqliteKV");

    // Create system runtime
    let system_runtime = SystemRuntime::new(kv, idgen);

    // Run the system
    run_system(system_runtime, &dag, end_node).await;
}
