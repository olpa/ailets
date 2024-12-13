use messages_to_markdown::messages_to_markdown;


#[no_mangle]
pub extern "C" fn open_write(_name_ptr: *const u8) -> u32 {
    println!("!!!!!!!!!!!!!!!!!!!!!!!!! open_write");
    0
}

#[no_mangle]
pub extern "C" fn awrite(_fd: u32, _buffer_ptr: *const u8, _count: u32) -> u32 {
    println!("!!!!!!!!!!!!!!!!!!!!!!!!! write");
    0
}

#[no_mangle]
pub extern "C" fn aclose(_fd: u32) {
    println!("!!!!!!!!!!!!!!!!!!!!!!!!! close");
}

#[test]
fn test_basic_conversion() {
    // use test_helpers::*;

    // clear_mocks();

    messages_to_markdown();

    // Get the written content from the mock file system
    //let written = MOCK_FILES.lock().unwrap()
    //    .get("")
    //    .expect("Output file should exist")
    //    .first()
    //    .expect("Output file should have content")
    //    .clone();

    //assert_eq!(written, b"Hello!\n");
}
