use actor_io::{error_kind_to_str, AReader, AWriter};
use actor_runtime::{err_to_heap_c_string, FfiActorRuntime, StdHandle};
use embedded_io::{Read, Write};
use std::ffi::c_char;

/// Core business logic: copies data from reader to writer
///
/// # Errors
///
/// Returns an error if:
/// - Reading from the input fails
/// - Writing to the output fails
/// - Closing the reader or writer fails
pub fn execute<'a>(mut reader: AReader<'a>, mut writer: AWriter<'a>) -> Result<(), String> {
    // Manual copy loop since std::io::copy is not available with embedded_io
    let mut buffer = [0u8; 8192];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break, // EOF
            Ok(n) => {
                let Some(data) = buffer.get(..n) else {
                    return Err("Buffer slice out of bounds".to_string());
                };
                if let Err(e) = writer.write_all(data) {
                    let error_msg = error_kind_to_str(e);
                    return Err(format!("Failed to write: {error_msg}"));
                }
            }
            Err(e) => {
                let error_msg = error_kind_to_str(e);
                return Err(format!("Failed to read: {error_msg}"));
            }
        }
    }

    if let Err(e) = writer.close() {
        let error_msg = error_kind_to_str(e);
        return Err(format!("Failed to close writer: {error_msg}"));
    }
    if let Err(e) = reader.close() {
        let error_msg = error_kind_to_str(e);
        return Err(format!("Failed to close reader: {error_msg}"));
    }

    Ok(())
}

/// WASM FFI wrapper
#[no_mangle]
pub extern "C" fn execute_wasm() -> *const c_char {
    let runtime = FfiActorRuntime::new();
    let reader = AReader::new_from_std(&runtime, StdHandle::Stdin);
    let writer = AWriter::new_from_std(&runtime, StdHandle::Stdout);

    match execute(reader, writer) {
        Ok(()) => std::ptr::null(),
        Err(e) => err_to_heap_c_string(-1, &e),
    }
}
