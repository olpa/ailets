//use jiter::{Jiter, NumberInt, Peek};

#[link(wasm_import_module = "")]
extern "C" {
//    // fn n_of_streams(name_ptr: *const u8) -> u32;
//    // fn open_read(name_ptr: *const u8, index: u32) -> u32;
    fn open_write(name_ptr: *const u8) -> u32;
//    // fn read(fd: u32, buffer_ptr: *mut u8, count: u32) -> u32;
    fn write(fd: u32, buffer_ptr: *const u8, count: u32) -> u32;
    fn close(fd: u32);
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

    let func_ptr: *const () = open_write as *const ();
    println!("!!!!!!!!!!!!!!!!!!!!!!!!! {:p}", func_ptr);
    //let json_data = r#"
    //{
    //    "role":"assistant",
    //    "content":[
    //        {"type":"text", "text":"Hello!"}
    //    ]
    //}"#;

    //let mut jiter = Jiter::new(json_data.as_bytes());
    //assert_eq!(jiter.next_object().unwrap(), Some("role"));
    //assert_eq!(jiter.next_str().unwrap(), "assistant");
    //assert_eq!(jiter.next_key().unwrap(), Some("content"));
    //assert_eq!(jiter.next_array().unwrap(), Some(Peek::String));

    let output_fd = unsafe { open_write(b"".as_ptr()) };
    println!("!!!!!!!!!!!!!!!!!!!!!!!!! output_fd: {}", output_fd);
    unsafe { write(output_fd, b"Hello!\n".as_ptr(), 6) };  // FIXME: write_all()
    unsafe { close(output_fd) };
}
