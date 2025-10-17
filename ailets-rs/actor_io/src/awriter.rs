//! A writer implementation for the actor runtime system.
//!
//! `AWriter` provides an implementation of the [`embedded_io::Write`] trait.
//! It manages a file descriptor internally and ensures proper cleanup through the Drop trait.
//!
//! # Example
//! ```no_run
//! use embedded_io::Write;
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

use actor_runtime::{aclose, awrite, get_errno, open_write, StdHandle};
use core::ffi::{c_int, c_uint, CStr};

use crate::error_mapping::errno_to_error_kind;

pub struct AWriter {
    fd: Option<c_int>,
}

impl AWriter {
    /// Create a new `AWriter` instance for the specified file.
    ///
    /// # Errors
    /// Returns an error if the file could not be created.
    pub fn new(filename: &CStr) -> Result<Self, embedded_io::ErrorKind> {
        let fd = unsafe { open_write(filename.as_ptr()) };
        if fd < 0 {
            let errno = unsafe { get_errno() };
            Err(errno_to_error_kind(errno))
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
    pub fn new_from_fd(fd: c_int) -> Result<Self, embedded_io::ErrorKind> {
        if fd < 0 {
            Err(embedded_io::ErrorKind::InvalidInput)
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
    pub fn close(&mut self) -> Result<(), embedded_io::ErrorKind> {
        if let Some(fd) = self.fd {
            let result = unsafe { aclose(fd) };
            if result < 0 {
                let errno = unsafe { get_errno() };
                return Err(errno_to_error_kind(errno));
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

impl embedded_io::ErrorType for AWriter {
    type Error = embedded_io::ErrorKind;
}

impl embedded_io::Write for AWriter {
    fn write(&mut self, buf: &[u8]) -> core::result::Result<usize, Self::Error> {
        let Some(fd) = self.fd else {
            return Ok(0);
        };

        #[allow(clippy::cast_possible_truncation)]
        let buf_len = buf.len() as c_uint;
        let n = unsafe { awrite(fd, buf.as_ptr(), buf_len) };

        if n < 0 {
            let errno = unsafe { get_errno() };
            return Err(errno_to_error_kind(errno));
        }
        #[allow(clippy::cast_sign_loss)]
        Ok(n as usize)
    }

    fn flush(&mut self) -> core::result::Result<(), Self::Error> {
        Ok(())
    }
}

impl core::fmt::Debug for AWriter {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AWriter").field("fd", &self.fd).finish()
    }
}
