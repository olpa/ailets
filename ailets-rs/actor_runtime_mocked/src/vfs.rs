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

pub struct Vfs {
    files: Mutex<Vec<VfsFile>>,
    handles: Mutex<Vec<FileHandle>>,
    io_errno: AtomicI32,
}

pub const WANT_ERROR: char = '\u{0001}';
pub const IO_INTERRUPT: char = '\n';

impl Default for Vfs {
    fn default() -> Self {
        Self::new()
    }
}

impl Vfs {
    #[must_use]
    pub fn new() -> Self {
        Self {
            files: Mutex::new(Vec::new()),
            handles: Mutex::new(Vec::new()),
            io_errno: AtomicI32::new(0),
        }
    }

    #[allow(clippy::missing_panics_doc)]
    #[allow(clippy::unwrap_used)]
    pub fn clear_mocks(&self) {
        self.io_errno.store(0, Ordering::Relaxed);
        let mut files = self.files.lock().unwrap();
        files.clear();
        let mut handles = self.handles.lock().unwrap();
        handles.clear();
    }

    #[allow(clippy::missing_panics_doc)]
    #[allow(clippy::unwrap_used)]
    pub fn add_file(&self, name: String, buffer: Vec<u8>) {
        let mut files = self.files.lock().unwrap();
        files.push(VfsFile { name, buffer });
    }

    /// Update an existing file by appending data to it
    /// # Errors
    /// - File not found
    #[allow(clippy::missing_panics_doc)]
    #[allow(clippy::unwrap_used)]
    pub fn append_to_file(&self, name: &str, data: &[u8]) -> Result<(), String> {
        let mut files = self.files.lock().unwrap();
        if let Some(file) = files.iter_mut().find(|f| f.name == name) {
            file.buffer.extend_from_slice(data);
            Ok(())
        } else {
            Err(format!("File {name} not found for appending"))
        }
    }

    #[allow(clippy::missing_errors_doc)]
    #[allow(clippy::unwrap_used)]
    pub fn get_file(&self, name: &str) -> Result<Vec<u8>, Box<dyn std::error::Error + '_>> {
        self.io_errno.store(0, Ordering::Relaxed);
        let files = self.files.lock()?;
        files
            .iter()
            .find(|f| f.name == name)
            .map(|f| f.buffer.clone())
            .ok_or_else(|| {
                self.io_errno.store(-1, Ordering::Relaxed);
                format!("File not found: {name}").into()
            })
    }

    #[allow(clippy::missing_panics_doc)]
    #[allow(clippy::unwrap_used)]
    pub fn open_read(&self, name_ptr: *const c_char) -> c_int {
        self.io_errno.store(0, Ordering::Relaxed);
        let files = self.files.lock().unwrap();
        let mut handles = self.handles.lock().unwrap();

        let name = cstr_to_string(name_ptr);

        if let Some(vfs_index) = files.iter().position(|f| f.name == name) {
            let handle = FileHandle { vfs_index, pos: 0 };
            handles.push(handle);
            return c_int::try_from(handles.len()).unwrap_or(-1) - 1;
        }

        self.io_errno.store(2, Ordering::Relaxed); // ENOENT - No such file or directory
        -1
    }

    #[allow(clippy::missing_panics_doc)]
    #[allow(clippy::unwrap_used)]
    pub fn open_write(&self, name_ptr: *const c_char) -> c_int {
        self.io_errno.store(0, Ordering::Relaxed);
        let mut files = self.files.lock().unwrap();
        let mut handles = self.handles.lock().unwrap();

        let name = cstr_to_string(name_ptr);
        if name.contains(WANT_ERROR) {
            self.io_errno.store(22, Ordering::Relaxed); // EINVAL - Invalid argument
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
            self.io_errno.store(-1, Ordering::Relaxed);
            -1
        })
    }

    #[allow(clippy::missing_panics_doc)]
    #[allow(clippy::unwrap_used)]
    pub fn aread(&self, fd: c_int, buffer_ptr: *mut u8, count: c_uint) -> c_int {
        self.io_errno.store(0, Ordering::Relaxed);
        let files = self.files.lock().unwrap();
        let mut handles = self.handles.lock().unwrap();

        let Ok(fd) = usize::try_from(fd) else {
            self.io_errno.store(9, Ordering::Relaxed); // EBADF - Bad file descriptor
            return -1;
        };
        let Some(handle) = handles.get_mut(fd) else {
            self.io_errno.store(9, Ordering::Relaxed); // EBADF - Bad file descriptor
            return -1;
        };
        let Some(file) = files.get(handle.vfs_index) else {
            self.io_errno.store(9, Ordering::Relaxed); // EBADF - Bad file descriptor
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
                self.io_errno.store(5, Ordering::Relaxed); // EIO - I/O error
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

    #[allow(clippy::missing_panics_doc)]
    #[allow(clippy::unwrap_used)]
    pub fn awrite(&self, fd: c_int, buffer_ptr: *mut u8, count: c_uint) -> c_int {
        self.io_errno.store(0, Ordering::Relaxed);
        let mut files = self.files.lock().unwrap();
        let handles = self.handles.lock().unwrap();

        let Ok(fd) = usize::try_from(fd) else {
            self.io_errno.store(9, Ordering::Relaxed); // EBADF - Bad file descriptor
            return -1;
        };
        let Some(handle) = handles.get(fd) else {
            self.io_errno.store(9, Ordering::Relaxed); // EBADF - Bad file descriptor
            return -1;
        };
        let Some(file) = files.get_mut(handle.vfs_index) else {
            self.io_errno.store(9, Ordering::Relaxed); // EBADF - Bad file descriptor
            return -1;
        };

        let buffer = cbuf_to_slice(buffer_ptr, count as usize);
        let len_before = file.buffer.len();

        for &ch in buffer.iter().take(count as usize) {
            if ch == WANT_ERROR as u8 {
                self.io_errno.store(5, Ordering::Relaxed); // EIO - I/O error
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

    #[allow(clippy::missing_panics_doc)]
    #[allow(clippy::unwrap_used)]
    pub fn aclose(&self, fd: c_int) -> c_int {
        self.io_errno.store(0, Ordering::Relaxed);
        let mut handles = self.handles.lock().unwrap();

        let Ok(fd) = usize::try_from(fd) else {
            self.io_errno.store(9, Ordering::Relaxed); // EBADF - Bad file descriptor
            return -1;
        };
        let Some(handle) = handles.get_mut(fd) else {
            self.io_errno.store(9, Ordering::Relaxed); // EBADF - Bad file descriptor
            return -1;
        };
        handle.vfs_index = usize::MAX;
        0
    }

    #[must_use]
    pub fn get_errno(&self) -> c_int {
        self.io_errno.load(Ordering::Relaxed)
    }

    #[allow(clippy::missing_panics_doc)]
    #[allow(clippy::unwrap_used)]
    pub fn dag_value_node(&self, value_ptr: *const u8, _explain_ptr: *const c_char) -> c_int {
        self.io_errno.store(0, Ordering::Relaxed);
        let mut files = self.files.lock().unwrap();

        // Extract the content from the value pointer (assuming it's a C string)
        let content = if value_ptr.is_null() {
            Vec::new()
        } else {
            unsafe { CStr::from_ptr(value_ptr.cast::<c_char>()) }
                .to_bytes()
                .to_vec()
        };

        // Create a file named "value.N" where N is the future handle
        let handle = files.len();
        let name = format!("value.{handle}");

        files.push(VfsFile {
            name,
            buffer: content,
        });

        c_int::try_from(handle).unwrap_or_else(|_| {
            self.io_errno.store(-1, Ordering::Relaxed);
            -1
        })
    }
}

fn cstr_to_string(ptr: *const c_char) -> String {
    unsafe { CStr::from_ptr(ptr) }.to_string_lossy().to_string()
}

fn cbuf_to_slice<'a>(ptr: *mut u8, count: usize) -> &'a mut [u8] {
    unsafe { std::slice::from_raw_parts_mut(ptr, count) }
}
