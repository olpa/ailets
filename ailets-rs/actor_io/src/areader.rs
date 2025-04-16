//! Read from actor streams.
//!
//! # Example
//!
//! ```no_run
//! use std::io::Read;
//! use actor_io::AReader;
//!
//! let mut reader = AReader::new(c"my_stream").unwrap();
//!
//! let mut buffer = Vec::new();
//! reader.read_to_end(&mut buffer).unwrap();
//! ```

use actor_runtime::{aclose, aread, get_errno, open_read, StdHandle};
use std::ffi::CStr;
use std::os::raw::{c_int, c_uint};

pub struct AReader {
    fd: Option<c_int>,
}

impl AReader {
    /// Create a new `AReader` for the given stream name.
    ///
    /// # Errors
    /// Returns an error if opening fails.
    pub fn new(filename: &CStr) -> std::io::Result<Self> {
        let fd = unsafe { open_read(filename.as_ptr()) };
        if fd < 0 {
            Err(std::io::Error::from_raw_os_error(unsafe { get_errno() }))
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

impl Drop for AReader {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

impl std::io::Read for AReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let Some(fd) = self.fd else {
            return Ok(0);
        };

        #[allow(clippy::cast_possible_truncation)]
        let buf_len = buf.len() as c_uint;
        let bytes_read = unsafe { aread(fd, buf.as_mut_ptr(), buf_len) };

        if bytes_read < 0 {
            return Err(std::io::Error::from_raw_os_error(unsafe { get_errno() }));
        }

        #[allow(clippy::cast_sign_loss)]
        Ok(bytes_read as usize)
    }
}

impl std::fmt::Debug for AReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AReader").field("fd", &self.fd).finish()
    }
}
