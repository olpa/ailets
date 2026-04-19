//! Shell input actor that receives data from shell commands
//!
//! The actor takes its mpsc receiver at startup, forwards each message to stdout,
//! and exits when the channel is closed (EOF).

use actor_io::{error_kind_to_str, AWriter};
use actor_runtime::StdHandle;
use ailetos::BlockingActorRuntime;
use embedded_io::Write;

use crate::shell_input_control;

/// Shell input actor that forwards channel data to stdout.
///
/// # Errors
/// Returns error if I/O operations fail or if the actor is not properly registered
pub fn execute(runtime: &BlockingActorRuntime) -> Result<(), String> {
    let my_handle = runtime.node_handle();

    let receiver = shell_input_control::take_receiver(my_handle)
        .ok_or_else(|| format!("shell_input actor {my_handle:?} not properly initialized (not registered)"))?;

    tracing::info!(node = ?my_handle, "shell_input actor starting");

    let mut writer = AWriter::new_from_std(&runtime, StdHandle::Stdout);

    for data in receiver {
        tracing::debug!(node = ?my_handle, bytes = data.len(), "received data from shell, writing to stdout");
        if let Err(e) = writer.write_all(&data) {
            return Err(format!("Failed to write: {}", error_kind_to_str(e)));
        }
    }

    tracing::info!(node = ?my_handle, "shell_input actor: channel closed, terminating");
    Ok(())
}
