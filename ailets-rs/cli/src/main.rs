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
        eprintln!("[SystemRuntime] Setting up std handles for actor {:?}", node_handle);

        // Pre-open stdin: check dependencies
        if dependencies.len() > 1 {
            panic!(
                "Actor {:?} has {} dependencies, expected at most 1 for stdin",
                node_handle,
                dependencies.len()
            );
        }

        let stdin = if let Some(&dep_handle) = dependencies.first() {
            eprintln!("[SystemRuntime] Actor {:?}: opening stdin from dependency {:?}", node_handle, dep_handle);

            // Ensure the dependency's output pipe exists (create if needed)
            if !self.pipe_pool.has_pipe(dep_handle) {
                let dep_pipe_name = format!("pipes/actor-{}", dep_handle.id());
                self.pipe_pool
                    .create_output_pipe(dep_handle, &dep_pipe_name, &self.id_gen)
                    .await;
                eprintln!("[SystemRuntime] Actor {:?}: created dependency {:?} output pipe", node_handle, dep_handle);
            }

            // Create reader for the dependency's pipe
            let pipe = self.pipe_pool.get_pipe(dep_handle);
            // Generate a unique handle for this reader
            let reader_handle = Handle::new(self.id_gen.get_next());
            let reader = pipe.get_reader(reader_handle);

            let channel_handle = self.alloc_channel_handle();
            self.channels.insert(channel_handle, Channel::Reader(Some(reader)));
            eprintln!("[SystemRuntime] Actor {:?}: stdin configured as {:?}", node_handle, channel_handle);
            channel_handle
        } else {
            eprintln!("[SystemRuntime] Actor {:?}: no dependencies, creating empty stdin", node_handle);

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
            eprintln!("[SystemRuntime] Actor {:?}: empty stdin configured as {:?}", node_handle, channel_handle);
            channel_handle
        };

        // Pre-open stdout: create output pipe
        eprintln!("[SystemRuntime] Actor {:?}: opening stdout", node_handle);

        if !self.pipe_pool.has_pipe(node_handle) {
            let pipe_name = format!("pipes/actor-{}", node_handle.id());
            self.pipe_pool
                .create_output_pipe(node_handle, &pipe_name, &self.id_gen)
                .await;
            eprintln!("[SystemRuntime] Actor {:?}: created output pipe", node_handle);
        }

        let stdout = self.alloc_channel_handle();
        self.channels.insert(stdout, Channel::Writer { node_handle });
        eprintln!("[SystemRuntime] Actor {:?}: stdout configured as {:?}", node_handle, stdout);

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
        eprintln!("[SystemRuntime] Processing OpenRead for {node_handle:?}");
        // No dependency wiring for now - just return a dummy handle
        let channel_handle = self.alloc_channel_handle();
        eprintln!("[SystemRuntime] OpenRead {node_handle:?}: no input configured, dummy {channel_handle:?}");
        let _ = response.send(channel_handle);
    }

    /// Handler for `OpenWrite` requests - creates output pipe and returns ChannelHandle
    async fn handle_open_write(
        &mut self,
        node_handle: Handle,
        response: oneshot::Sender<ChannelHandle>,
    ) {
        eprintln!("[SystemRuntime] Processing OpenWrite for {node_handle:?}");

        // Create output pipe for this actor if it doesn't exist yet
        if !self.pipe_pool.has_pipe(node_handle) {
            let pipe_name = format!("pipes/actor-{}", node_handle.id());
            self.pipe_pool
                .create_output_pipe(node_handle, &pipe_name, &self.id_gen)
                .await;
            eprintln!("[SystemRuntime] OpenWrite {node_handle:?}: created pipe");
        }

        let channel_handle = self.alloc_channel_handle();
        self.channels
            .insert(channel_handle, Channel::Writer { node_handle });
        eprintln!("[SystemRuntime] OpenWrite {node_handle:?}: created {channel_handle:?}");
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

    /// Handler for Write requests - writes to actor's pipe
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

        let result = if let Some(Channel::Writer { node_handle }) = self.channels.get(&handle) {
            let node_handle = *node_handle;
            eprintln!("[SystemRuntime] Write {handle:?}: writing to pipe");
            let pipe = self.pipe_pool.get_pipe(node_handle);
            let n = pipe.writer().write(data);
            eprintln!("[SystemRuntime] Write {handle:?}: pipe write returned {n}");
            #[allow(clippy::cast_possible_truncation)]
            {
                n as c_int
            }
        } else {
            eprintln!("[SystemRuntime] Write {handle:?}: channel not found or not a writer");
            -1
        };
        eprintln!("[SystemRuntime] Write {handle:?} completed");
        Box::pin(async move { IoEvent::SyncComplete { result, response } })
    }

    /// Handler for Close requests - uses ChannelHandle
    fn handle_close(
        &mut self,
        handle: ChannelHandle,
        response: oneshot::Sender<c_int>,
    ) -> IoFuture {
        eprintln!("[SystemRuntime] Processing Close for {handle:?}");
        let mut result = 0;

        if let Some(channel) = self.channels.remove(&handle) {
            match channel {
                Channel::Reader(_) => {
                    eprintln!("[SystemRuntime] Close {handle:?}: closed reader");
                }
                Channel::Writer { node_handle } => {
                    let pipe = self.pipe_pool.get_pipe(node_handle);
                    pipe.writer().close();
                    if let Err(e) = self.pipe_pool.flush_buffer(node_handle) {
                        eprintln!("[SystemRuntime] Failed to flush buffer: {e}");
                        result = -1;
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
        eprintln!(
            "[StubActorRuntime] Actor {:?}: requesting std handles setup with deps {:?}",
            self.node_handle, dependencies
        );

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

        eprintln!(
            "[StubActorRuntime] Actor {:?}: std handles ready (stdin=0, stdout=1)",
            self.node_handle
        );
    }

    /// Close all open handles when actor finishes.
    /// Closes in reverse order (highest fd first) to handle any dependencies.
    #[allow(clippy::unwrap_used)]
    pub fn close_all_handles(&self) {
        eprintln!(
            "[StubActorRuntime] Actor {:?}: close_all_handles()",
            self.node_handle
        );

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
            eprintln!(
                "[StubActorRuntime] Actor {:?}: closing fd {}",
                self.node_handle, fd
            );
            let _ = self.aclose(fd);
        }

        eprintln!(
            "[StubActorRuntime] Actor {:?}: all handles closed",
            self.node_handle
        );
    }
}

#[allow(clippy::unwrap_used)] // Stub implementation for testing - panics are acceptable
impl ActorRuntime for StubActorRuntime {
    fn get_errno(&self) -> c_int {
        eprintln!(
            "[StubActorRuntime] Actor {:?}: get_errno() entry",
            self.node_handle
        );
        0 // No error
    }

    fn open_read(&self, _name: &str) -> c_int {
        eprintln!(
            "[StubActorRuntime] Actor {:?}: open_read() entry",
            self.node_handle
        );
        // Send request to SystemRuntime and block for response
        let (tx, rx) = oneshot::channel();

        self.system_tx
            .send(IoRequest::OpenRead {
                node_handle: self.node_handle,
                response: tx,
            })
            .unwrap();

        eprintln!(
            "[StubActorRuntime] Actor {:?}: open_read() before blocking_recv",
            self.node_handle
        );
        let channel_handle = rx.blocking_recv().unwrap();

        // Allocate local fd and map to global channel handle
        let fd = self.fd_table.lock().unwrap().insert(channel_handle);
        eprintln!(
            "[StubActorRuntime] Actor {:?}: open_read() fd={} -> {:?}",
            self.node_handle, fd, channel_handle
        );
        fd
    }

    fn open_write(&self, _name: &str) -> c_int {
        eprintln!(
            "[StubActorRuntime] Actor {:?}: open_write() entry",
            self.node_handle
        );
        // Send request to SystemRuntime and block for response
        let (tx, rx) = oneshot::channel();

        self.system_tx
            .send(IoRequest::OpenWrite {
                node_handle: self.node_handle,
                response: tx,
            })
            .unwrap();

        eprintln!(
            "[StubActorRuntime] Actor {:?}: open_write() before blocking_recv",
            self.node_handle
        );
        let channel_handle = rx.blocking_recv().unwrap();

        // Allocate local fd and map to global channel handle
        let fd = self.fd_table.lock().unwrap().insert(channel_handle);
        eprintln!(
            "[StubActorRuntime] Actor {:?}: open_write() fd={} -> {:?}",
            self.node_handle, fd, channel_handle
        );
        fd
    }

    fn aread(&self, fd: c_int, buffer: &mut [u8]) -> c_int {
        eprintln!(
            "[StubActorRuntime] Actor {:?}: aread(fd={}) entry, buffer.len={}",
            self.node_handle, fd, buffer.len()
        );

        // Look up the channel handle for this fd
        let channel_handle = match self.fd_table.lock().unwrap().get(fd) {
            Some(h) => h,
            None => {
                eprintln!(
                    "[StubActorRuntime] Actor {:?}: aread() fd={} not found",
                    self.node_handle, fd
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
            self.node_handle
        );
        let bytes_read = rx.blocking_recv().unwrap();
        eprintln!(
            "[StubActorRuntime] Actor {:?}: aread() after blocking_recv, bytes_read={}",
            self.node_handle, bytes_read
        );

        bytes_read
    }

    fn awrite(&self, fd: c_int, buffer: &[u8]) -> c_int {
        eprintln!(
            "[StubActorRuntime] Actor {:?}: awrite(fd={}) entry, buffer.len={}",
            self.node_handle, fd, buffer.len()
        );

        // Look up the channel handle for this fd
        let channel_handle = match self.fd_table.lock().unwrap().get(fd) {
            Some(h) => h,
            None => {
                eprintln!(
                    "[StubActorRuntime] Actor {:?}: awrite() fd={} not found",
                    self.node_handle, fd
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
            self.node_handle
        );
        let result = rx.blocking_recv().unwrap();
        eprintln!(
            "[StubActorRuntime] Actor {:?}: awrite() after blocking_recv, result={}",
            self.node_handle, result
        );
        result
    }

    fn aclose(&self, fd: c_int) -> c_int {
        eprintln!(
            "[StubActorRuntime] Actor {:?}: aclose() entry, fd={}",
            self.node_handle, fd
        );

        // Look up and remove the channel handle for this fd
        let channel_handle = match self.fd_table.lock().unwrap().remove(fd) {
            Some(h) => h,
            None => {
                eprintln!(
                    "[StubActorRuntime] Actor {:?}: aclose() fd={} not found",
                    self.node_handle, fd
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
            self.node_handle
        );
        let result = rx.blocking_recv().unwrap();
        eprintln!(
            "[StubActorRuntime] Actor {:?}: aclose() after blocking_recv, result={}",
            self.node_handle, result
        );
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
        eprintln!("Node to build: {:?} ({})", node_handle, idname);

        // Get dependencies for this node
        let dependencies: Vec<Handle> = dag.get_direct_dependencies(node_handle).collect();

        // Create runtime for this actor
        let runtime = StubActorRuntime::new(node_handle, system_tx.clone());

        let task = tokio::task::spawn_blocking(move || {
            eprintln!("Task {:?} ({}): Starting", node_handle, idname);

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
                Ok(()) => eprintln!("Task {:?} ({}): completed", node_handle, idname),
                Err(e) => eprintln!("Error in {:?} ({}): {e}", node_handle, idname),
            }

            // Close all handles after actor finishes
            runtime.close_all_handles();

            eprintln!("Task {:?} ({}): Done", node_handle, idname);
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
    let system_runtime = SystemRuntime::new(kv, idgen);

    // Run the system
    run_system(system_runtime, &dag, end_node).await;
}
