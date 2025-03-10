use actor_io::{AReader, AWriter};
use actor_runtime::err_to_heap_c_string;
use std::ffi::c_char;
use std::io;

#[no_mangle]
pub extern "C" fn execute() -> *const c_char {
    let mut reader = match AReader::new(c"") {
        Ok(reader) => reader,
        Err(e) => return err_to_heap_c_string(&format!("Failed to open to read: {e}")),
    };
    let mut writer = match AWriter::new(c"") {
        Ok(writer) => writer,
        Err(e) => return err_to_heap_c_string(&format!("Failed to open to write: {e}")),
    };

    if let Err(e) = io::copy(&mut reader, &mut writer) {
        return err_to_heap_c_string(&format!("Failed to copy: {e}"));
    }

    if let Err(e) = writer.close() {
        return err_to_heap_c_string(&format!("Failed to close writer: {e}"));
    }
    if let Err(e) = reader.close() {
        return err_to_heap_c_string(&format!("Failed to close reader: {e}"));
    }

    std::ptr::null()
}
