mod scheduler;
mod sqlitekv;

use std::sync::Arc;

use actor_io::{AReader, AWriter};
use actor_runtime::{ActorRuntime, StdHandle};
use ailetos::dag::{Dag, DependsOn, For, NodeKind};
use ailetos::idgen::{Handle, IdGen};
use ailetos::notification_queue::NotificationQueueArc;
use ailetos::pipe::Reader;
use ailetos::pipepool::PipePool;
use ailetos::KVBuffers;
use scheduler::Scheduler;
use futures::stream::{FuturesUnordered, StreamExt};
use sqlitekv::SqliteKV;
use std::collections::HashMap;
use std::future::Future;
use std::io::Write as StdWrite;
use std::os::raw::c_int;
use std::pin::Pin;
use tokio::sync::{mpsc, oneshot};

/// Unique identifier for actors in the system
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ActorId(usize);

/// Global unique identifier for a pipe endpoint (reader or writer)
/// Used by SystemRuntime to identify channels across all actors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ChannelHandle(usize);

/// A channel endpoint - either a reader or writer
enum Channel {
    /// Reader channel - holds the Reader (None when in use during async read)
    Reader(Option<Reader>),
    /// Writer channel - holds the actor's output destination info
    Writer { actor_id: ActorId },
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

/// I/O requests sent from `ActorRuntime` to `SystemRuntime`
enum IoRequest {
    /// Open a stream for reading (returns global ChannelHandle)
    OpenRead {
        actor_id: ActorId,
        response: oneshot::Sender<ChannelHandle>,
    },
    /// Open a stream for writing (returns global ChannelHandle)
    OpenWrite {
        actor_id: ActorId,
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

/// Output destination configuration for an actor
enum ActorOutputDestination {
    /// Write to stdout
    Stdout,
    /// Write to own output pipe
    Pipe,
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
    /// Input pipe handle for each actor (dependency's output pipe)
    actor_input_pipes: HashMap<ActorId, Handle>,
    /// Output configuration for each actor
    actor_outputs: HashMap<ActorId, ActorOutputDestination>,
    /// Maps ActorId to the node Handle (for pipe_pool lookups)
    actor_handles: HashMap<ActorId, Handle>,
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
            actor_input_pipes: HashMap::new(),
            actor_outputs: HashMap::new(),
            actor_handles: HashMap::new(),
            system_tx: Some(system_tx),
            request_rx,
            id_gen,
        }
    }

    /// Allocate a new global channel handle
    fn alloc_channel_handle(&mut self) -> ChannelHandle {
        let handle = ChannelHandle(self.next_channel_id);
        self.next_channel_id += 1;
        handle
    }

    /// Factory method to create an `ActorRuntime` for a specific actor
    #[allow(clippy::expect_used)] // Called before run(), system_tx is always Some
    fn create_actor_runtime(&self, actor_id: ActorId) -> StubActorRuntime {
        StubActorRuntime::new(
            actor_id,
            self.system_tx.as_ref().expect("system_tx taken").clone(),
        )
    }

    /// Setup pipes and wiring based on DAG dependencies
    /// Each node gets an output pipe, input pipe handles are recorded for later channel creation
    async fn setup_pipes_from_dag(&mut self, dag: &Dag, target: Handle) {
        // Resolve the alias to get the actual target (concrete node)
        let actual_target: Handle = dag
            .resolve_dependencies(target)
            .next()
            .unwrap_or(target);

        // Create output pipe for each concrete node
        let scheduler = Scheduler::new(dag, target);
        for node_handle in scheduler.iter() {
            let node = dag.get_node(node_handle).expect("node exists");
            let pipe_name = format!("pipes/{}-{}", node.idname, node_handle.id());

            #[allow(clippy::cast_sign_loss)]
            let actor_id = ActorId(node_handle.id() as usize);

            // Create output pipe for this actor
            self.pipe_pool
                .create_output_pipe(node_handle, &pipe_name, &self.id_gen)
                .await;
            self.actor_handles.insert(actor_id, node_handle);

            // Wire output: target writes to stdout, others write to their pipe
            if node_handle == actual_target {
                self.actor_outputs
                    .insert(actor_id, ActorOutputDestination::Stdout);
            } else {
                self.actor_outputs
                    .insert(actor_id, ActorOutputDestination::Pipe);
            }

            // Record input pipe handle from first dependency
            // Actual reader creation happens in handle_open_read when actor calls open_read
            // TODO: support multiple dependencies - currently only reads from first dep
            let deps: Vec<Handle> = dag.resolve_dependencies(node_handle).collect();
            if let Some(&dep_handle) = deps.first() {
                self.actor_input_pipes.insert(actor_id, dep_handle);
            }

            // Pre-fill special nodes
            match node.idname.as_str() {
                "val" => {
                    // Value node: pre-fill with static data
                    let data = b"(mee too)";
                    let pipe = self.pipe_pool.get_pipe(node_handle);
                    let _ = pipe.writer().write(data);
                    pipe.writer().close();
                }
                "stdin" => {
                    // TODO: implement actual OS stdin reading
                    // For now, pre-fill with simulated stdin data
                    let data = b"simulated stdin\n";
                    let pipe = self.pipe_pool.get_pipe(node_handle);
                    let _ = pipe.writer().write(data);
                    pipe.writer().close();
                }
                _ => {}
            }
        }
    }

    /// Handler for `OpenRead` requests - creates reader and returns ChannelHandle
    fn handle_open_read(&mut self, actor_id: ActorId, response: oneshot::Sender<ChannelHandle>) {
        eprintln!("[SystemRuntime] Processing OpenRead for {actor_id:?}");

        if let Some(&dep_handle) = self.actor_input_pipes.get(&actor_id) {
            // Create reader for the dependency's output pipe
            let reader = self.pipe_pool.open_reader(dep_handle, &self.id_gen);
            let channel_handle = self.alloc_channel_handle();
            self.channels.insert(channel_handle, Channel::Reader(Some(reader)));
            eprintln!("[SystemRuntime] OpenRead {actor_id:?}: created {channel_handle:?} for dep {dep_handle:?}");
            let _ = response.send(channel_handle);
        } else {
            eprintln!("[SystemRuntime] OpenRead {actor_id:?}: no input pipe configured");
            // Return a dummy handle - reads will fail
            let channel_handle = self.alloc_channel_handle();
            let _ = response.send(channel_handle);
        }
    }

    /// Handler for `OpenWrite` requests - returns ChannelHandle for actor's output
    fn handle_open_write(&mut self, actor_id: ActorId, response: oneshot::Sender<ChannelHandle>) {
        eprintln!("[SystemRuntime] Processing OpenWrite for {actor_id:?}");

        let channel_handle = self.alloc_channel_handle();
        // Store the actor_id so we can look up output destination in handle_write
        self.channels.insert(channel_handle, Channel::Writer { actor_id });
        eprintln!("[SystemRuntime] OpenWrite {actor_id:?}: created {channel_handle:?}");
        let _ = response.send(channel_handle);
    }

    /// Handler for Read requests - uses ChannelHandle to find reader
    fn handle_read(
        &mut self,
        handle: ChannelHandle,
        buffer: SendableBuffer,
        response: oneshot::Sender<c_int>,
    ) -> IoFuture {
        eprintln!("[SystemRuntime] Processing Read for {handle:?}");

        if let Some(Channel::Reader(reader_slot)) = self.channels.get_mut(&handle) {
            if let Some(mut reader) = reader_slot.take() {
                eprintln!("[SystemRuntime] Read {handle:?}: spawning async read");
                Box::pin(async move {
                    // SAFETY: Buffer remains valid because aread() blocks until response
                    let buf = unsafe { &mut *buffer.into_raw() };
                    let bytes_read = reader.read(buf).await;
                    eprintln!("[SystemRuntime] Read for {handle:?} completed: {bytes_read} bytes");
                    IoEvent::ReadComplete {
                        handle,
                        reader,
                        bytes_read,
                        response,
                    }
                })
            } else {
                eprintln!("[SystemRuntime] Read {handle:?}: reader not available (already in use?)");
                Box::pin(async move {
                    IoEvent::SyncComplete {
                        result: 0,
                        response,
                    }
                })
            }
        } else {
            eprintln!("[SystemRuntime] Read {handle:?}: channel not found or not a reader");
            Box::pin(async move {
                IoEvent::SyncComplete {
                    result: 0,
                    response,
                }
            })
        }
    }

    /// Handler for Write requests - uses ChannelHandle to find writer
    fn handle_write(
        &self,
        handle: ChannelHandle,
        data: &[u8],
        response: oneshot::Sender<c_int>,
    ) -> IoFuture {
        eprintln!(
            "[SystemRuntime] Processing Write for {handle:?}, {} bytes",
            data.len()
        );

        let result = if let Some(Channel::Writer { actor_id }) = self.channels.get(&handle) {
            let actor_id = *actor_id;
            if let Some(output_dest) = self.actor_outputs.get(&actor_id) {
                match output_dest {
                    ActorOutputDestination::Stdout => {
                        eprintln!("[SystemRuntime] Write {handle:?}: writing to stdout");
                        let mut stdout = std::io::stdout();
                        match stdout.write(data) {
                            Ok(n) => {
                                if stdout.flush().is_err() {
                                    -1
                                } else {
                                    #[allow(
                                        clippy::cast_possible_truncation,
                                        clippy::cast_possible_wrap
                                    )]
                                    {
                                        n as c_int
                                    }
                                }
                            }
                            Err(_) => -1,
                        }
                    }
                    ActorOutputDestination::Pipe => {
                        if let Some(&actor_handle) = self.actor_handles.get(&actor_id) {
                            eprintln!("[SystemRuntime] Write {handle:?}: writing to pipe");
                            let pipe = self.pipe_pool.get_pipe(actor_handle);
                            let n = pipe.writer().write(data);
                            eprintln!("[SystemRuntime] Write {handle:?}: pipe write returned {n}");
                            #[allow(clippy::cast_possible_truncation)]
                            {
                                n as c_int
                            }
                        } else {
                            eprintln!("[SystemRuntime] Write {handle:?}: actor handle not found");
                            -1
                        }
                    }
                }
            } else {
                eprintln!("[SystemRuntime] Write {handle:?}: no output destination for actor");
                -1
            }
        } else {
            eprintln!("[SystemRuntime] Write {handle:?}: channel not found or not a writer");
            -1
        };
        eprintln!("[SystemRuntime] Write {handle:?} completed");
        Box::pin(async move { IoEvent::SyncComplete { result, response } })
    }

    /// Handler for Close requests - uses ChannelHandle
    fn handle_close(&mut self, handle: ChannelHandle, response: oneshot::Sender<c_int>) -> IoFuture {
        eprintln!("[SystemRuntime] Processing Close for {handle:?}");
        let mut result = 0;

        if let Some(channel) = self.channels.remove(&handle) {
            match channel {
                Channel::Reader(_) => {
                    eprintln!("[SystemRuntime] Close {handle:?}: closed reader");
                }
                Channel::Writer { actor_id } => {
                    if let Some(ActorOutputDestination::Pipe) = self.actor_outputs.get(&actor_id) {
                        if let Some(&actor_handle) = self.actor_handles.get(&actor_id) {
                            let pipe = self.pipe_pool.get_pipe(actor_handle);
                            pipe.writer().close();
                            if let Err(e) = self.pipe_pool.flush_buffer(actor_handle) {
                                eprintln!("[SystemRuntime] Failed to flush buffer: {e}");
                                result = -1;
                            }
                        }
                    }
                    eprintln!("[SystemRuntime] Close {handle:?}: closed writer");
                }
            }
        } else {
            eprintln!("[SystemRuntime] Close {handle:?}: channel not found");
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
        eprintln!("[SystemRuntime] Read completed for {handle:?}, {bytes_read} bytes");
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

    /// Prepare actor runtimes for all nodes in the DAG
    fn prepare_actors(&self, dag: &Dag, target: Handle) -> Vec<(ActorId, String, StubActorRuntime)> {
        let scheduler = Scheduler::new(dag, target);
        let mut actor_infos = Vec::new();

        for node_handle in scheduler.iter() {
            let node = dag.get_node(node_handle).expect("node exists");
            let idname = node.idname.clone();
            eprintln!("Node to build: {:?} ({})", node_handle, idname);
            #[allow(clippy::cast_sign_loss)]
            let actor_id = ActorId(node_handle.id() as usize);
            let runtime = self.create_actor_runtime(actor_id);
            actor_infos.push((actor_id, idname, runtime));
        }

        actor_infos
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
                eprintln!("[SystemRuntime] No more work, exiting");
                break;
            }

            tokio::select! {
                // Handle new requests from actors
                request = self.request_rx.recv(), if request_rx_open => {
                    if let Some(request) = request {
                        eprintln!("[SystemRuntime] Received request");
                        match request {
                            IoRequest::OpenRead { actor_id, response } => {
                                // Handled synchronously - creates channel and responds
                                self.handle_open_read(actor_id, response);
                            }
                            IoRequest::OpenWrite { actor_id, response } => {
                                // Handled synchronously - creates channel and responds
                                self.handle_open_write(actor_id, response);
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
                        eprintln!("[SystemRuntime] Request channel closed");
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
    /// This actor's unique identifier
    actor_id: ActorId,
    /// Channel to send async I/O requests to `SystemRuntime`
    system_tx: mpsc::UnboundedSender<IoRequest>,
    /// Per-actor fd table (POSIX fd → global ChannelHandle)
    fd_table: std::sync::Mutex<FdTable>,
}

impl Clone for StubActorRuntime {
    fn clone(&self) -> Self {
        Self {
            actor_id: self.actor_id,
            system_tx: self.system_tx.clone(),
            fd_table: std::sync::Mutex::new(FdTable::new()),
        }
    }
}

impl StubActorRuntime {
    /// Create a new `ActorRuntime` for the given actor ID
    fn new(actor_id: ActorId, system_tx: mpsc::UnboundedSender<IoRequest>) -> Self {
        Self {
            actor_id,
            system_tx,
            fd_table: std::sync::Mutex::new(FdTable::new()),
        }
    }
}

#[allow(clippy::unwrap_used)] // Stub implementation for testing - panics are acceptable
impl ActorRuntime for StubActorRuntime {
    fn get_errno(&self) -> c_int {
        eprintln!(
            "[StubActorRuntime] Actor {:?}: get_errno() entry",
            self.actor_id
        );
        0 // No error
    }

    fn open_read(&self, _name: &str) -> c_int {
        eprintln!(
            "[StubActorRuntime] Actor {:?}: open_read() entry",
            self.actor_id
        );
        // Send request to SystemRuntime and block for response
        let (tx, rx) = oneshot::channel();

        self.system_tx
            .send(IoRequest::OpenRead {
                actor_id: self.actor_id,
                response: tx,
            })
            .unwrap();

        eprintln!(
            "[StubActorRuntime] Actor {:?}: open_read() before blocking_recv",
            self.actor_id
        );
        let channel_handle = rx.blocking_recv().unwrap();

        // Allocate local fd and map to global channel handle
        let fd = self.fd_table.lock().unwrap().insert(channel_handle);
        eprintln!(
            "[StubActorRuntime] Actor {:?}: open_read() fd={} -> {:?}",
            self.actor_id, fd, channel_handle
        );
        fd
    }

    fn open_write(&self, _name: &str) -> c_int {
        eprintln!(
            "[StubActorRuntime] Actor {:?}: open_write() entry",
            self.actor_id
        );
        // Send request to SystemRuntime and block for response
        let (tx, rx) = oneshot::channel();

        self.system_tx
            .send(IoRequest::OpenWrite {
                actor_id: self.actor_id,
                response: tx,
            })
            .unwrap();

        eprintln!(
            "[StubActorRuntime] Actor {:?}: open_write() before blocking_recv",
            self.actor_id
        );
        let channel_handle = rx.blocking_recv().unwrap();

        // Allocate local fd and map to global channel handle
        let fd = self.fd_table.lock().unwrap().insert(channel_handle);
        eprintln!(
            "[StubActorRuntime] Actor {:?}: open_write() fd={} -> {:?}",
            self.actor_id, fd, channel_handle
        );
        fd
    }

    fn aread(&self, fd: c_int, buffer: &mut [u8]) -> c_int {
        eprintln!(
            "[StubActorRuntime] Actor {:?}: aread(fd={}) entry, buffer.len={}",
            self.actor_id, fd, buffer.len()
        );

        // Look up the channel handle for this fd
        let channel_handle = match self.fd_table.lock().unwrap().get(fd) {
            Some(h) => h,
            None => {
                eprintln!(
                    "[StubActorRuntime] Actor {:?}: aread() fd={} not found",
                    self.actor_id, fd
                );
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
        eprintln!(
            "[StubActorRuntime] Actor {:?}: aread() before blocking_recv",
            self.actor_id
        );
        let bytes_read = rx.blocking_recv().unwrap();
        eprintln!(
            "[StubActorRuntime] Actor {:?}: aread() after blocking_recv, bytes_read={}",
            self.actor_id, bytes_read
        );

        bytes_read
    }

    fn awrite(&self, fd: c_int, buffer: &[u8]) -> c_int {
        eprintln!(
            "[StubActorRuntime] Actor {:?}: awrite(fd={}) entry, buffer.len={}",
            self.actor_id, fd, buffer.len()
        );

        // Look up the channel handle for this fd
        let channel_handle = match self.fd_table.lock().unwrap().get(fd) {
            Some(h) => h,
            None => {
                eprintln!(
                    "[StubActorRuntime] Actor {:?}: awrite() fd={} not found",
                    self.actor_id, fd
                );
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

        eprintln!(
            "[StubActorRuntime] Actor {:?}: awrite() before blocking_recv",
            self.actor_id
        );
        let result = rx.blocking_recv().unwrap();
        eprintln!(
            "[StubActorRuntime] Actor {:?}: awrite() after blocking_recv, result={}",
            self.actor_id, result
        );
        result
    }

    fn aclose(&self, fd: c_int) -> c_int {
        eprintln!(
            "[StubActorRuntime] Actor {:?}: aclose() entry, fd={}",
            self.actor_id, fd
        );

        // Look up and remove the channel handle for this fd
        let channel_handle = match self.fd_table.lock().unwrap().remove(fd) {
            Some(h) => h,
            None => {
                eprintln!(
                    "[StubActorRuntime] Actor {:?}: aclose() fd={} not found",
                    self.actor_id, fd
                );
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

        eprintln!(
            "[StubActorRuntime] Actor {:?}: aclose() before blocking_recv",
            self.actor_id
        );
        let result = rx.blocking_recv().unwrap();
        eprintln!(
            "[StubActorRuntime] Actor {:?}: aclose() after blocking_recv, result={}",
            self.actor_id, result
        );
        result
    }
}

/// Spawn actor tasks for each node in the system
fn spawn_actor_tasks(
    actor_infos: Vec<(ActorId, String, StubActorRuntime)>,
) -> Vec<tokio::task::JoinHandle<()>> {
    let mut tasks = Vec::new();

    for (actor_id, idname, runtime) in actor_infos {
        let task = tokio::task::spawn_blocking(move || {
            eprintln!("Task {:?} ({}): Starting", actor_id, idname);

            match idname.as_str() {
                "val" => {
                    // Value node: data is pre-filled, nothing to do
                    eprintln!("Task {:?} ({}): Value node, skipping", actor_id, idname);
                }
                "stdin" => {
                    // TODO: implement actual OS stdin reading
                    // For now, data is pre-filled in the pipe, just copy to output
                    let areader = AReader::new_from_std(&runtime, StdHandle::Stdin);
                    let awriter = AWriter::new_from_std(&runtime, StdHandle::Stdout);
                    match cat::execute(areader, awriter) {
                        Ok(()) => eprintln!("Task {:?} ({}): completed", actor_id, idname),
                        Err(e) => eprintln!("Error in {:?} ({}): {e}", actor_id, idname),
                    }
                }
                _ => {
                    // Default: cat actor (copy from stdin to stdout)
                    let areader = AReader::new_from_std(&runtime, StdHandle::Stdin);
                    let awriter = AWriter::new_from_std(&runtime, StdHandle::Stdout);
                    match cat::execute(areader, awriter) {
                        Ok(()) => eprintln!("Task {:?} ({}): completed", actor_id, idname),
                        Err(e) => eprintln!("Error in {:?} ({}): {e}", actor_id, idname),
                    }
                }
            }
            eprintln!("Task {:?} ({}): Done", actor_id, idname);
        });
        tasks.push(task);
    }

    tasks
}

/// Run the system: spawn system runtime and actor tasks, wait for completion
async fn run_system<K: KVBuffers + 'static>(
    system_runtime: SystemRuntime<K>,
    actor_infos: Vec<(ActorId, String, StubActorRuntime)>,
) {
    // Spawn SystemRuntime task
    let system_task = tokio::spawn(async move {
        system_runtime.run().await;
    });

    // Spawn actor tasks
    let actor_tasks = spawn_actor_tasks(actor_infos);

    // Wait for system runtime
    if let Err(e) = system_task.await {
        eprintln!("SystemRuntime task failed: {e}");
    }

    // Wait for all actor tasks
    for task in actor_tasks {
        if let Err(e) = task.await {
            eprintln!("Task failed: {e}");
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
    // Create DAG and build flow
    let idgen = Arc::new(IdGen::new());
    let mut dag = Dag::new(Arc::clone(&idgen));
    let end_node = build_flow(&mut dag);

    // Print dependency tree
    eprintln!("Dependency tree:\n{}", dag.dump(end_node));

    // Create key-value store for pipe buffers
    let _ = std::fs::remove_file("example.db");
    let kv = SqliteKV::new("example.db").expect("Failed to create SqliteKV");

    // Create system runtime
    let mut system_runtime = SystemRuntime::new(kv, idgen);

    // Prepare actor runtimes
    let actor_infos = system_runtime.prepare_actors(&dag, end_node);

    // Setup pipes based on DAG dependencies
    eprintln!("Setup: Setting up pipes based on DAG");
    system_runtime.setup_pipes_from_dag(&dag, end_node).await;
    eprintln!("Setup: All pipes configured");

    // Run the system
    run_system(system_runtime, actor_infos).await;
}
