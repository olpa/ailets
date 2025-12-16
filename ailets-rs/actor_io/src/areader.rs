//! Read from actor streams.
//!
//! # Example
//!
//! ```no_run
//! use embedded_io::Read;
//! use actor_io::AReader;
//! # use actor_runtime::ActorRuntime;
//! # fn example(runtime: &dyn ActorRuntime) -> Result<(), embedded_io::ErrorKind> {
//!
//! let mut reader = AReader::new(runtime, "my_stream")?;
//!
//! let mut buffer = Vec::new();
//! let mut chunk = [0u8; 1024];
//! loop {
//!     let n = reader.read(&mut chunk)?;
//!     if n == 0 {
//!         break;
//!     }
//!     buffer.extend_from_slice(&chunk[..n]);
//! }
//! # Ok(())
//! # }
//! ```

use actor_runtime::{ActorRuntime, StdHandle};
use core::ffi::c_int;

use crate::error_mapping::errno_to_error_kind;

pub struct AReader<'a> {
    fd: Option<c_int>,
    runtime: &'a dyn ActorRuntime,
}

impl<'a> AReader<'a> {
    /// Create a new `AReader` for the given stream name.
    ///
    /// # Errors
    /// Returns an error if opening fails.
    pub fn new(
        runtime: &'a dyn ActorRuntime,
        filename: &str,
    ) -> Result<Self, embedded_io::ErrorKind> {
        let fd = runtime.open_read(filename);
        if fd < 0 {
            let errno = runtime.get_errno();
            Err(errno_to_error_kind(errno))
        } else {
            Ok(AReader {
                fd: Some(fd),
                runtime,
            })
        }
    }

    /// Create a new `AReader` for the given standard handle.
    #[must_use]
    pub fn new_from_std(runtime: &'a dyn ActorRuntime, handle: StdHandle) -> Self {
        Self {
            fd: Some(handle as c_int),
            runtime,
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

impl Drop for AReader<'_> {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

impl embedded_io::ErrorType for AReader<'_> {
    type Error = embedded_io::ErrorKind;
}

impl embedded_io::Read for AReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> core::result::Result<usize, Self::Error> {
        let Some(fd) = self.fd else {
            return Ok(0);
        };

        let bytes_read = self.runtime.aread(fd, buf);

        if bytes_read < 0 {
            let errno = self.runtime.get_errno();
            return Err(errno_to_error_kind(errno));
        }

        #[allow(clippy::cast_sign_loss)]
        Ok(bytes_read as usize)
    }
}

impl core::fmt::Debug for AReader<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AReader").field("fd", &self.fd).finish()
    }
}
