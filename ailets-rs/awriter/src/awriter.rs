use std::ffi::CStr;
use std::os::raw::c_int;

use actor_runtime::{aclose, awrite, open_write};

pub struct AWriter {
    fd: Option<c_int>,
}

impl AWriter {
    #[must_use]
    pub fn new(filename: &CStr) -> Result<Self, std::io::Error> {
        let fd = unsafe { open_write(filename.as_ptr()) };
        if fd < 0 {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to open file '{}'", filename.to_string_lossy()),
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
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to close output stream {}", fd),
                ));
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
        if let Some(fd) = self.fd {
            let n = unsafe { awrite(fd, buf.as_ptr(), buf.len().try_into().unwrap()) };
            let n: usize = match n {
                n if n <= 0 => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Failed to write to output stream {}", fd),
                    ))
                }
                n => n.try_into().unwrap(),
            };
            Ok(n)
        } else {
            Ok(0)
        }
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
