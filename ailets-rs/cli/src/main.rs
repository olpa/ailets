use actor_io::{AReader, AWriter};
use actor_runtime::{ActorRuntime, StdHandle};
use ailetos::notification_queue::{Handle, NotificationQueueArc};
use ailetos::pipe::{Buffer, Pipe, Reader, Writer};
use std::cell::RefCell;
use std::io::Write as StdWrite;
use std::os::raw::c_int;

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

/// Input source for StubActorRuntime
enum InputSource {
    /// Read from explicit static byte slice
    Static {
        data: &'static [u8],
        position: std::cell::Cell<usize>,
    },
    /// Read from pipe
    Pipe(RefCell<Reader<VecBuffer>>),
}

/// Output destination for StubActorRuntime
enum OutputDestination<'a> {
    /// Write to stdout
    Stdout,
    /// Write to pipe
    Pipe(&'a Writer<VecBuffer>),
}

/// Stub `ActorRuntime` implementation for CLI testing
/// Supports multiple input/output modes:
/// - Input: explicit value or from pipe
/// - Output: to stdout or to pipe
struct StubActorRuntime<'a> {
    input: InputSource,
    output: OutputDestination<'a>,
}

impl<'a> StubActorRuntime<'a> {
    /// Create runtime with pipe input and stdout output
    fn from_pipe_to_stdout(reader: Reader<VecBuffer>) -> Self {
        Self {
            input: InputSource::Pipe(RefCell::new(reader)),
            output: OutputDestination::Stdout,
        }
    }

    /// Create runtime with static input and pipe output
    fn to_pipe(data: &'static [u8], writer: &'a Writer<VecBuffer>) -> Self {
        Self {
            input: InputSource::Static {
                data,
                position: std::cell::Cell::new(0),
            },
            output: OutputDestination::Pipe(writer),
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
        match &self.input {
            InputSource::Static { data, position } => {
                let pos = position.get();
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

                position.set(pos + to_copy);
                #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
                let result = to_copy as c_int;
                result
            }
            InputSource::Pipe(reader) => {
                // Block on async read using block_in_place to avoid runtime nesting issues
                let result = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async {
                        let mut reader = reader.borrow_mut();
                        reader.read(buffer).await
                    })
                });
                #[allow(clippy::cast_possible_truncation)]
                {
                    result as c_int
                }
            }
        }
    }

    fn awrite(&self, _fd: c_int, buffer: &[u8]) -> c_int {
        match &self.output {
            OutputDestination::Stdout => match std::io::stdout().write(buffer) {
                Ok(n) => {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
                    let result = n as c_int;
                    result
                }
                Err(_) => -1,
            },
            OutputDestination::Pipe(writer) => {
                let result = writer.write(buffer);
                #[allow(clippy::cast_possible_truncation)]
                {
                    result as c_int
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
    // Create notification queue
    let queue = NotificationQueueArc::new();
    let writer_handle = Handle::new(1);
    let reader_handle = Handle::new(2);

    // Create pipe
    let pipe = Pipe::new(writer_handle, queue.clone(), "cat-pipe", VecBuffer::new());
    let reader = pipe.get_reader(reader_handle);

    // First cat::execute - reads "Hello, world!\n" and writes to pipe
    let task1 = tokio::spawn(async move {
        let runtime1 = StubActorRuntime::to_pipe(b"Hello, world!\n", pipe.writer());
        let areader1 = AReader::new_from_std(&runtime1, StdHandle::Stdin);
        let awriter1 = AWriter::new_from_std(&runtime1, StdHandle::Stdout);

        match cat::execute(areader1, awriter1) {
            Ok(()) => {}
            Err(e) => eprintln!("Error in first cat: {e}"),
        }

        // Close the writer to signal EOF to the reader
        pipe.writer().close();
    });

    // Second cat::execute - reads from pipe and writes to stdout
    let task2 = tokio::spawn(async move {
        let runtime2 = StubActorRuntime::from_pipe_to_stdout(reader);
        let areader2 = AReader::new_from_std(&runtime2, StdHandle::Stdin);
        let awriter2 = AWriter::new_from_std(&runtime2, StdHandle::Stdout);

        match cat::execute(areader2, awriter2) {
            Ok(()) => {}
            Err(e) => eprintln!("Error in second cat: {e}"),
        }
    });

    // Wait for both tasks to complete
    let _ = tokio::join!(task1, task2);
}
