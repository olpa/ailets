use actor_io::{AReader, AWriter};
use actor_runtime::{ActorRuntime, StdHandle};
use ailetos::notification_queue::{Handle, NotificationQueueArc};
use ailetos::pipe::{Buffer, Pipe, Reader};
use std::collections::HashMap;
use std::io::Write as StdWrite;
use std::os::raw::c_int;
use tokio::sync::{mpsc, oneshot};

/// Simple Vec<u8> wrapper implementing Buffer trait for pipe usage
struct VecBuffer(Vec<u8>);

impl VecBuffer {
    fn new() -> Self {
        Self(Vec::new())
    }
}

impl Buffer for VecBuffer {
    fn write(&mut self, data: &[u8]) -> isize {
        self.0.extend_from_slice(data);
        #[allow(clippy::cast_possible_wrap)]
        {
            data.len() as isize
        }
    }

    fn len(&self) -> usize {
        self.0.len()
    }

    fn as_slice(&self) -> &[u8] {
        &self.0
    }
}

/// Unique identifier for actors in the system
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ActorId(usize);

/// Unique identifier for pipes in the system
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PipeId(usize);

/// A wrapper around a raw mutable slice pointer that can be sent between threads.
/// SAFETY: This is only safe because the sender (aread) blocks until the receiver
/// (SystemRuntime handler) sends a response, ensuring:
/// 1. The buffer remains valid (stack frame doesn't unwind)
/// 2. No concurrent access (sender is blocked)
/// 3. Proper synchronization (channel enforces happens-before)
struct SendableBuffer {
    ptr: *mut [u8],
    #[cfg(debug_assertions)]
    consumed: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl SendableBuffer {
    /// Create a new SendableBuffer from a mutable slice reference.
    ///
    /// # Safety
    ///
    /// Caller must ensure:
    /// 1. The pointer remains valid until consumed via `into_raw()`
    /// 2. The caller will block waiting for a response before the buffer goes out of scope
    /// 3. No other references to this buffer exist during the async operation
    /// 4. The SendableBuffer is consumed exactly once via `into_raw()`
    unsafe fn new(buffer: &mut [u8]) -> Self {
        Self {
            ptr: buffer as *mut [u8],
            #[cfg(debug_assertions)]
            consumed: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Consume the SendableBuffer and return the raw pointer.
    /// This prevents accidental reuse of the same buffer.
    fn into_raw(self) -> *mut [u8] {
        #[cfg(debug_assertions)]
        {
            let already_consumed = self.consumed.swap(true, std::sync::atomic::Ordering::SeqCst);
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

/// I/O requests sent from ActorRuntime to SystemRuntime
enum IoRequest {
    /// Open a stream for reading (returns file descriptor)
    OpenRead {
        response: oneshot::Sender<c_int>,
    },
    /// Open a stream for writing (returns file descriptor)
    OpenWrite {
        response: oneshot::Sender<c_int>,
    },
    /// Read from a file descriptor (async operation)
    /// SAFETY: The buffer pointer must remain valid until the response is sent.
    /// This is guaranteed because aread() blocks waiting for the response.
    Read {
        actor_id: ActorId,
        buffer: SendableBuffer,
        response: oneshot::Sender<c_int>,
    },
    /// Write to a file descriptor (async operation)
    Write {
        actor_id: ActorId,
        data: Vec<u8>,
        response: oneshot::Sender<c_int>,
    },
    /// Close a file descriptor
    Close {
        actor_id: ActorId,
        fd: c_int,
        response: oneshot::Sender<c_int>,
    },
}

/// Input source configuration for an actor
enum ActorInputSource {
    /// Read from a pipe
    Pipe(PipeId),
}

/// Output destination configuration for an actor
enum ActorOutputDestination {
    /// Write to stdout
    Stdout,
    /// Write to a pipe
    Pipe(PipeId),
}

/// SystemRuntime manages all async I/O operations
/// Actors communicate with it via channels
struct SystemRuntime {
    /// All pipes in the system (we store the whole pipe to access both reader and writer)
    pipes: HashMap<PipeId, Pipe<VecBuffer>>,
    /// All pipe readers in the system (readers are async)
    pipe_readers: HashMap<PipeId, Reader<VecBuffer>>,
    /// Input configuration for each actor
    actor_inputs: HashMap<ActorId, ActorInputSource>,
    /// Output configuration for each actor
    actor_outputs: HashMap<ActorId, ActorOutputDestination>,
    /// Channel to send I/O requests to this runtime
    system_tx: mpsc::UnboundedSender<IoRequest>,
    /// Receives I/O requests from actors
    request_rx: mpsc::UnboundedReceiver<IoRequest>,
    /// Counter for generating unique pipe IDs
    next_pipe_id: usize,
    /// Counter for generating unique notification queue handles
    next_handle_id: i64,
}

impl SystemRuntime {
    fn new() -> Self {
        let (system_tx, request_rx) = mpsc::unbounded_channel();
        Self {
            pipes: HashMap::new(),
            pipe_readers: HashMap::new(),
            actor_inputs: HashMap::new(),
            actor_outputs: HashMap::new(),
            system_tx,
            request_rx,
            next_pipe_id: 1,
            next_handle_id: 1,
        }
    }

    /// Factory method to create an ActorRuntime for a specific actor
    fn create_actor_runtime(&self, actor_id: ActorId) -> StubActorRuntime {
        StubActorRuntime::new(actor_id, self.system_tx.clone())
    }

    /// Setup standard handles for all actors
    /// This configures the I/O mappings directly instead of going through the request channel
    fn setup_std_handles(&mut self) {
        // Create pipe 1: pre-filled with test data for Actor 1 to read from
        let input_pipe_id = self.create_pipe("input-data");
        if let Some(pipe) = self.pipes.get(&input_pipe_id) {
            let test_data = b"Hello, world!\n";
            let written = pipe.writer().write(test_data);
            assert_eq!(written, test_data.len() as isize, "Failed to write test data to input pipe");
        }

        // Create pipe 2: for Actor 1 -> Actor 2 communication
        let cat_pipe_id = self.create_pipe("cat-pipe");

        // Actor 1: reads from input pipe, writes to cat pipe
        self.actor_inputs.insert(ActorId(1), ActorInputSource::Pipe(input_pipe_id));
        self.actor_outputs.insert(ActorId(1), ActorOutputDestination::Pipe(cat_pipe_id));

        // Actor 2: reads from cat pipe, writes to stdout
        self.actor_inputs.insert(ActorId(2), ActorInputSource::Pipe(cat_pipe_id));
        self.actor_outputs.insert(ActorId(2), ActorOutputDestination::Stdout);
    }

    /// Create a new pipe and return its ID
    fn create_pipe(&mut self, name: &str) -> PipeId {
        let pipe_id = PipeId(self.next_pipe_id);
        self.next_pipe_id += 1;

        let queue = NotificationQueueArc::new();
        let writer_handle = Handle::new(self.next_handle_id);
        self.next_handle_id += 1;
        let reader_handle = Handle::new(self.next_handle_id);
        self.next_handle_id += 1;

        let pipe = Pipe::new(writer_handle, queue, name, VecBuffer::new());
        let reader = pipe.get_reader(reader_handle);

        self.pipes.insert(pipe_id, pipe);
        self.pipe_readers.insert(pipe_id, reader);

        pipe_id
    }

    /// Main event loop - processes I/O requests asynchronously
    async fn run(mut self) {
        while let Some(request) = self.request_rx.recv().await {
            match request {
                IoRequest::OpenRead { response, .. } => {
                    // Input sources are pre-configured in setup_std_handles()
                    // Just return dummy fd
                    let _ = response.send(0); // fd = 0
                }
                IoRequest::OpenWrite { response, .. } => {
                    // Output destinations are pre-configured in setup_std_handles()
                    // Just return dummy fd
                    let _ = response.send(1); // fd = 1
                }
                IoRequest::Read { actor_id, buffer, response } => {
                    // SAFETY: The buffer pointer is valid because aread() is blocked
                    // waiting for our response, keeping its stack frame alive.
                    // We consume the SendableBuffer here to prevent reuse.
                    let buf = unsafe { &mut *buffer.into_raw() };

                    // All reads are from pipes now
                    if let Some(ActorInputSource::Pipe(pipe_id)) = self.actor_inputs.get(&actor_id) {
                        if let Some(reader) = self.pipe_readers.get_mut(pipe_id) {
                            let n = reader.read(buf).await;
                            #[allow(clippy::cast_possible_truncation)]
                            let _ = response.send(n as c_int);
                        } else {
                            let _ = response.send(0);
                        }
                    } else {
                        let _ = response.send(0);
                    }
                }
                IoRequest::Write { actor_id, data, response } => {
                    // Determine where to write based on actor_id
                    if let Some(output_dest) = self.actor_outputs.get(&actor_id) {
                        let result = match output_dest {
                            ActorOutputDestination::Stdout => {
                                // Write to stdout (sync)
                                match std::io::stdout().write(&data) {
                                    Ok(n) => {
                                        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
                                        {
                                            n as c_int
                                        }
                                    }
                                    Err(_) => -1,
                                }
                            }
                            ActorOutputDestination::Pipe(pipe_id) => {
                                // Write to pipe (sync)
                                if let Some(pipe) = self.pipes.get(pipe_id) {
                                    let n = pipe.writer().write(&data);
                                    #[allow(clippy::cast_possible_truncation)]
                                    {
                                        n as c_int
                                    }
                                } else {
                                    -1
                                }
                            }
                        };
                        let _ = response.send(result);
                    } else {
                        let _ = response.send(-1);
                    }
                }
                IoRequest::Close { actor_id, fd, response } => {
                    // Close the underlying resource if it's a pipe
                    // fd=1 is stdout/writer, fd=0 is stdin/reader
                    if fd == 1 {
                        // Closing a writer
                        if let Some(ActorOutputDestination::Pipe(pipe_id)) = self.actor_outputs.get(&actor_id) {
                            if let Some(pipe) = self.pipes.get(pipe_id) {
                                pipe.writer().close();
                            }
                        }
                    }
                    // Readers don't need explicit close - they clean up on drop
                    let _ = response.send(0);
                }
            }
        }
    }
}

/// Stub `ActorRuntime` implementation for CLI testing
/// Acts as a pure proxy to SystemRuntime for all I/O operations
/// Provides sync-to-async adapters (blocking on async operations)
#[derive(Clone)]
pub struct StubActorRuntime {
    /// This actor's unique identifier
    actor_id: ActorId,
    /// Channel to send async I/O requests to SystemRuntime
    system_tx: mpsc::UnboundedSender<IoRequest>,
}

impl StubActorRuntime {
    /// Create a new ActorRuntime for the given actor ID
    fn new(actor_id: ActorId, system_tx: mpsc::UnboundedSender<IoRequest>) -> Self {
        Self {
            actor_id,
            system_tx,
        }
    }
}

impl ActorRuntime for StubActorRuntime {
    fn get_errno(&self) -> c_int {
        eprintln!("[StubActorRuntime] Actor {:?}: get_errno() entry", self.actor_id);
        0 // No error
    }

    fn open_read(&self, _name: &str) -> c_int {
        eprintln!("[StubActorRuntime] Actor {:?}: open_read() entry", self.actor_id);
        // Send request to SystemRuntime and block for response
        let (tx, rx) = oneshot::channel();

        self.system_tx
            .send(IoRequest::OpenRead {
                response: tx,
            })
            .unwrap();

        eprintln!("[StubActorRuntime] Actor {:?}: open_read() before blocking_recv", self.actor_id);
        let result = rx.blocking_recv().unwrap();
        eprintln!("[StubActorRuntime] Actor {:?}: open_read() after blocking_recv, result={}", self.actor_id, result);
        result
    }

    fn open_write(&self, _name: &str) -> c_int {
        eprintln!("[StubActorRuntime] Actor {:?}: open_write() entry", self.actor_id);
        // Send request to SystemRuntime and block for response
        let (tx, rx) = oneshot::channel();

        self.system_tx
            .send(IoRequest::OpenWrite {
                response: tx,
            })
            .unwrap();

        eprintln!("[StubActorRuntime] Actor {:?}: open_write() before blocking_recv", self.actor_id);
        let result = rx.blocking_recv().unwrap();
        eprintln!("[StubActorRuntime] Actor {:?}: open_write() after blocking_recv, result={}", self.actor_id, result);
        result
    }

    fn aread(&self, _fd: c_int, buffer: &mut [u8]) -> c_int {
        eprintln!("[StubActorRuntime] Actor {:?}: aread() entry, buffer.len={}", self.actor_id, buffer.len());
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
                actor_id: self.actor_id,
                buffer: buffer_ptr,
                response: tx,
            })
            .unwrap();

        // Block waiting for SystemRuntime to complete the async read
        eprintln!("[StubActorRuntime] Actor {:?}: aread() before blocking_recv", self.actor_id);
        let bytes_read = rx.blocking_recv().unwrap();
        eprintln!("[StubActorRuntime] Actor {:?}: aread() after blocking_recv, bytes_read={}", self.actor_id, bytes_read);

        bytes_read
    }

    fn awrite(&self, _fd: c_int, buffer: &[u8]) -> c_int {
        eprintln!("[StubActorRuntime] Actor {:?}: awrite() entry, buffer.len={}", self.actor_id, buffer.len());
        // Send request to SystemRuntime and block for response
        let (tx, rx) = oneshot::channel();

        self.system_tx
            .send(IoRequest::Write {
                actor_id: self.actor_id,
                data: buffer.to_vec(),
                response: tx,
            })
            .unwrap();

        eprintln!("[StubActorRuntime] Actor {:?}: awrite() before blocking_recv", self.actor_id);
        let result = rx.blocking_recv().unwrap();
        eprintln!("[StubActorRuntime] Actor {:?}: awrite() after blocking_recv, result={}", self.actor_id, result);
        result
    }

    fn aclose(&self, fd: c_int) -> c_int {
        eprintln!("[StubActorRuntime] Actor {:?}: aclose() entry, fd={}", self.actor_id, fd);
        // Send request to SystemRuntime and block for response
        let (tx, rx) = oneshot::channel();

        self.system_tx
            .send(IoRequest::Close {
                actor_id: self.actor_id,
                fd,
                response: tx,
            })
            .unwrap();

        eprintln!("[StubActorRuntime] Actor {:?}: aclose() before blocking_recv", self.actor_id);
        let result = rx.blocking_recv().unwrap();
        eprintln!("[StubActorRuntime] Actor {:?}: aclose() after blocking_recv, result={}", self.actor_id, result);
        result
    }
}

#[tokio::main]
async fn main() {
    // Create SystemRuntime and setup standard handles for all actors
    let mut system_runtime = SystemRuntime::new();

    // Define actor IDs
    let actor1_id = ActorId(1);
    let actor2_id = ActorId(2);

    // Get ActorRuntimes from SystemRuntime (before moving system_runtime)
    let runtime1 = system_runtime.create_actor_runtime(actor1_id);
    let runtime2 = system_runtime.create_actor_runtime(actor2_id);

    // Setup standard handles for all actors directly on SystemRuntime
    eprintln!("Setup: Setting up standard handles for all actors");
    system_runtime.setup_std_handles();
    eprintln!("Setup: All handles configured");

    // Spawn SystemRuntime task
    let system_task = tokio::spawn(async move {
        system_runtime.run().await;
    });

    // First actor: reads from stdin (static data) and writes to pipe
    let task1 = tokio::task::spawn_blocking(move || {
        eprintln!("Task1: Starting");

        let areader1 = AReader::new_from_std(&runtime1, StdHandle::Stdin);
        let awriter1 = AWriter::new_from_std(&runtime1, StdHandle::Stdout);

        eprintln!("Task1: About to execute cat");
        match cat::execute(areader1, awriter1) {
            Ok(()) => eprintln!("Task1: Cat completed successfully"),
            Err(e) => eprintln!("Error in first cat: {e}"),
        }
        eprintln!("Task1: Done");
    });

    // Second actor: reads from pipe and writes to stdout
    let task2 = tokio::task::spawn_blocking(move || {
        eprintln!("Task2: Starting");

        let areader2 = AReader::new_from_std(&runtime2, StdHandle::Stdin);
        let awriter2 = AWriter::new_from_std(&runtime2, StdHandle::Stdout);

        eprintln!("Task2: About to execute cat");
        match cat::execute(areader2, awriter2) {
            Ok(()) => eprintln!("Task2: Cat completed successfully"),
            Err(e) => eprintln!("Error in second cat: {e}"),
        }
        eprintln!("Task2: Done");
    });

    // Wait for all tasks to complete
    let (system_result, task1_result, task2_result) = tokio::join!(system_task, task1, task2);

    // Check for panics or errors
    if let Err(e) = system_result {
        eprintln!("SystemRuntime task failed: {e}");
    }
    if let Err(e) = task1_result {
        eprintln!("Task1 failed: {e}");
    }
    if let Err(e) = task2_result {
        eprintln!("Task2 failed: {e}");
    }
}
