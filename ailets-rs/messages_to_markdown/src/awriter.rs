use actor_runtime::{aclose, awrite, open_write};
use std::ffi::CStr;
pub struct AWriter {
    fd: Option<i32>,
    need_para_divider: bool,
}

impl AWriter {
    pub fn new(filename: &CStr) -> Self {
        let fd = unsafe { open_write(filename.as_ptr()) };
        AWriter {
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

    pub fn str(&self, text: &str) {
        let text_bytes = text.as_bytes();
        let text_bytes_len = text_bytes.len() as u32;
        if let Some(fd) = self.fd {
            let mut bytes_written: u32 = 0;
            while bytes_written < text_bytes_len {
                let n = unsafe {
                    awrite(
                        fd,
                        text_bytes.as_ptr().add(bytes_written as usize),
                        text_bytes_len - bytes_written,
                    )
                };
                let n: u32 = match n {
                    n if n <= 0 => panic!("Failed to write to output stream"),
                    n => n.try_into().unwrap(),
                };
                bytes_written += n;
            }
        }
    }

    pub fn finish_with_newline(&mut self) {
        if self.need_para_divider {
            self.str("\n");
        }
        self.need_para_divider = false;
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
