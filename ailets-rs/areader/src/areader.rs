use std::io::{self, Read};
use std::ffi::CStr;

use actor_runtime::{aclose, aread, n_of_streams, open_read};

pub struct AReader<'a> {
    fd: Option<i32>,
    stream_index: usize,
    stream_name: &'a CStr,
}

impl<'a> AReader<'a> {
    #[must_use]
    pub fn new(stream_name: &'a CStr) -> Self {
        AReader {
            fd: None,
            stream_index: 0,
            stream_name,
        }
    }
}

impl<'a> Read for AReader<'a> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.fd.is_none() {
            let n = unsafe { n_of_streams(self.stream_name.as_ptr()) };
            let n: usize = match n {
                n if n < 0 => panic!("Failed to get number of streams"),
                n => n.try_into().unwrap(),
            };
            if self.stream_index >= n {
                return Ok(0);
            }
            let fd = unsafe { open_read(self.stream_name.as_ptr(), self.stream_index) };
            assert!(fd >= 0, "Failed to open read stream");
            self.fd = Some(fd);
        }
        let fd = self.fd.unwrap();
        let bytes_read = unsafe { aread(fd, buf.as_mut_ptr(), buf.len()) };
        match bytes_read {
            n if n < 0 => panic!("Failed to read stream"),
            0 => {
                unsafe { aclose(fd) };
                self.fd = None;
                self.stream_index += 1;
                self.read(buf)
            }
            n => Ok(usize::try_from(n).unwrap()),
        }
    }
}

impl<'a> Drop for AReader<'a> {
    fn drop(&mut self) {
        if let Some(fd) = self.fd.take() {
            unsafe {
                aclose(fd);
            }
        }
    }
}
