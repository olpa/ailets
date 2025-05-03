//! Dependency-free logging for libraries running in WASM mode.

use std::os::raw::{c_int, c_uint};

#[link(wasm_import_module = "")]
extern "C" {
    pub fn get_errno() -> c_int;
    pub fn awrite(fd: c_int, buffer_ptr: *const u8, count: c_uint) -> c_int;
}

const LOG_HANLDE: c_int = 2;

pub struct AiletsLog { }

impl std::io::Write for AiletsLog {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        #[allow(clippy::cast_possible_truncation)]
        let buf_len = buf.len() as c_uint;
        let n = unsafe { awrite(LOG_HANLDE, buf.as_ptr(), buf_len) };

        if n < 0 {
            return Err(std::io::Error::from_raw_os_error(unsafe { get_errno() }));
        }
        #[allow(clippy::cast_sign_loss)]
        Ok(n as usize)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
