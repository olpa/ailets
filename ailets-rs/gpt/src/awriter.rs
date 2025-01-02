use crate::node_runtime::{aclose, awrite, open_write};

pub struct AWriter {
    fd: Option<u32>,
    message_has_role: bool,
    message_has_content: bool,
    text_is_open: bool,
}

/*
fn escape_json_value(s: &str) -> &str {
    s.chars()
        .map(|c| match c {
            '\\' => "\\",
            '"' => "\"",
            '\n' => "\\n",
            c if c < '\x20' => format!("\\u{:04x}", c as u32).as_str(),
            c => c.to_string().as_str(), // FIXME
        })
        .collect()
}
*/

impl AWriter {
    #[must_use]
    pub fn new(filename: &str) -> Self {
        let fd = unsafe { open_write(filename.as_ptr()) };
        AWriter {
            fd: Some(fd),
            message_has_role: false,
            message_has_content: false,
            text_is_open: false,
        }
    }

    pub fn begin_message(&mut self) {
        self.message_has_role = false;
        self.message_has_content = false;
        self.text_is_open = false;
    }

    pub fn end_message(&mut self) {
        if !self.message_has_role && !self.message_has_content {
            return;
        }
        if !self.message_has_content {
            self.begin_content();
        }
        if self.text_is_open {
            self.str("\"}");
            self.text_is_open = false;
        }
        if self.message_has_content {
            self.str("]");
        }
        self.str("}\n");
    }

    pub fn role(&mut self, role: &str) {
        if self.message_has_role {
            return;
        }
        self.str("{\"role\":\"");
        self.str(role);
        self.str("\"");
        self.message_has_role = true;
    }

    pub fn begin_content(&mut self) {
        if self.message_has_content {
            return;
        }
        if !self.message_has_role {
            self.role("assistant");
        }
        self.str(",\"content\":[");
        self.message_has_content = true;
        self.text_is_open = false;
    }

    pub fn begin_text_chunk(&mut self) {
        if !self.message_has_content {
            self.begin_content();
        }
        if !self.text_is_open {
            self.str("{\"type\":\"text\",\"text\":\"");
            self.text_is_open = true;
        }
    }

    #[allow(clippy::missing_panics_doc)]
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
