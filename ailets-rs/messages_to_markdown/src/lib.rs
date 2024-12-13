use jiter::{Jiter, Peek};

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
enum Level {
    Top,
    Message,
    Body,
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
    let mut level = Level::Top;
    let mut at_begin = true;

    loop {
        if level == Level::Top {
            if jiter.finish().is_ok() {
                break;
            }
            let peek = jiter.peek();
            println!("! top level peek: {peek:?}");
            if peek.is_err() {
                panic!("Error: {peek:?}");
            }
            if peek != Ok(Peek::Object) {
                panic!("Expected object at top level");
            }
            // level = Level::Message;
            let consumed = jiter.next_value();
            println!("! top level consumed: {consumed:?}");
            continue;
        }

        let next = if at_begin { jiter.next_object() } else { jiter.next_key() };
        println!("! top level next: {next:?} in level: {level:?}");
        at_begin = false;

        if next.is_err() {
            println!("Error: {next:?}");  // FIXME
            break;
        }
        let key = next.unwrap();

        if level == Level::Top {
            level = Level::Message;
        }
        if level == Level::Message {
            if key.is_none() {
                level = Level::Top;
                continue;
            }
            if key.unwrap() != "content" {
                jiter.next_skip().unwrap();
                continue;
            }
            level = Level::Body;
            // Get first array item
            let next2 = jiter.next_array();
            println!("! inside body next2: {next2:?}");

            let next2a = jiter.next_value();
            println!("! inside body next2a: {next2a:?}");

            // Iterate through remaining array items
            loop {
                let step = jiter.array_step();
                println!("! array step: {step:?}");
                if step.is_err() {
                    println!("Error: {step:?}");  // FIXME
                    break;
                }

                let next3a = jiter.next_value();
                println!("! inside body next3a: {next3a:?}");
            }

            let next3 = jiter.next_array();
            println!("! inside body next3: {next3:?}");
            at_begin = true;
            continue;
        }
        panic!("Unexpected level: {level:?}");
    }

    let output_fd = unsafe { open_write(b"".as_ptr()) };
    println!("!!!!!!!!!!!!!!!!!!!!!!!!! output_fd: {output_fd}");
    unsafe { awrite(output_fd, b"Hello!\n".as_ptr(), 7) }; // FIXME: write_all()
    unsafe { aclose(output_fd) };
}
