use lazy_static::lazy_static;

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

pub fn clear_mocks() {
    let mut file = MOCK_WRITE_FILE.lock().unwrap();
    file.clear();
}

#[allow(dead_code)]
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

pub fn get_output() -> String {
    let file = MOCK_WRITE_FILE.lock().unwrap();
    String::from_utf8(file.to_vec()).unwrap()
}
