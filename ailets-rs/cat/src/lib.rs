use actor_io::{error_kind_to_str, AReader, AWriter};
use actor_runtime::{err_to_heap_c_string, ActorRuntime, FfiActorRuntime, StdHandle};
use embedded_io::{Read, Write};
use std::ffi::c_char;

/// Core business logic implementation: copies data from reader to writer
///
/// # Errors
///
/// Returns an error if:
/// - Reading from the input fails
/// - Writing to the output fails
fn execute_impl<'a>(mut reader: AReader<'a>, mut writer: AWriter<'a>) -> Result<(), String> {
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

    // Note: Actors never close stdout/stdin - they didn't open them.
    // SystemRuntime will close these pipes during ActorShutdown.

    Ok(())
}

/// Native actor entry point - receives runtime and creates I/O streams
///
/// # Errors
///
/// Returns an error if:
/// - Reading from the input fails
/// - Writing to the output fails
pub fn execute(runtime: &dyn ActorRuntime) -> Result<(), String> {
    let reader = AReader::new_from_std(runtime, StdHandle::Stdin);
    let writer = AWriter::new_from_std(runtime, StdHandle::Stdout);
    let result = execute_impl(reader, writer);
    if let Err(ref e) = result {
        let mut log = AWriter::new_from_std(runtime, StdHandle::Log);
        if log.write_all(format!("{e}\n").as_bytes()).is_err() {}
    }
    result
}

/// WASM FFI wrapper
#[no_mangle]
pub extern "C" fn execute_wasm() -> *const c_char {
    let runtime = FfiActorRuntime::new();
    match execute(&runtime) {
        Ok(()) => std::ptr::null(),
        Err(e) => err_to_heap_c_string(-1, &e),
    }
}
