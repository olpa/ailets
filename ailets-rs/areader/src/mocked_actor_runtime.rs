#![allow(clippy::pedantic)] // FIXME
#![allow(clippy::not_unsafe_ptr_arg_deref)] // FIXME
#![allow(clippy::unnecessary_cast)] // FIXME

use lazy_static::lazy_static;

use std::sync::Mutex;

struct VfsFile {
    name: String,
    buffer: Vec<u8>,
}

struct FileHandle {
    vfs_index: usize,
    pos: usize,
}

struct TestFixture {
    files: Vec<VfsFile>,
    read_handles: Vec<FileHandle>,
}

lazy_static! {
    static ref FIXTURE: Mutex<TestFixture> = Mutex::new(TestFixture {
        files: Vec::new(),
        read_handles: Vec::new(),
    });
}

pub fn clear_mocks() {
    let mut fixture = FIXTURE.lock().unwrap();
    fixture.files.clear();
    fixture.read_handles.clear();
}

pub fn add_file(name: String, buffer: Vec<u8>) {
    let mut fixture = FIXTURE.lock().unwrap();
    fixture.files.push(VfsFile {
        name,
        buffer,
    });
}


#[no_mangle]
pub extern "C" fn n_of_streams(_name_ptr: *const u8) -> u32 {
    0
}

#[no_mangle]
pub extern "C" fn open_read(_name_ptr: *const u8, _index: u32) -> u32 {
    -1
}

#[no_mangle]
pub extern "C" fn open_write(_name_ptr: *const u8) -> u32 {
    -1
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
