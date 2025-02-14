use lazy_static::lazy_static;
use std::ffi::CStr;
use std::sync::Mutex;

struct VfsFile {
    name: String,
    buffer: Vec<u8>,
}

struct FileHandle {
    vfs_index: usize,
    pos: usize,
}

lazy_static! {
    static ref FILES: Mutex<Vec<VfsFile>> = Mutex::new(Vec::new());
    static ref HANDLES: Mutex<Vec<FileHandle>> = Mutex::new(Vec::new());
}

pub fn clear_mocks() {
    let mut files = FILES.lock().unwrap();
    files.clear();
    let mut handles = HANDLES.lock().unwrap();
    handles.clear();
}

pub fn add_file(name: String, buffer: Vec<u8>) {
    let mut files = FILES.lock().unwrap();
    files.push(VfsFile {
        name,
        buffer,
    });
}


#[no_mangle]
pub extern "C" fn n_of_streams(_name_ptr: *const u8) -> u32 {
    0
}

#[no_mangle]
pub extern "C" fn open_read(name_ptr: *const u8, index: usize) -> i32 {
    let mut files = FILES.lock().unwrap();
    let mut handles = HANDLES.lock().unwrap();
    
    let raw_name = unsafe { CStr::from_ptr(name_ptr.cast::<i8>()) };
    let name = raw_name.to_string_lossy();

    let name = format!("{name}_{index}");

    if let Some(vfs_index) = files.iter().position(|f| f.name == name) {
        let handle = FileHandle {
            vfs_index,
            pos: 0,
        };
        handles.push(handle);
        return i32::try_from(handles.len()).unwrap_or(-1) - 1;
    }

    -1
}

#[no_mangle]
pub extern "C" fn open_write(_name_ptr: *const u8) -> i32 {
    -1
}

#[no_mangle]
pub extern "C" fn aread(fd: usize, buffer_ptr: *mut u8, count: usize) -> i32 {
    let mut files = FILES.lock().unwrap();
    let mut handles = HANDLES.lock().unwrap();
    
    let Some(handle) = handles.get_mut(fd) else { return -1 };
    let Some(file) = files.get(handle.vfs_index) else { return -1 };
    
    let buffer = unsafe { std::slice::from_raw_parts_mut(buffer_ptr, count) };
    let pos_before = handle.pos;
    let remaining = file.buffer.len() - pos_before;
    let to_copy = std::cmp::min(count, remaining);

    for b in buffer.iter_mut().take(to_copy) {
        *b = file.buffer[handle.pos];
        handle.pos += 1;
    }

    handle.pos as i32 - pos_before as i32
}

#[no_mangle]
pub extern "C" fn awrite(fd: usize, buffer_ptr: *mut u8, count: usize) -> i32 {
    let mut files = FILES.lock().unwrap();
    let mut handles = HANDLES.lock().unwrap();
    
    let vfs_index = if let Some(handle) = handles.get(fd) {
        handle.vfs_index
    } else {
        return -1
    };
    let Some(file) = files.get_mut(vfs_index) else { return -1 };
    
    let buffer = unsafe { std::slice::from_raw_parts(buffer_ptr, count as usize) };

    for i in 0..count as usize {
        file.buffer.push(buffer[i]);
    }

    count as i32
}

#[no_mangle]
pub extern "C" fn aclose(fd: usize) -> i32 {
    let mut handles = HANDLES.lock().unwrap();
    
    let Some(handle) = handles.get_mut(fd) else { return -1 };
    handle.vfs_index = usize::MAX;
    0
}
