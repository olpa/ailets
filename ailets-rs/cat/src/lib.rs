use actor_io::{AReader, AWriter};
use std::io;
use std::os::raw::c_char;

fn err_to_c_string(f: &str, e: impl std::error::Error) -> *const c_char {
    let err = format!("Failed to {f}: {e}");
    #[allow(clippy::unwrap_used)]
    let err = std::ffi::CString::new(err).unwrap();
    err.as_ptr()
}

#[no_mangle]
#[allow(clippy::missing_panics_doc)]
pub extern "C" fn execute() -> *const c_char {
    let mut reader = match AReader::new(c"") {
        Ok(reader) => reader,
        Err(e) => return err_to_c_string("open to read", e),
    };
    let mut writer = match AWriter::new(c"") {
        Ok(writer) => writer,
        Err(e) => return err_to_c_string("open to write", e),
    };

    if let Err(e) = io::copy(&mut reader, &mut writer) {
        return err_to_c_string("copy", e);
    }

    if let Err(e) = writer.close() {
        return err_to_c_string("close writer", e);
    }
    if let Err(e) = reader.close() {
        return err_to_c_string("close reader", e);
    }

    std::ptr::null()
}
