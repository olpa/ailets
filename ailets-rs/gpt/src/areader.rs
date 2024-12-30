use std::io::{self, Read};

use crate::node_runtime::{aclose, aread, n_of_streams, open_read};

pub struct AReader {
    fd: Option<u32>,
    stream_index: u32,
    stream_name: String,
}

impl AReader {
    pub fn new(stream_name: &str) -> Self {
        AReader {
            fd: None,
            stream_index: 0,
            stream_name: stream_name.to_string(),
        }
    }
}

impl Read for AReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.fd.is_none() {
            let n = unsafe { n_of_streams(self.stream_name.as_ptr()) };
            if self.stream_index >= n {
                return Ok(0);
            }
            let fd = unsafe { open_read(self.stream_name.as_ptr(), self.stream_index) };
            self.fd = Some(fd);
        }
        let fd = self.fd.unwrap();
        let bytes_read = unsafe { aread(fd, buf.as_mut_ptr(), u32::try_from(buf.len()).unwrap()) };
        if bytes_read == 0 {
            unsafe { aclose(fd) };
            self.fd = None;

            self.stream_index += 1;
            return self.read(buf);
        }
        Ok(bytes_read as usize)
    }
}

impl Drop for AReader {
    fn drop(&mut self) {
        if let Some(fd) = self.fd.take() {
            unsafe {
                crate::node_runtime::aclose(fd);
            }
        }
    }
}
