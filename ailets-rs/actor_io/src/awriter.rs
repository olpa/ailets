//! A writer implementation for the actor runtime system.
//!
//! `AWriter` provides an implementation of the [`embedded_io::Write`] trait.
//! It manages a file descriptor internally and ensures proper cleanup through the Drop trait.
//!
//! # Example
//! ```no_run
//! use embedded_io::Write;
//! use actor_io::AWriter;
//! # use actor_runtime::ActorRuntime;
//! # fn example(runtime: &dyn ActorRuntime) -> Result<(), embedded_io::ErrorKind> {
//!
//! let mut writer = AWriter::new(runtime, "example.txt")?;
//! writer.write_all(b"Hello, world!")?;
//! writer.close()?;
//! # Ok(())
//! # }
//! ```
//!
//! # Safety
//! This module uses unsafe code to interact with the actor runtime's C FFI interface.
//! The safety guarantees are maintained through proper file descriptor management
//! and automatic cleanup in the Drop implementation.

use actor_runtime::{ActorRuntime, StdHandle};
use core::ffi::c_int;

use crate::error_mapping::errno_to_error_kind;

pub struct AWriter<'a> {
    fd: Option<c_int>,
    runtime: &'a dyn ActorRuntime,
}

impl<'a> AWriter<'a> {
    /// Create a new `AWriter` instance for the specified file.
    ///
    /// # Errors
    /// Returns an error if the file could not be created.
    pub fn new(
        runtime: &'a dyn ActorRuntime,
        filename: &str,
    ) -> Result<Self, embedded_io::ErrorKind> {
        let fd = runtime.open_write(filename);
        if fd < 0 {
            let errno = runtime.get_errno();
            Err(errno_to_error_kind(errno))
        } else {
            Ok(AWriter {
                fd: Some(fd),
                runtime,
            })
        }
    }

    /// Create a new `AWriter` instance for the given standard handle.
    #[must_use]
    pub fn new_from_std(runtime: &'a dyn ActorRuntime, handle: StdHandle) -> Self {
        Self {
            fd: Some(handle as c_int),
            runtime,
        }
    }

    /// Create a new `AWriter` instance from an existing file descriptor.
    ///
    /// # Errors
    /// Returns an error if the file descriptor is invalid (negative).
    pub fn new_from_fd(
        runtime: &'a dyn ActorRuntime,
        fd: c_int,
    ) -> Result<Self, embedded_io::ErrorKind> {
        if fd < 0 {
            Err(embedded_io::ErrorKind::InvalidInput)
        } else {
            Ok(AWriter {
                fd: Some(fd),
                runtime,
            })
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
            let result = self.runtime.aclose(fd);
            if result < 0 {
                let errno = self.runtime.get_errno();
                return Err(errno_to_error_kind(errno));
            }
            self.fd = None;
        }
        Ok(())
    }
}

impl Drop for AWriter<'_> {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

impl embedded_io::ErrorType for AWriter<'_> {
    type Error = embedded_io::ErrorKind;
}

impl embedded_io::Write for AWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> core::result::Result<usize, Self::Error> {
        let Some(fd) = self.fd else {
            return Ok(0);
        };

        let n = self.runtime.awrite(fd, buf);

        if n < 0 {
            let errno = self.runtime.get_errno();
            return Err(errno_to_error_kind(errno));
        }
        #[allow(clippy::cast_sign_loss)]
        Ok(n as usize)
    }

    fn flush(&mut self) -> core::result::Result<(), Self::Error> {
        Ok(())
    }
}

impl core::fmt::Debug for AWriter<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AWriter").field("fd", &self.fd).finish()
    }
}
