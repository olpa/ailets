//! Debug actor that passes through N bytes, then pauses for resume

use actor_io::{error_kind_to_str, AReader, AWriter};
use actor_runtime::StdHandle;
use ailetos::BlockingActorRuntime;
use embedded_io::{Read, Write};

use crate::dbg_control;

const DEFAULT_BYTE_LIMIT: usize = 100;

/// Debug actor that passes through N bytes, then pauses for resume
///
/// Configuration is retrieved from the global registry using the actor's node handle.
///
/// # Errors
/// Returns error if I/O operations fail or if configuration is invalid
pub fn execute(runtime: BlockingActorRuntime) -> Result<(), String> {
    // Get my node handle from runtime
    let my_handle = runtime.node_handle();

    // Look up my control structure in global registry
    let control = dbg_control::get_dbg_control(my_handle)
        .ok_or_else(|| format!("dbg actor {:?} not properly initialized (not registered)", my_handle))?;

    tracing::info!(node = ?my_handle, "dbg actor starting");

    // Get byte limit from control structure
    let byte_limit = control.byte_limit().unwrap_or(DEFAULT_BYTE_LIMIT);
    tracing::debug!(node = ?my_handle, byte_limit = byte_limit, "dbg actor configuration");

    // Create I/O streams
    let mut reader = AReader::new_from_std(&runtime, StdHandle::Stdin);
    let mut writer = AWriter::new_from_std(&runtime, StdHandle::Stdout);

    // Phase 1: Pass through N bytes
    let bytes_copied = copy_n_bytes(&mut reader, &mut writer, byte_limit)?;
    tracing::info!(
        node = ?my_handle,
        bytes_copied = bytes_copied,
        "dbg actor reached byte limit, pausing"
    );

    // Phase 2: Pause and wait for resume
    control.wait_for_resume();
    tracing::info!(node = ?my_handle, "dbg actor resumed, continuing");

    // Phase 3: Continue until EOF
    let remaining_bytes = copy_until_eof(&mut reader, &mut writer)?;
    tracing::info!(
        node = ?my_handle,
        remaining_bytes = remaining_bytes,
        "dbg actor finished (EOF reached)"
    );

    Ok(())
}

/// Copy up to N bytes from reader to writer
///
/// Returns the number of bytes actually copied (may be less than N if EOF reached)
fn copy_n_bytes<'a>(
    reader: &mut AReader<'a>,
    writer: &mut AWriter<'a>,
    n: usize,
) -> Result<usize, String> {
    let mut buffer = [0u8; 8192];
    let mut total_copied = 0;

    while total_copied < n {
        let remaining = n - total_copied;
        let to_read = remaining.min(buffer.len());

        match reader.read(&mut buffer[..to_read]) {
            Ok(0) => break, // EOF
            Ok(bytes_read) => {
                let Some(data) = buffer.get(..bytes_read) else {
                    return Err("Buffer slice out of bounds".to_string());
                };
                if let Err(e) = writer.write_all(data) {
                    let error_msg = error_kind_to_str(e);
                    return Err(format!("Failed to write: {error_msg}"));
                }
                total_copied += bytes_read;
            }
            Err(e) => {
                let error_msg = error_kind_to_str(e);
                return Err(format!("Failed to read: {error_msg}"));
            }
        }
    }

    Ok(total_copied)
}

/// Copy all remaining bytes from reader to writer until EOF
///
/// Returns the number of bytes copied
fn copy_until_eof<'a>(reader: &mut AReader<'a>, writer: &mut AWriter<'a>) -> Result<usize, String> {
    let mut buffer = [0u8; 8192];
    let mut total_copied = 0;

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
                total_copied += n;
            }
            Err(e) => {
                let error_msg = error_kind_to_str(e);
                return Err(format!("Failed to read: {error_msg}"));
            }
        }
    }

    Ok(total_copied)
}
