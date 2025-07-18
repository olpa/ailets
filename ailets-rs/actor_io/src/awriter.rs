//! A writer implementation for the actor runtime system.
//!
//! `AWriter` provides an implementation of the standard [`std::io::Write`] trait.
//! It manages a file descriptor internally and ensures proper cleanup through the Drop trait.
//!
//! # Example
//! ```no_run
//! use std::io::Write;
//! use actor_io::AWriter;
//!
//! let mut writer = AWriter::new(c"example.txt").unwrap();
//! writer.write_all(b"Hello, world!").unwrap();
//! writer.close().unwrap();
//! ```
//!
//! # Safety
//! This module uses unsafe code to interact with the actor runtime's C FFI interface.
//! The safety guarantees are maintained through proper file descriptor management
//! and automatic cleanup in the Drop implementation.

use std::ffi::CStr;
use std::os::raw::{c_int, c_uint};

use actor_runtime::{aclose, awrite, get_errno, open_write, StdHandle};

pub struct AWriter {
    fd: Option<c_int>,
}

impl AWriter {
    /// Create a new `AWriter` instance for the specified file.
    ///
    /// # Errors
    /// Returns an error if the file could not be created.
    pub fn new(filename: &CStr) -> std::io::Result<Self> {
        let fd = unsafe { open_write(filename.as_ptr()) };
        if fd < 0 {
            Err(std::io::Error::from_raw_os_error(unsafe { get_errno() }))
        } else {
            Ok(AWriter { fd: Some(fd) })
        }
    }

    /// Create a new `AWriter` instance for the given standard handle.
    #[must_use]
    pub fn new_from_std(handle: StdHandle) -> Self {
        Self {
            fd: Some(handle as c_int),
        }
    }

    /// Create a new `AWriter` instance from an existing file descriptor.
    ///
    /// # Errors
    /// Returns an error if the file descriptor is invalid (negative).
    pub fn new_from_fd(fd: c_int) -> std::io::Result<Self> {
        if fd < 0 {
            Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Bad handle",
            ))
        } else {
            Ok(AWriter { fd: Some(fd) })
        }
    }

    /// Close the writer.
    /// Can be called multiple times.
    /// "drop" will call "close" automatically.
    ///
    /// # Errors
    /// Returns an error if closing fails.
    pub fn close(&mut self) -> std::io::Result<()> {
        if let Some(fd) = self.fd {
            let result = unsafe { aclose(fd) };
            if result < 0 {
                return Err(std::io::Error::from_raw_os_error(unsafe { get_errno() }));
            }
            self.fd = None;
        }
        Ok(())
    }
}

impl Drop for AWriter {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

impl std::io::Write for AWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let Some(fd) = self.fd else {
            return Ok(0);
        };

        #[allow(clippy::cast_possible_truncation)]
        let buf_len = buf.len() as c_uint;
        let n = unsafe { awrite(fd, buf.as_ptr(), buf_len) };

        if n < 0 {
            return Err(std::io::Error::from_raw_os_error(unsafe { get_errno() }));
        }
        #[allow(clippy::cast_sign_loss)]
        Ok(n as usize)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl std::fmt::Debug for AWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AWriter").field("fd", &self.fd).finish()
    }
}
