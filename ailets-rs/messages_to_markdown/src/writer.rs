use crate::node_runtime::{aclose, awrite, open_write};

pub struct Writer {
    fd: Option<u32>,
}

impl Writer {
    pub fn new(filename: &str) -> Self {
        let fd = unsafe { open_write(filename.as_ptr()) };
        Writer { fd: Some(fd) }
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

impl Drop for Writer {
    fn drop(&mut self) {
        if let Some(fd) = self.fd.take() {
            unsafe { aclose(fd) };
        }
    }
}
