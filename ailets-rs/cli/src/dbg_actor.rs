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
pub fn execute(runtime: &BlockingActorRuntime) -> Result<(), String> {
    let my_handle = runtime.node_handle();

    let bytes_before_pause = dbg_control::get_bytes_before_pause(my_handle)
        .ok_or_else(|| format!("dbg actor {my_handle:?} not properly initialized (not registered)"))?;

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

            let collected = collect_n_bytes(&mut reader, n)?;
            tracing::info!(node = ?my_handle, bytes_collected = collected.len(), "dbg actor reached pause threshold, pausing");

            runtime.suspend_and_wait();
            tracing::info!(node = ?my_handle, "dbg actor resumed, outputting collected data");

            if let Err(e) = writer.write_all(&collected) {
                return Err(format!(
                    "Failed to write collected data: {}",
                    error_kind_to_str(e)
                ));
            }
            tracing::info!(node = ?my_handle, bytes_written = collected.len(), "dbg actor wrote collected data, continuing");

            let remaining_bytes = copy_until_eof(&mut reader, &mut writer)?;
            tracing::info!(node = ?my_handle, remaining_bytes, "dbg actor finished (EOF reached)");
        }
    }

    Ok(())
}

/// Collect up to N bytes from reader into a Vec (does not write to output)
fn collect_n_bytes(reader: &mut AReader<'_>, n: usize) -> Result<Vec<u8>, String> {
    let mut buffer = [0u8; 8192];
    let mut collected = Vec::with_capacity(n);

    while collected.len() < n {
        let remaining = n - collected.len();
        let to_read = remaining.min(buffer.len());

        let buf = buffer.get_mut(..to_read).ok_or("slice out of bounds")?;
        match reader.read(buf) {
            Ok(0) => break,
            Ok(bytes_read) => {
                if let Some(slice) = buf.get(..bytes_read) {
                    collected.extend_from_slice(slice);
                }
            }
            Err(e) => return Err(format!("Failed to read: {}", error_kind_to_str(e))),
        }
    }

    Ok(collected)
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
