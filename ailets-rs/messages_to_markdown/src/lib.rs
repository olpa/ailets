use jiter::{Jiter};

#[link(wasm_import_module = "")]
extern "C" {
    //    // fn n_of_streams(name_ptr: *const u8) -> u32;
    fn open_read(name_ptr: *const u8, index: u32) -> u32;
    fn open_write(name_ptr: *const u8) -> u32;
    fn aread(fd: u32, buffer_ptr: *mut u8, count: u32) -> u32;
    fn awrite(fd: u32, buffer_ptr: *const u8, count: u32) -> u32;
    fn aclose(fd: u32);
}

const BUFFER_SIZE: u32 = 1024;

#[derive(Debug, PartialEq)]
enum State {
    TopLevel,
    InsideMessage,
    InsideBody,
    // InsideContentItem,
}

/// Converts a JSON message format to markdown.
///
/// # Panics
///
/// This function will panic if:
/// - The input JSON is malformed
/// - The JSON structure doesn't match the expected format of
///   ```
pub fn messages_to_markdown() {
    println!("!!!!!!!!!!!!!!!!!!!!!!!!! messages_to_markdown");
    let mut buffer = [0u8; BUFFER_SIZE as usize];

    let input_fd = unsafe { open_read(b"".as_ptr(), 0) };
    let bytes_read = unsafe { aread(input_fd, buffer.as_mut_ptr(), BUFFER_SIZE) };

    let mut jiter = Jiter::new(&buffer[..bytes_read as usize]);
    let mut state = State::TopLevel;
    let mut expect_object = true;

    loop {
        let next = if expect_object { jiter.next_object() } else { jiter.next_key() };
        println!("! top level next: {next:?} in state: {state:?}");
        expect_object = false;

        if next.is_err() {
            println!("Error: {next:?}");  // FIXME
            break;
        }
        let key = next.unwrap();

        if state == State::TopLevel {
            state = State::InsideMessage;
        }
        if state == State::InsideMessage {
            if key.is_none() {
                state = State::TopLevel;
                continue;
            }
            if key.unwrap() == "content" {
                state = State::InsideBody;
                expect_object = true;
                continue;
            }
        }
        panic!("Unexpected state: {state:?}");
    }

    let output_fd = unsafe { open_write(b"".as_ptr()) };
    println!("!!!!!!!!!!!!!!!!!!!!!!!!! output_fd: {output_fd}");
    unsafe { awrite(output_fd, b"Hello!\n".as_ptr(), 7) }; // FIXME: write_all()
    unsafe { aclose(output_fd) };
}
