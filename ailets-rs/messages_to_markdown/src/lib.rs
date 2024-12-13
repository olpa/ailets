//use jiter::{Jiter, NumberInt, Peek};

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
    let json_data = std::str::from_utf8(&buffer[..bytes_read as usize]).unwrap();
    println!("!!!!!!!!!!!!!!!!!!!!!!!!! json_data: {json_data}");

    //let mut jiter = Jiter::new(json_data.as_bytes());
    //assert_eq!(jiter.next_object().unwrap(), Some("role"));
    //assert_eq!(jiter.next_str().unwrap(), "assistant");
    //assert_eq!(jiter.next_key().unwrap(), Some("content"));
    //assert_eq!(jiter.next_array().unwrap(), Some(Peek::String));

    let output_fd = unsafe { open_write(b"".as_ptr()) };
    println!("!!!!!!!!!!!!!!!!!!!!!!!!! output_fd: {output_fd}");
    unsafe { awrite(output_fd, b"Hello!\n".as_ptr(), 7) }; // FIXME: write_all()
    unsafe { aclose(output_fd) };
}
