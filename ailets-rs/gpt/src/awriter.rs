use crate::node_runtime::{aclose, awrite, open_write};

pub struct AWriter {
    fd: Option<u32>,
}

impl AWriter {
    pub fn new(filename: &str) -> Self {
        let fd = unsafe { open_write(filename.as_ptr()) };
        AWriter { fd: Some(fd) }
    }

    pub fn start_message(&mut self) {
        self.str("{");
    }

    pub fn end_message(&mut self) {
        self.str("}");
    }

    pub fn str(&self, text: &str) {
        // FIXME: write_all
        if let Some(fd) = self.fd {
            unsafe {
                awrite(fd, text.as_ptr(), u32::try_from(text.len()).unwrap());
            }
        }
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
            unsafe {
                awrite(fd, buf.as_ptr(), u32::try_from(buf.len()).unwrap());
            }
            Ok(buf.len())
        } else {
            Ok(0)
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
