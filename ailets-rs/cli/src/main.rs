use actor_io::{AReader, AWriter};
use actor_runtime::{ActorRuntime, StdHandle};
use std::io::Write as StdWrite;
use std::os::raw::c_int;

/// Stub `ActorRuntime` implementation for CLI testing
/// - Reads return data from provided slice
/// - Writes go to stdout
struct StubActorRuntime {
    data: &'static [u8],
    position: std::cell::Cell<usize>,
}

impl StubActorRuntime {
    fn new(data: &'static [u8]) -> Self {
        Self {
            data,
            position: std::cell::Cell::new(0),
        }
    }
}

impl ActorRuntime for StubActorRuntime {
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
        let position = self.position.get();
        if position >= self.data.len() {
            return 0; // EOF
        }

        let Some(remaining) = self.data.get(position..) else {
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

        self.position.set(position + to_copy);
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let result = to_copy as c_int;
        result
    }

    fn awrite(&self, _fd: c_int, buffer: &[u8]) -> c_int {
        match std::io::stdout().write(buffer) {
            Ok(n) => {
                #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
                let result = n as c_int;
                result
            }
            Err(_) => -1,
        }
    }

    fn aclose(&self, _fd: c_int) -> c_int {
        0 // Success
    }
}

fn main() {
    // First cat::execute - prints "Hello, "
    let runtime1 = StubActorRuntime::new(b"Hello, ");
    let reader1 = AReader::new_from_std(&runtime1, StdHandle::Stdin);
    let writer1 = AWriter::new_from_std(&runtime1, StdHandle::Stdout);

    match cat::execute(reader1, writer1) {
        Ok(()) => {}
        Err(e) => eprintln!("Error: {e}"),
    }

    // Second cat::execute - prints "world!"
    let runtime2 = StubActorRuntime::new(b"world!\n");
    let reader2 = AReader::new_from_std(&runtime2, StdHandle::Stdin);
    let writer2 = AWriter::new_from_std(&runtime2, StdHandle::Stdout);

    match cat::execute(reader2, writer2) {
        Ok(()) => {}
        Err(e) => eprintln!("Error: {e}"),
    }
}
