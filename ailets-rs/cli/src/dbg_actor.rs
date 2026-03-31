//! Debug actor that passes through N bytes, then pauses for resume

use actor_io::{error_kind_to_str, AReader, AWriter};
use embedded_io::{Read, Write};

use crate::dbg_control;

const DEFAULT_BYTE_LIMIT: usize = 100;

/// Debug actor that passes through N bytes, then pauses for resume
///
/// Configuration (via thread-local from environment):
/// - `byte_limit`: number of bytes to pass through before pausing (default: 100)
///
/// # Errors
/// Returns error if I/O operations fail or if configuration is invalid
pub fn execute<'a>(mut reader: AReader<'a>, mut writer: AWriter<'a>) -> Result<(), String> {
    // Get the control handle from thread-local storage
    let control = dbg_control::get_current_dbg_control()
        .ok_or_else(|| "dbg actor not properly initialized (no control handle)".to_string())?;

    tracing::info!("dbg actor starting");

    // Get byte limit from thread-local (set by environment before spawning)
    let byte_limit = dbg_control::get_current_dbg_byte_limit()
        .unwrap_or(DEFAULT_BYTE_LIMIT);
    tracing::debug!(byte_limit = byte_limit, "dbg actor configuration");

    // Phase 1: Pass through N bytes
    let bytes_copied = copy_n_bytes(&mut reader, &mut writer, byte_limit)?;
    tracing::info!(
        bytes_copied = bytes_copied,
        "dbg actor reached byte limit, pausing"
    );

    // Phase 2: Pause and wait for resume
    control.wait_for_resume();
    tracing::info!("dbg actor resumed, continuing");

    // Phase 3: Continue until EOF
    let remaining_bytes = copy_until_eof(&mut reader, &mut writer)?;
    tracing::info!(
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
