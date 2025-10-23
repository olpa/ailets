//! Read from actor streams.
//!
//! # Example
//!
//! ```no_run
//! use embedded_io::Read;
//! use actor_io::AReader;
//!
//! let mut reader = AReader::new(c"my_stream").unwrap();
//!
//! let mut buffer = Vec::new();
//! let mut chunk = [0u8; 1024];
//! loop {
//!     let n = reader.read(&mut chunk).unwrap();
//!     if n == 0 {
//!         break;
//!     }
//!     buffer.extend_from_slice(&chunk[..n]);
//! }
//! ```

use actor_runtime::{aclose, aread, get_errno, open_read, StdHandle};
use core::ffi::{c_int, c_uint, CStr};

use crate::error_mapping::errno_to_error_kind;

pub struct AReader {
    fd: Option<c_int>,
}

impl AReader {
    /// Create a new `AReader` for the given stream name.
    ///
    /// # Errors
    /// Returns an error if opening fails.
    pub fn new(filename: &CStr) -> Result<Self, embedded_io::ErrorKind> {
        let fd = unsafe { open_read(filename.as_ptr()) };
        if fd < 0 {
            let errno = unsafe { get_errno() };
            Err(errno_to_error_kind(errno))
        } else {
            Ok(AReader { fd: Some(fd) })
        }
    }

    /// Create a new `AReader` for the given standard handle.
    #[must_use]
    pub fn new_from_std(handle: StdHandle) -> Self {
        Self {
            fd: Some(handle as c_int),
        }
    }

    /// Close the stream.
    /// Can be called multiple times.
    /// "read" and "drop" will call "close" automatically.
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

impl Drop for AReader {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

impl embedded_io::ErrorType for AReader {
    type Error = embedded_io::ErrorKind;
}

impl embedded_io::Read for AReader {
    fn read(&mut self, buf: &mut [u8]) -> core::result::Result<usize, Self::Error> {
        let Some(fd) = self.fd else {
            return Ok(0);
        };

        #[allow(clippy::cast_possible_truncation)]
        let buf_len = buf.len() as c_uint;
        let bytes_read = unsafe { aread(fd, buf.as_mut_ptr(), buf_len) };

        if bytes_read < 0 {
            let errno = unsafe { get_errno() };
            return Err(errno_to_error_kind(errno));
        }

        #[allow(clippy::cast_sign_loss)]
        Ok(bytes_read as usize)
    }
}

impl core::fmt::Debug for AReader {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AReader").field("fd", &self.fd).finish()
    }
}
