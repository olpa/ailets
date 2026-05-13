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

#[cfg(feature = "std")]
use crate::error_mapping::embedded_io_to_std_error;
use crate::error_mapping::errno_to_error_kind;

pub struct AReader<'a> {
    fd: Option<isize>,
    runtime: &'a dyn ActorRuntime,
    /// Whether this reader owns the fd and should close it on drop.
    /// Standard handles are owned by `SystemRuntime`, not the actor.
    owns_fd: bool,
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
        match runtime.open_read(filename) {
            Ok(fd) => Ok(AReader {
                fd: Some(fd),
                runtime,
                owns_fd: true,
            }),
            Err(errno) => Err(errno_to_error_kind(errno)),
        }
    }

    /// Create a new `AReader` for the given standard handle.
    ///
    /// Note: Standard handles are owned by `SystemRuntime`, not the actor.
    /// This reader will NOT close the fd on drop.
    #[must_use]
    pub fn new_from_std(runtime: &'a dyn ActorRuntime, handle: StdHandle) -> Self {
        Self {
            fd: Some(handle as isize),
            runtime,
            owns_fd: false,
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
            if let Err(errno) = self.runtime.aclose(fd) {
                return Err(errno_to_error_kind(errno));
            }
            self.fd = None;
        }
        Ok(())
    }
}

impl Drop for AReader<'_> {
    fn drop(&mut self) {
        // Only close if we own the fd. Standard handles are owned by `SystemRuntime`.
        if self.owns_fd {
            if let Err(e) = self.close() {
                tracing::warn!(fd = ?self.fd, error = ?e, "AReader: failed to close on drop");
            }
        }
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

        self.runtime.aread(fd, buf).map_err(errno_to_error_kind)
    }
}

#[cfg(feature = "std")]
impl std::io::Read for AReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        embedded_io::Read::read(self, buf).map_err(embedded_io_to_std_error)
    }
}

impl core::fmt::Debug for AReader<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AReader").field("fd", &self.fd).finish()
    }
}
