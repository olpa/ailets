use actor_io::{error_kind_to_str, AReader, AWriter};
use actor_runtime::{err_to_heap_c_string, StdHandle};
use embedded_io::{Read, Write};
use std::ffi::c_char;

#[no_mangle]
pub extern "C" fn execute() -> *const c_char {
    let mut reader = AReader::new_from_std(StdHandle::Stdin);
    let mut writer = AWriter::new_from_std(StdHandle::Stdout);

    // Manual copy loop since std::io::copy is not available with embedded_io
    let mut buffer = [0u8; 8192];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break, // EOF
            Ok(n) => {
                let Some(data) = buffer.get(..n) else {
                    return err_to_heap_c_string(-1, "Buffer slice out of bounds");
                };
                if let Err(e) = writer.write_all(data) {
                    let error_msg = error_kind_to_str(e);
                    return err_to_heap_c_string(-1, &format!("Failed to write: {error_msg}"));
                }
            }
            Err(e) => {
                let error_msg = error_kind_to_str(e);
                return err_to_heap_c_string(-1, &format!("Failed to read: {error_msg}"));
            }
        }
    }

    if let Err(e) = writer.close() {
        let error_msg = error_kind_to_str(e);
        return err_to_heap_c_string(-1, &format!("Failed to close writer: {error_msg}"));
    }
    if let Err(e) = reader.close() {
        let error_msg = error_kind_to_str(e);
        return err_to_heap_c_string(-1, &format!("Failed to close reader: {error_msg}"));
    }

    std::ptr::null()
}
