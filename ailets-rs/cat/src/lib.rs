use actor_io::{AReader, AWriter};
use std::io;
use std::os::raw::c_char;

#[no_mangle]
#[allow(clippy::missing_panics_doc)]
#[allow(clippy::unwrap_used)]
pub extern "C" fn execute() -> *const c_char {
    let mut reader = match AReader::new(c"") {
        Ok(reader) => reader,
        Err(e) => {
            let err = format!("Failed to open to read: {e}");
            let err = std::ffi::CString::new(err).unwrap();
            return err.as_ptr();
        }
    };
    let mut writer = match AWriter::new(c"") {
        Ok(writer) => writer,
        Err(e) => {
            let err = format!("Failed to open to write: {e}");
            let err = std::ffi::CString::new(err).unwrap();
            return err.as_ptr();
        }
    };

    if let Err(e) = io::copy(&mut reader, &mut writer) {
        let err = format!("Failed to copy: {e}");
        let err = std::ffi::CString::new(err).unwrap();
        return err.as_ptr();
    }

    if let Err(e) = writer.close() {
        let err = format!("Failed to close writer: {e}");
        let err = std::ffi::CString::new(err).unwrap();
        return err.as_ptr();
    }
    if let Err(e) = reader.close() {
        let err = format!("Failed to close reader: {e}");
        let err = std::ffi::CString::new(err).unwrap();
        return err.as_ptr();
    }

    std::ptr::null()
}
