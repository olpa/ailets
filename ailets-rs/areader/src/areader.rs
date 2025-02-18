use actor_runtime::{aclose, aread, n_of_streams, open_read};
use std::ffi::CStr;
use std::io::{Error, ErrorKind, Read, Result};
use std::os::raw::{c_int, c_uint};

pub struct AReader<'a> {
    fd: Option<c_int>,
    stream_index: c_uint,
    stream_name: &'a CStr,
}

impl<'a> AReader<'a> {
    /// Create a new `AReader` for the given stream name.
    ///
    /// # Errors
    /// Returns an error if opening fails.
    pub fn new(stream_name: &'a CStr) -> Result<Self> {
        let fd = Self::open(stream_name, 0)?;
        Ok(AReader {
            fd: Some(fd),
            stream_index: 0,
            stream_name,
        })
    }

    fn open(stream_name: &CStr, index: c_uint) -> Result<c_int> {
        let fd = unsafe { open_read(stream_name.as_ptr(), index) };
        if fd < 0 {
            return Err(Error::new(
                ErrorKind::Other,
                format!(
                    "Failed to open read stream '{}'",
                    stream_name.to_string_lossy()
                ),
            ));
        }
        Ok(fd)
    }

    /// Close the stream.
    ///
    /// # Errors
    /// Returns an error if closing fails.
    pub fn close(&mut self) -> Result<()> {
        if let Some(fd) = self.fd {
            let result = unsafe { aclose(fd) };
            if result < 0 {
                return Err(Error::new(
                    ErrorKind::Other,
                    format!(
                        "Failed to close stream '{}'",
                        self.stream_name.to_string_lossy()
                    ),
                ));
            }
            self.fd = None;
        }
        Ok(())
    }
}

impl<'a> std::fmt::Debug for AReader<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AReader")
            .field("fd", &self.fd)
            .field("stream_index", &self.stream_index)
            .field("stream_name", &self.stream_name)
            .finish()
    }
}

impl<'a> Read for AReader<'a> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if self.fd.is_none() {
            let n = unsafe { n_of_streams(self.stream_name.as_ptr()) };
            let n: c_uint = match n {
                n if n < 0 => panic!("Failed to get number of streams"),
                n => n.try_into().unwrap(),
            };
            if self.stream_index >= n {
                return Ok(0);
            }
            let fd = Self::open(self.stream_name, self.stream_index)?;
            self.fd = Some(fd);
        }

        let fd = self.fd.unwrap();
        let bytes_read =
            unsafe { aread(fd, buf.as_mut_ptr(), c_uint::try_from(buf.len()).unwrap()) };
        match bytes_read {
            n if n < 0 => Err(Error::new(
                ErrorKind::Other,
                format!(
                    "Failed to read stream '{}'",
                    self.stream_name.to_string_lossy()
                ),
            )),
            0 => {
                self.close()?;
                self.stream_index += 1;
                self.read(buf)
            }
            n => Ok(usize::try_from(n).unwrap()),
        }
    }
}

impl<'a> Drop for AReader<'a> {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
