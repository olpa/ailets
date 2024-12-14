use lazy_static::lazy_static;
use messages_to_markdown::messages_to_markdown;
use std::sync::Mutex;

struct MockReadFile {
    buffer: Vec<u8>,
    pos: usize,
}

lazy_static! {
    static ref MOCK_WRITE_FILE: Mutex<Vec<u8>> = Mutex::new(Vec::new());
    static ref MOCK_READ_FILE: Mutex<MockReadFile> = Mutex::new(MockReadFile {
        buffer: Vec::new(),
        pos: 0,
    });
}

fn clear_mocks() {
    let mut file = MOCK_WRITE_FILE.lock().unwrap();
    file.clear();
}

pub fn set_input(inputs: &[&str]) {
    let mut file = MOCK_READ_FILE.lock().unwrap();
    file.buffer.clear();
    for input in inputs {
        file.buffer.extend_from_slice(input.as_bytes());
    }
    file.pos = 0;
}

#[no_mangle]
pub extern "C" fn n_of_streams(_name_ptr: *const u8) -> u32 {
    1
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
    let buffer: &mut [u8] = unsafe { std::slice::from_raw_parts_mut(buffer_ptr, count as usize) };
    let bytes_to_copy = std::cmp::min(count as usize, file.buffer.len() - file.pos);
    buffer[..bytes_to_copy].copy_from_slice(&file.buffer[file.pos..file.pos + bytes_to_copy]);
    file.pos += bytes_to_copy;
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

#[test]
fn test_two_messages() {
    clear_mocks();
    let json_data = r#"
    {
        "role":"assistant", 
        "content":[
            {"type":"text", "text":"First message"}
        ]
    }
    {
        "role":"assistant",
        "content":[
            {"type":"text", "text":"Second message"},
            {"type":"text", "text":"Extra text"}
        ]
    }"#;
    set_input(&[json_data]);

    messages_to_markdown();

    let file = MOCK_WRITE_FILE.lock().unwrap();
    assert_eq!(&*file, b"First message\n\nSecond message\n\nExtra text");
}

#[test]
fn test_empty_input() {
    clear_mocks();
    let json_data = "";
    set_input(&[json_data]);

    messages_to_markdown();

    let file = MOCK_WRITE_FILE.lock().unwrap();
    assert_eq!(&*file, b"");
}

#[test]
fn test_long_text() {
    clear_mocks();
    // Create a 4KB text string
    let long_text = "x".repeat(4096);
    let json_data = format!(
        r#"
    {{
        "role":"assistant",
        "content":[
            {{"type":"text", "text":"{}"}}
        ]
    }}"#,
        long_text
    );
    set_input(&[&json_data]);

    messages_to_markdown();

    let file = MOCK_WRITE_FILE.lock().unwrap();
    assert_eq!(&*file, long_text.as_bytes());
}
