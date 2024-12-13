use lazy_static::lazy_static;
use messages_to_markdown::messages_to_markdown;
use std::sync::Mutex;

lazy_static! {
    static ref MOCK_FILES: Mutex<Vec<Vec<u8>>> = Mutex::new(Vec::new());
}

fn clear_mocks() {
    MOCK_FILES.lock().unwrap().clear();
}

#[no_mangle]
pub extern "C" fn open_write(_name_ptr: *const u8) -> u32 {
    clear_mocks();
    let mut files = MOCK_FILES.lock().unwrap();
    files.push(Vec::new());
    files.len() as u32 - 1
}

#[no_mangle]
pub extern "C" fn awrite(fd: u32, buffer_ptr: *const u8, count: u32) -> u32 {
    let mut files = MOCK_FILES.lock().unwrap();
    let buffer = unsafe { std::slice::from_raw_parts(buffer_ptr, count as usize) };
    files[fd as usize].extend_from_slice(buffer);
    0
}

#[no_mangle]
pub extern "C" fn aclose(_fd: u32) {}

#[test]
fn test_basic_conversion() {
    clear_mocks();

    messages_to_markdown();

    let files = MOCK_FILES.lock().unwrap();
    let written = files.get(0).unwrap();
    assert_eq!(written, b"Hello!\n");
}
