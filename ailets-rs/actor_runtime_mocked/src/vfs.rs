/// Mocked actor runtime and a virtual file system.
/// Use the feature `+mocked` to enable this module.
///
/// - `clear_mocks` clears the mocks.
/// - `add_file` adds a file to the virtual file system.
/// - `get_file` gets the content of a file from the virtual file system.
/// - `WANT_ERROR` is a character that can be used to simulate an error.
/// - `IO_INTERRUPT` is a character that can be used to simulate an interrupt.
///
/// `open_read(name, index)`:
/// - expects a file named `name.index` in the virtual file system.
///
/// `open_write(name)`:
/// - returns an error if `name` contains `WANT_ERROR`.
///
/// `aread`, `awrite`:
/// - stops on `IO_INTERRUPT` or `WANT_ERROR`.
/// - return an error if `WANT_ERROR` is encountered.
use lazy_static::lazy_static;
use std::ffi::CStr;
use std::os::raw::c_char;
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

pub const WANT_ERROR: char = '\u{0001}';
pub const IO_INTERRUPT: char = '\n';

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

fn cstr_to_string(ptr: *const c_char) -> String {
    unsafe { CStr::from_ptr(ptr) }.to_string_lossy().to_string()
}

#[no_mangle]
#[allow(clippy::missing_panics_doc)]
pub extern "C" fn n_of_streams(name_ptr: *const c_char) -> i32 {
    let files = FILES.lock().unwrap();

    let name = cstr_to_string(name_ptr);
    if name.contains(WANT_ERROR) {
        return -1;
    }

    let mut count = 0;
    while files.iter().any(|f| f.name == format!("{name}.{count}")) {
        count += 1;
    }
    count
}

#[no_mangle]
#[allow(clippy::missing_panics_doc)]
pub extern "C" fn open_read(name_ptr: *const c_char, index: usize) -> i32 {
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
pub extern "C" fn open_write(name_ptr: *const c_char) -> i32 {
    let mut files = FILES.lock().unwrap();
    let mut handles = HANDLES.lock().unwrap();

    let name = cstr_to_string(name_ptr);
    if name.contains(WANT_ERROR) {
        return -1;
    }

    files.push(VfsFile {
        name,
        buffer: Vec::new(),
    });
    let vfs_index = files.len() - 1;

    let handle = FileHandle { vfs_index, pos: 0 };
    handles.push(handle);
    let handle_index = handles.len() - 1;

    i32::try_from(handle_index).unwrap_or(-1)
}

fn cbuf_to_slice<'a>(ptr: *mut u8, count: usize) -> &'a mut [u8] {
    unsafe { std::slice::from_raw_parts_mut(ptr, count) }
}

#[no_mangle]
#[allow(clippy::missing_panics_doc)]
pub extern "C" fn aread(fd: i32, buffer_ptr: *mut u8, count: usize) -> i32 {
    let files = FILES.lock().unwrap();
    let mut handles = HANDLES.lock().unwrap();

    let Ok(fd) = usize::try_from(fd) else {
        return -1;
    };
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
        let ch = file.buffer[handle.pos];
        if ch == WANT_ERROR as u8 {
            return -1;
        }
        *b = ch;
        handle.pos += 1;
        if ch == IO_INTERRUPT as u8 {
            break;
        }
    }

    (handle.pos - pos_before).try_into().unwrap()
}

#[no_mangle]
#[allow(clippy::missing_panics_doc)]
pub extern "C" fn awrite(fd: i32, buffer_ptr: *mut u8, count: usize) -> i32 {
    let mut files = FILES.lock().unwrap();
    let handles = HANDLES.lock().unwrap();

    let Ok(fd) = usize::try_from(fd) else {
        return -1;
    };
    let Some(handle) = handles.get(fd) else {
        return -1;
    };
    let Some(file) = files.get_mut(handle.vfs_index) else {
        return -1;
    };

    let buffer = cbuf_to_slice(buffer_ptr, count);
    let len_before = file.buffer.len();

    for &ch in buffer.iter().take(count) {
        if ch == WANT_ERROR as u8 {
            return -1;
        }
        file.buffer.push(ch);
        if ch == IO_INTERRUPT as u8 {
            break;
        }
    }

    let len_after = file.buffer.len();
    let bytes_written = len_after - len_before;

    bytes_written.try_into().unwrap()
}

#[no_mangle]
#[allow(clippy::missing_panics_doc)]
pub extern "C" fn aclose(fd: i32) -> i32 {
    let mut handles = HANDLES.lock().unwrap();

    let Ok(fd) = usize::try_from(fd) else {
        return -1;
    };
    let Some(handle) = handles.get_mut(fd) else {
        return -1;
    };
    handle.vfs_index = usize::MAX;
    0
}
