//! Shell input actor that receives data from shell commands
//!
//! This actor waits for data to be enqueued via shell commands (write),
//! writes the data to stdout, and continues in a loop until close is called.

use actor_io::{error_kind_to_str, AWriter};
use actor_runtime::StdHandle;
use ailetos::BlockingActorRuntime;
use embedded_io::Write;

use crate::shell_input_control;

/// Shell input actor that forwards queued data to stdout
///
/// The actor waits for data to be written via the `write` command,
/// forwards it to stdout, and continues until a `close` command is issued.
///
/// # Errors
/// Returns error if I/O operations fail or if the actor is not properly registered
pub fn execute(runtime: BlockingActorRuntime) -> Result<(), String> {
    // Get my node handle from runtime
    let my_handle = runtime.node_handle();

    // Look up my control structure in global registry
    let control = shell_input_control::get_shell_input_control(my_handle)
        .ok_or_else(|| format!("shell_input actor {:?} not properly initialized (not registered)", my_handle))?;

    tracing::info!(node = ?my_handle, "shell_input actor starting");

    // Create output stream
    let mut writer = AWriter::new_from_std(&runtime, StdHandle::Stdout);

    // Loop: wait for data, write it, repeat until closed
    loop {
        match control.wait_for_data() {
            Some(data) => {
                tracing::debug!(
                    node = ?my_handle,
                    bytes = data.len(),
                    "received data from shell, writing to stdout"
                );

                if let Err(e) = writer.write_all(&data) {
                    let error_msg = error_kind_to_str(e);
                    return Err(format!("Failed to write: {error_msg}"));
                }

                tracing::debug!(
                    node = ?my_handle,
                    bytes = data.len(),
                    "wrote data to stdout"
                );
            }
            None => {
                // Closed signal received
                tracing::info!(node = ?my_handle, "shell_input actor received close signal, terminating");
                break;
            }
        }
    }

    Ok(())
}
