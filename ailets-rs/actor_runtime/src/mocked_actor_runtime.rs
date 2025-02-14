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
    handles: Vec<FileHandle>,
}

lazy_static! {
    static ref FIXTURE: Mutex<TestFixture> = Mutex::new(TestFixture {
        files: Vec::new(),
        handles: Vec::new(),
    });
}

pub fn clear_mocks() {
    let mut fixture = FIXTURE.lock().unwrap();
    fixture.files.clear();
    fixture.handles.clear();
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
    let fixture = FIXTURE.lock().unwrap();
    
    let handle = match fixture.handles.get(_fd as usize) {
        Some(h) => h,
        None => return -1,
    };

    let file = match fixture.files.get(handle.vfs_index) {
        Some(f) => f,
        None => return -1,
    };
    
    let buffer = unsafe { std::slice::from_raw_parts_mut(buffer_ptr, count as usize) };
    let remaining = file.buffer.len() - handle.offset;
    let to_copy = std::cmp::min(count as usize, remaining);

    for i in 0..to_copy {
        buffer[i] = file.buffer[handle.offset + i];
    }

    to_copy as u32
}

#[no_mangle]
pub extern "C" fn awrite(_fd: u32, buffer_ptr: *mut u8, count: u32) -> u32 {
    let mut fixture = FIXTURE.lock().unwrap();
    
    let handle = match fixture.handles.get(_fd as usize) {
        Some(h) => h,
        None => return -1,
    };

    let file = match fixture.files.get_mut(handle.vfs_index) {
        Some(f) => f,
        None => return -1,
    };
    
    let buffer = unsafe { std::slice::from_raw_parts(buffer_ptr, count as usize) };

    for i in 0..count as usize {
        file.buffer[handle.offset + i] = buffer[i];
    }

    count
}

#[no_mangle]
pub extern "C" fn aclose(_fd: u32) -> i32 {
    let mut fixture = FIXTURE.lock().unwrap();
    
    match fixture.handles.get_mut(_fd as usize) {
        Some(handle) => {
            handle.vfs_index = -1;
            0
        },
        None => -1,
    }
}
