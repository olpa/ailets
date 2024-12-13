use lazy_static::lazy_static;
use messages_to_markdown::messages_to_markdown;
use std::sync::Mutex;

lazy_static! {
    static ref MOCK_WRITE_FILE: Mutex<Vec<u8>> = Mutex::new(Vec::new());
}

fn clear_mocks() {
    MOCK_WRITE_FILE.lock().unwrap().clear();
}

#[no_mangle]
pub extern "C" fn open_write(_name_ptr: *const u8) -> u32 {
    0
}

#[no_mangle]
pub extern "C" fn awrite(_fd: u32, buffer_ptr: *const u8, count: u32) -> u32 {
    let mut file = MOCK_WRITE_FILE.lock().unwrap();
    let buffer = unsafe { std::slice::from_raw_parts(buffer_ptr, count as usize) };
    file.extend_from_slice(buffer);
    0
}

#[no_mangle]
pub extern "C" fn aclose(_fd: u32) {}

#[test]
fn test_basic_conversion() {
    clear_mocks();

    messages_to_markdown();

    let file = MOCK_WRITE_FILE.lock().unwrap();
    assert_eq!(&*file, b"Hello!\n");
}
