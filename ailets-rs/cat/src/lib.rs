use actor_io::{AReader, AWriter};
use actor_runtime::{err_to_heap_c_string, StdHandle};
use std::ffi::c_char;
use std::io;

#[no_mangle]
pub extern "C" fn execute() -> *const c_char {
    let mut reader = AReader::new_from_std(StdHandle::Stdin);
    let mut writer = AWriter::new_from_std(StdHandle::Stdout);

    if let Err(e) = io::copy(&mut reader, &mut writer) {
        return err_to_heap_c_string(
            e.raw_os_error().unwrap_or(-1),
            &format!("Failed to copy: {e}"),
        );
    }

    if let Err(e) = writer.close() {
        return err_to_heap_c_string(
            e.raw_os_error().unwrap_or(-1),
            &format!("Failed to close writer: {e}"),
        );
    }
    if let Err(e) = reader.close() {
        return err_to_heap_c_string(
            e.raw_os_error().unwrap_or(-1),
            &format!("Failed to close reader: {e}"),
        );
    }

    std::ptr::null()
}
