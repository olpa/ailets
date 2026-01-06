use actor_io::{AReader, AWriter};
use actor_runtime::{ActorRuntime, StdHandle};
use ailetos::notification_queue::{Handle, NotificationQueueArc};
use ailetos::pipe::{Buffer, Pipe, Reader, Writer};
use std::cell::RefCell;
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

/// Unique identifier for pipes in the system
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct PipeId(usize);

/// I/O requests sent from ActorRuntime to SystemRuntime
enum IoRequest {
    /// Read from a pipe (async operation)
    Read {
        pipe_id: PipeId,
        len: usize,
        response: oneshot::Sender<Vec<u8>>,
    },
}

/// SystemRuntime manages all async I/O operations
/// Actors communicate with it via channels
struct SystemRuntime {
    /// All pipe readers in the system (readers are async)
    readers: HashMap<PipeId, Reader<VecBuffer>>,
    /// Receives I/O requests from actors
    request_rx: mpsc::UnboundedReceiver<IoRequest>,
}

impl SystemRuntime {
    fn new(request_rx: mpsc::UnboundedReceiver<IoRequest>) -> Self {
        Self {
            readers: HashMap::new(),
            request_rx,
        }
    }

    fn add_reader(&mut self, pipe_id: PipeId, reader: Reader<VecBuffer>) {
        self.readers.insert(pipe_id, reader);
    }

    /// Main event loop - processes I/O requests asynchronously
    async fn run(mut self) {
        while let Some(request) = self.request_rx.recv().await {
            match request {
                IoRequest::Read { pipe_id, len, response } => {
                    if let Some(reader) = self.readers.get_mut(&pipe_id) {
                        let mut buf = vec![0; len];
                        let n = reader.read(&mut buf).await;
                        #[allow(clippy::cast_sign_loss)]
                        buf.truncate(n as usize);
                        let _ = response.send(buf); // Unblocks actor
                    }
                }
            }
        }
    }
}

/// Input source for StubActorRuntime
enum InputSource {
    /// Read from explicit static byte slice
    Static {
        data: &'static [u8],
        position: usize,
    },
    /// Read from pipe via SystemRuntime
    Pipe(PipeId),
}

/// Output destination for StubActorRuntime
enum OutputDestination<'a> {
    /// Write to stdout (synchronous)
    Stdout,
    /// Write to pipe (synchronous)
    Pipe(&'a Writer<VecBuffer>),
}

/// Stub `ActorRuntime` implementation for CLI testing
/// Acts as a proxy to SystemRuntime for async I/O operations
struct StubActorRuntime<'a> {
    input: RefCell<InputSource>,
    output: OutputDestination<'a>,
    /// Channel to send async I/O requests to SystemRuntime
    system_tx: mpsc::UnboundedSender<IoRequest>,
}

impl<'a> StubActorRuntime<'a> {
    /// Create runtime with pipe input and stdout output
    fn from_pipe_to_stdout(pipe_id: PipeId, system_tx: mpsc::UnboundedSender<IoRequest>) -> Self {
        Self {
            input: RefCell::new(InputSource::Pipe(pipe_id)),
            output: OutputDestination::Stdout,
            system_tx,
        }
    }

    /// Create runtime with static input and pipe output
    fn to_pipe(
        data: &'static [u8],
        writer: &'a Writer<VecBuffer>,
        system_tx: mpsc::UnboundedSender<IoRequest>,
    ) -> Self {
        Self {
            input: RefCell::new(InputSource::Static {
                data,
                position: 0,
            }),
            output: OutputDestination::Pipe(writer),
            system_tx,
        }
    }
}

impl<'a> ActorRuntime for StubActorRuntime<'a> {
    fn get_errno(&self) -> c_int {
        0 // No error
    }

    fn open_read(&self, _name: &str) -> c_int {
        0 // Success, return dummy fd
    }

    fn open_write(&self, _name: &str) -> c_int {
        1 // Success, return dummy fd
    }

    fn aread(&self, _fd: c_int, buffer: &mut [u8]) -> c_int {
        match &mut *self.input.borrow_mut() {
            InputSource::Static { data, position } => {
                // Static input: read directly from memory (no async)
                let pos = *position;
                if pos >= data.len() {
                    return 0; // EOF
                }

                let Some(remaining) = data.get(pos..) else {
                    return 0; // EOF if position is beyond data
                };
                let to_copy = remaining.len().min(buffer.len());

                let Some(buffer_slice) = buffer.get_mut(..to_copy) else {
                    return -1; // Error if buffer slice is invalid
                };
                let Some(data_slice) = remaining.get(..to_copy) else {
                    return -1; // Error if data slice is invalid
                };
                buffer_slice.copy_from_slice(data_slice);

                *position = pos + to_copy;
                #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
                let result = to_copy as c_int;
                result
            }
            InputSource::Pipe(pipe_id) => {
                // Pipe input: send request to SystemRuntime and block on response
                let (tx, rx) = oneshot::channel();

                self.system_tx
                    .send(IoRequest::Read {
                        pipe_id: *pipe_id,
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
        }
    }

    fn awrite(&self, _fd: c_int, buffer: &[u8]) -> c_int {
        // Writes are synchronous - no need to go through SystemRuntime
        match &self.output {
            OutputDestination::Stdout => match std::io::stdout().write(buffer) {
                Ok(n) => {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
                    {
                        n as c_int
                    }
                }
                Err(_) => -1,
            },
            OutputDestination::Pipe(writer) => {
                let n = writer.write(buffer);
                #[allow(clippy::cast_possible_truncation)]
                {
                    n as c_int
                }
            }
        }
    }

    fn aclose(&self, _fd: c_int) -> c_int {
        0 // Success
    }
}

#[tokio::main]
async fn main() {
    // Create channel for actor -> SystemRuntime communication
    let (system_tx, system_rx) = mpsc::unbounded_channel();

    // Create notification queue for pipe
    let queue = NotificationQueueArc::new();
    let writer_handle = Handle::new(1);
    let reader_handle = Handle::new(2);

    // Create pipe and extract reader
    let pipe = Pipe::new(writer_handle, queue.clone(), "cat-pipe", VecBuffer::new());
    let reader = pipe.get_reader(reader_handle);

    // Create SystemRuntime and register the pipe reader
    let mut system_runtime = SystemRuntime::new(system_rx);
    let pipe_reader_id = PipeId(2);
    system_runtime.add_reader(pipe_reader_id, reader);

    // Spawn SystemRuntime task
    tokio::spawn(async move {
        system_runtime.run().await;
    });

    // First actor: reads "Hello, world!\n" and writes to pipe
    // Move pipe into task1 so it owns the writer
    let system_tx1 = system_tx.clone();
    let task1 = tokio::task::spawn_blocking(move || {
        let writer = pipe.writer();
        let runtime1 = StubActorRuntime::to_pipe(b"Hello, world!\n", writer, system_tx1);
        let areader1 = AReader::new_from_std(&runtime1, StdHandle::Stdin);
        let awriter1 = AWriter::new_from_std(&runtime1, StdHandle::Stdout);

        match cat::execute(areader1, awriter1) {
            Ok(()) => {}
            Err(e) => eprintln!("Error in first cat: {e}"),
        }

        // Close the writer to signal EOF to the reader
        pipe.writer().close();
    });

    // Second actor: reads from pipe and writes to stdout
    let system_tx2 = system_tx;
    let task2 = tokio::task::spawn_blocking(move || {
        let runtime2 = StubActorRuntime::from_pipe_to_stdout(pipe_reader_id, system_tx2);
        let areader2 = AReader::new_from_std(&runtime2, StdHandle::Stdin);
        let awriter2 = AWriter::new_from_std(&runtime2, StdHandle::Stdout);

        match cat::execute(areader2, awriter2) {
            Ok(()) => {}
            Err(e) => eprintln!("Error in second cat: {e}"),
        }
    });

    // Wait for both actors to complete
    let _ = tokio::join!(task1, task2);
}
