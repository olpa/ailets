use actor_runtime::{aclose, awrite, open_write};
use std::ffi::CStr;
use std::io::Write;
pub struct StructureBuilder {
    fd: Option<i32>,
    need_para_divider: bool,
}

impl StructureBuilder {
    pub fn new(filename: &CStr) -> Self {
        let fd = unsafe { open_write(filename.as_ptr()) };
        StructureBuilder {
            fd: Some(fd),
            need_para_divider: false,
        }
    }

    pub fn start_paragraph(&mut self) {
        if self.need_para_divider {
            self.str("\n\n");
        }
        self.need_para_divider = true;
    }

    pub fn str(&mut self, text: &str) {
        let text_bytes = text.as_bytes();
        self.write_all(text_bytes).unwrap();
    }

    pub fn finish_with_newline(&mut self) {
        if self.need_para_divider {
            self.str("\n");
        }
        self.need_para_divider = false;
    }
}

impl Drop for StructureBuilder {
    fn drop(&mut self) {
        if let Some(fd) = self.fd.take() {
            unsafe { aclose(fd) };
        }
    }
}

impl std::io::Write for StructureBuilder {
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
