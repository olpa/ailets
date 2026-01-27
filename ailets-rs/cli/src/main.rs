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

/// I/O requests sent from ActorRuntime to SystemRuntime
enum IoRequest {
    /// Open a stream for reading (returns file descriptor)
    OpenRead {
        actor_id: ActorId,
        name: String,
        response: oneshot::Sender<c_int>,
    },
    /// Open a stream for writing (returns file descriptor)
    OpenWrite {
        actor_id: ActorId,
        name: String,
        response: oneshot::Sender<c_int>,
    },
    /// Read from a file descriptor (async operation)
    Read {
        actor_id: ActorId,
        fd: c_int,
        len: usize,
        response: oneshot::Sender<Vec<u8>>,
    },
    /// Write to a file descriptor (async operation)
    Write {
        actor_id: ActorId,
        fd: c_int,
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
    /// Read from stdin (for now, represented as static data for testing)
    Stdin(&'static [u8]),
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
    /// Track static data position for stdin readers
    stdin_positions: HashMap<ActorId, usize>,
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
            stdin_positions: HashMap::new(),
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

    /// Configure an actor to read from stdin (static data for testing)
    fn set_actor_stdin(&mut self, actor_id: ActorId, data: &'static [u8]) {
        self.actor_inputs.insert(actor_id, ActorInputSource::Stdin(data));
        self.stdin_positions.insert(actor_id, 0);
    }

    /// Configure an actor to read from a pipe
    fn set_actor_input_pipe(&mut self, actor_id: ActorId, pipe_id: PipeId) {
        self.actor_inputs.insert(actor_id, ActorInputSource::Pipe(pipe_id));
    }

    /// Configure an actor to write to stdout
    fn set_actor_stdout(&mut self, actor_id: ActorId) {
        self.actor_outputs.insert(actor_id, ActorOutputDestination::Stdout);
    }

    /// Configure an actor to write to a pipe
    fn set_actor_output_pipe(&mut self, actor_id: ActorId, pipe_id: PipeId) {
        self.actor_outputs.insert(actor_id, ActorOutputDestination::Pipe(pipe_id));
    }

    /// Main event loop - processes I/O requests asynchronously
    async fn run(mut self) {
        while let Some(request) = self.request_rx.recv().await {
            match request {
                IoRequest::OpenRead { actor_id: _, name: _, response } => {
                    // For now, we ignore the name and just return a dummy fd
                    // The actor_id tells us what to read from
                    let _ = response.send(0); // fd = 0
                }
                IoRequest::OpenWrite { actor_id: _, name: _, response } => {
                    // For now, we ignore the name and just return a dummy fd
                    let _ = response.send(1); // fd = 1
                }
                IoRequest::Read { actor_id, fd: _, len, response } => {
                    // Determine where to read from based on actor_id
                    if let Some(input_source) = self.actor_inputs.get(&actor_id) {
                        match input_source {
                            ActorInputSource::Stdin(data) => {
                                // Read from static stdin data
                                let pos = *self.stdin_positions.get(&actor_id).unwrap_or(&0);
                                let remaining = data.get(pos..).unwrap_or(&[]);
                                let to_copy = remaining.len().min(len);
                                let result = remaining[..to_copy].to_vec();

                                // Update position
                                self.stdin_positions.insert(actor_id, pos + to_copy);

                                let _ = response.send(result);
                            }
                            ActorInputSource::Pipe(pipe_id) => {
                                // Read from pipe (async)
                                if let Some(reader) = self.pipe_readers.get_mut(pipe_id) {
                                    let mut buf = vec![0; len];
                                    let n = reader.read(&mut buf).await;
                                    #[allow(clippy::cast_sign_loss)]
                                    buf.truncate(n as usize);
                                    let _ = response.send(buf);
                                } else {
                                    let _ = response.send(vec![]);
                                }
                            }
                        }
                    } else {
                        let _ = response.send(vec![]);
                    }
                }
                IoRequest::Write { actor_id, fd: _, data, response } => {
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
        0 // No error
    }

    fn open_read(&self, name: &str) -> c_int {
        // Send request to SystemRuntime and block for response
        let (tx, rx) = oneshot::channel();

        self.system_tx
            .send(IoRequest::OpenRead {
                actor_id: self.actor_id,
                name: name.to_string(),
                response: tx,
            })
            .unwrap();

        rx.blocking_recv().unwrap()
    }

    fn open_write(&self, name: &str) -> c_int {
        // Send request to SystemRuntime and block for response
        let (tx, rx) = oneshot::channel();

        self.system_tx
            .send(IoRequest::OpenWrite {
                actor_id: self.actor_id,
                name: name.to_string(),
                response: tx,
            })
            .unwrap();

        rx.blocking_recv().unwrap()
    }

    fn aread(&self, fd: c_int, buffer: &mut [u8]) -> c_int {
        // Send request to SystemRuntime and block for response
        let (tx, rx) = oneshot::channel();

        self.system_tx
            .send(IoRequest::Read {
                actor_id: self.actor_id,
                fd,
                len: buffer.len(),
                response: tx,
            })
            .unwrap();

        // Block waiting for SystemRuntime to complete the async read
        let data = rx.blocking_recv().unwrap();

        // Copy result into buffer
        let n = data.len().min(buffer.len());
        buffer[..n].copy_from_slice(&data[..n]);

        #[allow(clippy::cast_possible_truncation)]
        {
            n as c_int
        }
    }

    fn awrite(&self, fd: c_int, buffer: &[u8]) -> c_int {
        // Send request to SystemRuntime and block for response
        let (tx, rx) = oneshot::channel();

        self.system_tx
            .send(IoRequest::Write {
                actor_id: self.actor_id,
                fd,
                data: buffer.to_vec(),
                response: tx,
            })
            .unwrap();

        rx.blocking_recv().unwrap()
    }

    fn aclose(&self, fd: c_int) -> c_int {
        // Send request to SystemRuntime and block for response
        let (tx, rx) = oneshot::channel();

        self.system_tx
            .send(IoRequest::Close {
                actor_id: self.actor_id,
                fd,
                response: tx,
            })
            .unwrap();

        rx.blocking_recv().unwrap()
    }
}

#[tokio::main]
async fn main() {
    // Create SystemRuntime and configure it
    let mut system_runtime = SystemRuntime::new();

    // Define actor IDs
    let actor1_id = ActorId(1);
    let actor2_id = ActorId(2);

    // Create pipe connecting actor1 to actor2
    let pipe_id = system_runtime.create_pipe("cat-pipe");

    // Configure Actor 1: reads from stdin (static data), writes to pipe
    system_runtime.set_actor_stdin(actor1_id, b"Hello, world!\n");
    system_runtime.set_actor_output_pipe(actor1_id, pipe_id);

    // Configure Actor 2: reads from pipe, writes to stdout
    system_runtime.set_actor_input_pipe(actor2_id, pipe_id);
    system_runtime.set_actor_stdout(actor2_id);

    // Get ActorRuntimes from SystemRuntime (before moving system_runtime)
    let runtime1 = system_runtime.create_actor_runtime(actor1_id);
    let runtime2 = system_runtime.create_actor_runtime(actor2_id);

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
