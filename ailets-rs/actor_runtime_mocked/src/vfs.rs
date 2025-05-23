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
use std::os::raw::{c_char, c_int, c_uint};
use std::sync::atomic::{AtomicI32, Ordering};
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

static IO_ERRNO: AtomicI32 = AtomicI32::new(0);

pub const WANT_ERROR: char = '\u{0001}';
pub const IO_INTERRUPT: char = '\n';

#[allow(clippy::missing_panics_doc)]
#[allow(clippy::unwrap_used)]
pub fn clear_mocks() {
    IO_ERRNO.store(0, Ordering::Relaxed);
    let mut files = FILES.lock().unwrap();
    files.clear();
    let mut handles = HANDLES.lock().unwrap();
    handles.clear();
}

#[allow(clippy::missing_panics_doc)]
#[allow(clippy::unwrap_used)]
pub fn add_file(name: String, buffer: Vec<u8>) {
    let mut files = FILES.lock().unwrap();
    files.push(VfsFile { name, buffer });
}

#[allow(clippy::missing_errors_doc)]
#[allow(clippy::unwrap_used)]
pub fn get_file(name: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    IO_ERRNO.store(0, Ordering::Relaxed);
    let files = FILES.lock()?;
    files
        .iter()
        .find(|f| f.name == name)
        .map(|f| f.buffer.clone())
        .ok_or_else(|| {
            IO_ERRNO.store(-1, Ordering::Relaxed);
            format!("File not found: {name}").into()
        })
}

fn cstr_to_string(ptr: *const c_char) -> String {
    unsafe { CStr::from_ptr(ptr) }.to_string_lossy().to_string()
}

#[no_mangle]
#[allow(clippy::missing_panics_doc)]
#[allow(clippy::unwrap_used)]
pub extern "C" fn open_read(name_ptr: *const c_char) -> c_int {
    IO_ERRNO.store(0, Ordering::Relaxed);
    let files = FILES.lock().unwrap();
    let mut handles = HANDLES.lock().unwrap();

    let name = cstr_to_string(name_ptr);

    if let Some(vfs_index) = files.iter().position(|f| f.name == name) {
        let handle = FileHandle { vfs_index, pos: 0 };
        handles.push(handle);
        return c_int::try_from(handles.len()).unwrap_or(-1) - 1;
    }

    IO_ERRNO.store(-1, Ordering::Relaxed);
    -1
}

#[no_mangle]
#[allow(clippy::missing_panics_doc)]
#[allow(clippy::unwrap_used)]
pub extern "C" fn open_write(name_ptr: *const c_char) -> c_int {
    IO_ERRNO.store(0, Ordering::Relaxed);
    let mut files = FILES.lock().unwrap();
    let mut handles = HANDLES.lock().unwrap();

    let name = cstr_to_string(name_ptr);
    if name.contains(WANT_ERROR) {
        IO_ERRNO.store(-1, Ordering::Relaxed);
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

    c_int::try_from(handle_index).unwrap_or_else(|_| {
        IO_ERRNO.store(-1, Ordering::Relaxed);
        -1
    })
}

fn cbuf_to_slice<'a>(ptr: *mut u8, count: usize) -> &'a mut [u8] {
    unsafe { std::slice::from_raw_parts_mut(ptr, count) }
}

#[no_mangle]
#[allow(clippy::missing_panics_doc)]
#[allow(clippy::unwrap_used)]
pub extern "C" fn aread(fd: c_int, buffer_ptr: *mut u8, count: c_uint) -> c_int {
    IO_ERRNO.store(0, Ordering::Relaxed);
    let files = FILES.lock().unwrap();
    let mut handles = HANDLES.lock().unwrap();

    let Ok(fd) = usize::try_from(fd) else {
        IO_ERRNO.store(-1, Ordering::Relaxed);
        return -1;
    };
    let Some(handle) = handles.get_mut(fd) else {
        IO_ERRNO.store(-1, Ordering::Relaxed);
        return -1;
    };
    let Some(file) = files.get(handle.vfs_index) else {
        IO_ERRNO.store(-1, Ordering::Relaxed);
        return -1;
    };

    let buffer = cbuf_to_slice(buffer_ptr, count as usize);
    let pos_before = handle.pos;
    let remaining = file.buffer.len() - pos_before;
    let to_copy = std::cmp::min(count as usize, remaining);

    for b in buffer.iter_mut().take(to_copy) {
        #[allow(clippy::indexing_slicing)]
        let ch = file.buffer[handle.pos];
        if ch == WANT_ERROR as u8 {
            IO_ERRNO.store(-1, Ordering::Relaxed);
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
#[allow(clippy::unwrap_used)]
pub extern "C" fn awrite(fd: c_int, buffer_ptr: *mut u8, count: c_uint) -> c_int {
    IO_ERRNO.store(0, Ordering::Relaxed);
    let mut files = FILES.lock().unwrap();
    let handles = HANDLES.lock().unwrap();

    let Ok(fd) = usize::try_from(fd) else {
        IO_ERRNO.store(-1, Ordering::Relaxed);
        return -1;
    };
    let Some(handle) = handles.get(fd) else {
        IO_ERRNO.store(-1, Ordering::Relaxed);
        return -1;
    };
    let Some(file) = files.get_mut(handle.vfs_index) else {
        IO_ERRNO.store(-1, Ordering::Relaxed);
        return -1;
    };

    let buffer = cbuf_to_slice(buffer_ptr, count as usize);
    let len_before = file.buffer.len();

    for &ch in buffer.iter().take(count as usize) {
        if ch == WANT_ERROR as u8 {
            IO_ERRNO.store(-1, Ordering::Relaxed);
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
#[allow(clippy::unwrap_used)]
pub extern "C" fn aclose(fd: c_int) -> c_int {
    IO_ERRNO.store(0, Ordering::Relaxed);
    let mut handles = HANDLES.lock().unwrap();

    let Ok(fd) = usize::try_from(fd) else {
        IO_ERRNO.store(-1, Ordering::Relaxed);
        return -1;
    };
    let Some(handle) = handles.get_mut(fd) else {
        IO_ERRNO.store(-1, Ordering::Relaxed);
        return -1;
    };
    handle.vfs_index = usize::MAX;
    0
}

#[no_mangle]
#[must_use]
pub extern "C" fn get_errno() -> c_int {
    IO_ERRNO.load(Ordering::Relaxed)
}
