use crate::node_runtime::{aclose, awrite, open_write};

pub struct AWriter {
    fd: Option<u32>,
    message_has_field: bool,
}

impl AWriter {
    pub fn new(filename: &str) -> Self {
        let fd = unsafe { open_write(filename.as_ptr()) };
        AWriter {
            fd: Some(fd),
            message_has_field: false,
        }
    }

    pub fn begin_message(&mut self) {
        self.message_has_field = false;
    }

    fn _in_message(&mut self) {
        if !self.message_has_field {
            self.str("{");
            self.message_has_field = true;
        } else {
            self.str(",");
        }   
    }

    pub fn end_message(&mut self) {
        if self.message_has_field {
            self.str("}\n");
        }
    }

    pub fn role(&mut self, role: &str) {
        self._in_message();
        self.str("\"role\":\"");
        self.str(role);
        self.str("\"");
    }

    pub fn begin_text_content(&mut self) {
        self._in_message();
        self.str("\"content\":[{type:\"text\",text:\"");
    }

    pub fn end_text_content(&mut self) {
        self.str("\"}]");
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
