//! Debug actor that passes through N bytes, then pauses for resume

use actor_io::{error_kind_to_str, AReader, AWriter};
use actor_runtime::StdHandle;
use ailetos::BlockingActorRuntime;
use embedded_io::{Read, Write};

use crate::dbg_control;

/// Debug actor that passes through data, optionally pausing after N bytes
///
/// By default, copies all data without pausing (acts like cat).
/// If --bytes-before-pause=N is specified, pauses after N bytes and waits for resume.
///
/// # Errors
/// Returns error if I/O operations fail or if configuration is invalid
pub fn execute(runtime: BlockingActorRuntime) -> Result<(), String> {
    let my_handle = runtime.node_handle();

    let bytes_before_pause = dbg_control::get_bytes_before_pause(my_handle)
        .ok_or_else(|| format!("dbg actor {:?} not properly initialized (not registered)", my_handle))?;

    tracing::info!(node = ?my_handle, "dbg actor starting");

    let mut reader = AReader::new_from_std(&runtime, StdHandle::Stdin);
    let mut writer = AWriter::new_from_std(&runtime, StdHandle::Stdout);

    match bytes_before_pause {
        None => {
            tracing::info!(node = ?my_handle, "dbg actor: no pause configured, copying all");
            let total_bytes = copy_until_eof(&mut reader, &mut writer)?;
            tracing::info!(node = ?my_handle, total_bytes, "dbg actor finished (EOF reached)");
        }
        Some(n) => {
            tracing::info!(node = ?my_handle, bytes_before_pause = n, "dbg actor: pause configured");

            let bytes_copied = copy_n_bytes(&mut reader, &mut writer, n)?;
            tracing::info!(node = ?my_handle, bytes_copied, "dbg actor reached pause threshold, pausing");

            runtime.suspend_and_wait();
            tracing::info!(node = ?my_handle, "dbg actor resumed, continuing");

            let remaining_bytes = copy_until_eof(&mut reader, &mut writer)?;
            tracing::info!(node = ?my_handle, remaining_bytes, "dbg actor finished (EOF reached)");
        }
    }

    Ok(())
}

/// Copy up to N bytes from reader to writer
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
            Ok(0) => break,
            Ok(bytes_read) => {
                let Some(data) = buffer.get(..bytes_read) else {
                    return Err("Buffer slice out of bounds".to_string());
                };
                if let Err(e) = writer.write_all(data) {
                    return Err(format!("Failed to write: {}", error_kind_to_str(e)));
                }
                total_copied += bytes_read;
            }
            Err(e) => return Err(format!("Failed to read: {}", error_kind_to_str(e))),
        }
    }

    Ok(total_copied)
}

/// Copy all remaining bytes from reader to writer until EOF
fn copy_until_eof<'a>(reader: &mut AReader<'a>, writer: &mut AWriter<'a>) -> Result<usize, String> {
    let mut buffer = [0u8; 8192];
    let mut total_copied = 0;

    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => {
                let Some(data) = buffer.get(..n) else {
                    return Err("Buffer slice out of bounds".to_string());
                };
                if let Err(e) = writer.write_all(data) {
                    return Err(format!("Failed to write: {}", error_kind_to_str(e)));
                }
                total_copied += n;
            }
            Err(e) => return Err(format!("Failed to read: {}", error_kind_to_str(e))),
        }
    }

    Ok(total_copied)
}
