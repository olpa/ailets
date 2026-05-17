//! Debug actor that passes through N bytes, then pauses for resume

use actor_io::{AReader, AWriter};
use actor_runtime::{ActorRuntime, StdHandle};
use ailetos::Handle;
use std::io::{Read as _, Write as _};

use crate::dbg_control;

/// Debug actor that passes through data, optionally pausing after N bytes
///
/// By default, copies all data without pausing (acts like cat).
/// If --bytes-before-pause=N is specified, pauses after N bytes and waits for resume.
///
/// # Errors
/// Returns error if I/O operations fail or if configuration is invalid
pub fn execute(runtime: &dyn ActorRuntime) -> Result<(), String> {
    let my_handle = Handle::new(runtime.node_handle());

    let bytes_before_pause = dbg_control::get_bytes_before_pause(my_handle).ok_or_else(|| {
        format!("dbg actor {my_handle:?} not properly initialized (not registered)")
    })?;

    tracing::info!(node = ?my_handle, "dbg actor starting");

    let mut reader = AReader::new_from_std(runtime, StdHandle::Stdin);
    let mut writer = AWriter::new_from_std(runtime, StdHandle::Stdout);

    match bytes_before_pause {
        None => {
            tracing::info!(node = ?my_handle, "dbg actor: no pause configured, copying all");
            let total_bytes =
                std::io::copy(&mut reader, &mut writer).map_err(|e| format!("I/O error: {e}"))?;
            tracing::info!(node = ?my_handle, total_bytes, "dbg actor finished (EOF reached)");
        }
        Some(n) => {
            tracing::info!(node = ?my_handle, bytes_before_pause = n, "dbg actor: pause configured");

            let mut collected = Vec::new();
            (&mut reader)
                .take(n as u64)
                .read_to_end(&mut collected)
                .map_err(|e| format!("Failed to read: {e}"))?;
            if collected.len() < n {
                tracing::info!(node = ?my_handle, bytes_collected = collected.len(), "dbg actor: EOF before pause threshold, exiting without suspend");
                writer
                    .write_all(&collected)
                    .map_err(|e| format!("Failed to write collected data: {e}"))?;
                return Ok(());
            }

            tracing::info!(node = ?my_handle, bytes_collected = collected.len(), "dbg actor reached pause threshold, pausing");

            runtime.suspend_and_wait();
            if dbg_control::is_killed(my_handle) {
                tracing::info!(node = ?my_handle, "dbg actor killed");
                return Err("killed".to_string());
            }
            tracing::info!(node = ?my_handle, "dbg actor resumed, outputting collected data");

            writer
                .write_all(&collected)
                .map_err(|e| format!("Failed to write collected data: {e}"))?;
            tracing::info!(node = ?my_handle, bytes_written = collected.len(), "dbg actor wrote collected data, continuing");

            let remaining_bytes =
                std::io::copy(&mut reader, &mut writer).map_err(|e| format!("I/O error: {e}"))?;
            tracing::info!(node = ?my_handle, remaining_bytes, "dbg actor finished (EOF reached)");
        }
    }

    Ok(())
}
