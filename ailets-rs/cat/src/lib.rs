use actor_io::{AReader, AWriter};
use actor_runtime::{err_to_heap_c_string, ActorRuntime, FfiActorRuntime, StdHandle};
use embedded_io::Write;
use std::ffi::c_char;

fn execute_impl<'a>(mut reader: AReader<'a>, mut writer: AWriter<'a>) -> Result<(), String> {
    std::io::copy(&mut reader, &mut writer).map_err(|e| format!("cat: copy error: {e}"))?;
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
