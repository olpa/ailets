use std::ffi::CStr;
use std::os::raw::c_int;

use actor_runtime::{aclose, awrite, open_write};

pub struct AWriter {
    fd: Option<c_int>,
}

impl AWriter {
    #[must_use]
    pub fn new(filename: &CStr) -> Self {
        let fd = unsafe { open_write(filename.as_ptr()) };
        AWriter { fd: Some(fd) }
    }
}

impl Drop for AWriter {
    fn drop(&mut self) {
        if let Some(fd) = self.fd.take() {
            unsafe { aclose(fd) };
        }
    }
}

impl std::io::Write for AWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if let Some(fd) = self.fd {
            let n = unsafe { awrite(fd, buf.as_ptr(), buf.len().try_into().unwrap()) };
            let n: usize = match n {
                n if n <= 0 => panic!("Failed to write to output stream"),
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
