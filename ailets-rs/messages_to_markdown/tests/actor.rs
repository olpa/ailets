use lazy_static::lazy_static;
use messages_to_markdown::messages_to_markdown;
use std::sync::Mutex;

struct MockReadFile {
    lines: Vec<Vec<u8>>,
    pos: usize,
}

lazy_static! {
    static ref MOCK_WRITE_FILE: Mutex<Vec<u8>> = Mutex::new(Vec::new());
    static ref MOCK_READ_FILE: Mutex<MockReadFile> = Mutex::new(MockReadFile {
        lines: Vec::new(),
        pos: 0,
    });
}

fn clear_mocks() {
    MOCK_WRITE_FILE.lock().unwrap().clear();
    MOCK_READ_FILE.lock().unwrap().lines.clear();
    MOCK_READ_FILE.lock().unwrap().pos = 0;
}

pub fn set_input(inputs: &[&str]) {
    let mut file = MOCK_READ_FILE.lock().unwrap();
    for input in inputs {
        file.lines.push(input.as_bytes().to_vec());
    }
    file.pos = 0;
}

#[no_mangle]
pub extern "C" fn open_read(_name_ptr: *const u8, _index: u32) -> u32 {
    0
}

#[no_mangle]
pub extern "C" fn open_write(_name_ptr: *const u8) -> u32 {
    0
}

#[no_mangle]
pub extern "C" fn aread(_fd: u32, buffer_ptr: *mut u8, count: u32) -> u32 {
    let mut file = MOCK_READ_FILE.lock().unwrap();
    if file.pos >= file.lines.len() {
        return 0;
    }
    let line = &file.lines[file.pos];
    if count < line.len() as u32 {
        panic!(
            "Buffer size {} is too small for line of length {}",
            count,
            line.len()
        );
    }
    let bytes_to_copy = std::cmp::min(count as usize, line.len());
    let buffer: &mut [u8] = unsafe { std::slice::from_raw_parts_mut(buffer_ptr, bytes_to_copy) };
    buffer[..bytes_to_copy].copy_from_slice(&line[..bytes_to_copy]);
    file.pos += 1;
    bytes_to_copy as u32
}

#[no_mangle]
pub extern "C" fn awrite(_fd: u32, buffer_ptr: *mut u8, count: u32) -> u32 {
    let mut file = MOCK_WRITE_FILE.lock().unwrap();
    let buffer = unsafe { std::slice::from_raw_parts(buffer_ptr, count as usize) };
    file.extend_from_slice(buffer);
    count as u32
}

#[no_mangle]
pub extern "C" fn aclose(_fd: u32) {}

#[test]
fn test_basic_conversion() {
    clear_mocks();
    let json_data = r#"
    {
        "role":"assistant",
        "content":[
            {"type":"text", "text":"Hello!"}
        ]
    }"#;
    set_input(&[json_data]);

    messages_to_markdown();

    let file = MOCK_WRITE_FILE.lock().unwrap();
    assert_eq!(&*file, b"Hello!");
}

#[test]
fn test_multiple_content_items() {
    clear_mocks();
    let json_data = r#"
    {
        "role":"assistant",
        "content":[
            {"type":"text", "text":"First item"},
            {"type":"text", "text":"Second item"},
            {"type":"text", "text":"Third item"}
        ]
    }"#;
    set_input(&[json_data]);

    messages_to_markdown();

    let file = MOCK_WRITE_FILE.lock().unwrap();
    assert_eq!(&*file, b"First item\n\nSecond item\n\nThird item");
}
