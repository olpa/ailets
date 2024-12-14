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
    ContentItem,
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
    let mut buffer = [0u8; BUFFER_SIZE as usize];

    let input_fd = unsafe { open_read(b"".as_ptr(), 0) };
    let bytes_read = unsafe { aread(input_fd, buffer.as_mut_ptr(), BUFFER_SIZE) };

    let mut jiter = Jiter::new(&buffer[..bytes_read as usize]);
    let mut level = Level::Top;
    let mut at_begin = true;

    loop {
        //
        // Top level
        //
        if level == Level::Top {
            if jiter.finish().is_ok() {
                break;
            }
            let peek = jiter.peek();
            assert!(peek.is_ok(), "Error: {peek:?}");
            assert!(peek == Ok(Peek::Object), "Expected object at top level");

            level = Level::Message;
            at_begin = true;
            // not continue, but fall-through
        }

        //
        // Message body level: loop through content items
        //

        if level == Level::Body {
            let next = if at_begin {
                jiter.next_array()
            } else {
                jiter.array_step()
            };
            println!("! message body next: {next:?}"); // FIXME
            assert!(next.is_ok(), "Error on the message body level: {next:?}");

            level = Level::ContentItem;
            at_begin = true;
            // not continue, but fall-through
        }

        //
        // Get the next object key
        //
        let next = if at_begin {
            jiter.next_object()
        } else {
            jiter.next_key()
        };
        println!("! top level next: {next:?} in level: {level:?}");
        at_begin = false;

        //
        // End of object: level up
        //
        let key = next.unwrap();
        if key.is_none() {
            if level == Level::ContentItem {
                level = Level::Body;
            } else if level == Level::Body {
                level = Level::Message;
            } else if level == Level::Message {
                level = Level::Top;
            } else {
                panic!("Unexpected level {level:?}");
            }
            at_begin = false;
            continue;
        }
        let key = key.unwrap();

        //
        // Message level: loop through content items
        //
        if level == Level::Message {
            if key != "content" {
                jiter.next_skip().unwrap();
                continue;
            }

            level = Level::Body;
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
