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

#[allow(clippy::missing_panics_doc)]
pub fn clear_mocks() {
    let mut files = FILES.lock().unwrap();
    files.clear();
    let mut handles = HANDLES.lock().unwrap();
    handles.clear();
}

#[allow(clippy::missing_panics_doc)]
pub fn add_file(name: String, buffer: Vec<u8>) {
    let mut files = FILES.lock().unwrap();
    files.push(VfsFile { name, buffer });
}

#[allow(clippy::missing_errors_doc)]
pub fn get_file(name: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let files = FILES.lock()?;
    files
        .iter()
        .find(|f| f.name == name)
        .map(|f| f.buffer.clone())
        .ok_or(format!("File not found: {name}").into())
}

fn cstr_to_string(ptr: *const i8) -> String {
    unsafe { CStr::from_ptr(ptr.cast::<i8>()) }
        .to_string_lossy()
        .to_string()
}

#[no_mangle]
#[allow(clippy::missing_panics_doc)]
pub extern "C" fn n_of_streams(name_ptr: *const i8) -> u32 {
    let files = FILES.lock().unwrap();
    let name = cstr_to_string(name_ptr);

    let mut count = 0;
    while files.iter().any(|f| f.name == format!("{name}.{count}")) {
        count += 1;
    }
    count
}

#[no_mangle]
#[allow(clippy::missing_panics_doc)]
pub extern "C" fn open_read(name_ptr: *const i8, index: usize) -> i32 {
    let files = FILES.lock().unwrap();
    let mut handles = HANDLES.lock().unwrap();

    let name = cstr_to_string(name_ptr);
    let name = format!("{name}.{index}");

    if let Some(vfs_index) = files.iter().position(|f| f.name == name) {
        let handle = FileHandle { vfs_index, pos: 0 };
        handles.push(handle);
        return i32::try_from(handles.len()).unwrap_or(-1) - 1;
    }

    -1
}

#[no_mangle]
#[allow(clippy::missing_panics_doc)]
pub extern "C" fn open_write(_name_ptr: *const i8) -> i32 {
    -1
}

fn cbuf_to_slice<'a>(ptr: *mut u8, count: usize) -> &'a mut [u8] {
    unsafe { std::slice::from_raw_parts_mut(ptr, count) }
}

#[no_mangle]
#[allow(clippy::missing_panics_doc)]
pub extern "C" fn aread(fd: usize, buffer_ptr: *mut u8, count: usize) -> i32 {
    let files = FILES.lock().unwrap();
    let mut handles = HANDLES.lock().unwrap();

    let Some(handle) = handles.get_mut(fd) else {
        return -1;
    };
    let Some(file) = files.get(handle.vfs_index) else {
        return -1;
    };

    let buffer = cbuf_to_slice(buffer_ptr, count);
    let pos_before = handle.pos;
    let remaining = file.buffer.len() - pos_before;
    let to_copy = std::cmp::min(count, remaining);

    for b in buffer.iter_mut().take(to_copy) {
        *b = file.buffer[handle.pos];
        handle.pos += 1;
    }

    (handle.pos - pos_before).try_into().unwrap()
}

#[no_mangle]
#[allow(clippy::missing_panics_doc)]
pub extern "C" fn awrite(fd: usize, buffer_ptr: *mut u8, count: usize) -> i32 {
    let mut files = FILES.lock().unwrap();
    let handles = HANDLES.lock().unwrap();

    let vfs_index = if let Some(handle) = handles.get(fd) {
        handle.vfs_index
    } else {
        return -1;
    };
    let Some(file) = files.get_mut(vfs_index) else {
        return -1;
    };

    let buffer = cbuf_to_slice(buffer_ptr, count);

    for &b in buffer.iter().take(count) {
        file.buffer.push(b);
    }

    count.try_into().unwrap()
}

#[no_mangle]
#[allow(clippy::missing_panics_doc)]
pub extern "C" fn aclose(fd: usize) -> i32 {
    let mut handles = HANDLES.lock().unwrap();

    let Some(handle) = handles.get_mut(fd) else {
        return -1;
    };
    handle.vfs_index = usize::MAX;
    0
}
