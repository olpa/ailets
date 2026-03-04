//! Stream attachments - forward actor output to host stdout/stderr
//!
//! Attachments run as background tasks that read from actor pipes
//! and write to host stdout/stderr. They are spawned lazily when
//! pipes are created, not during actor initialization.

use std::io::Write as StdWrite;

use tracing::{debug, warn};

use crate::idgen::Handle;
use crate::pipe::Reader;

/// Attach a pipe reader to host stdout
///
/// Continuously reads from the pipe and forwards data to host stdout.
/// Exits when EOF is reached or an error occurs.
pub async fn attach_to_stdout(node_handle: Handle, mut reader: Reader) {
    debug!(node = ?node_handle, "stdout attachment started");

    let mut buf = vec![0u8; 4096];
    let mut stdout = std::io::stdout();

    loop {
        let n = reader.read(&mut buf).await;

        if n > 0 {
            // Got data, write to stdout
            let bytes_written = n as usize;
            if let Err(e) = stdout.write_all(&buf[..bytes_written]) {
                warn!(node = ?node_handle, error = %e, "failed to write to stdout");
                break;
            }
            if let Err(e) = stdout.flush() {
                warn!(node = ?node_handle, error = %e, "failed to flush stdout");
                break;
            }
        } else if n == 0 {
            // EOF
            debug!(node = ?node_handle, "stdout attachment EOF");
            break;
        } else {
            // Error
            warn!(node = ?node_handle, "read error in stdout attachment");
            break;
        }
    }

    reader.close();
    debug!(node = ?node_handle, "stdout attachment finished");
}

/// Attach a pipe reader to host stderr
///
/// Continuously reads from the pipe and forwards data to host stderr.
/// Exits when EOF is reached or an error occurs.
pub async fn attach_to_stderr(node_handle: Handle, mut reader: Reader) {
    debug!(node = ?node_handle, "stderr attachment started");

    let mut buf = vec![0u8; 4096];
    let mut stderr = std::io::stderr();

    loop {
        let n = reader.read(&mut buf).await;

        if n > 0 {
            // Got data, write to stderr
            let bytes_written = n as usize;
            if let Err(e) = stderr.write_all(&buf[..bytes_written]) {
                warn!(node = ?node_handle, error = %e, "failed to write to stderr");
                break;
            }
            if let Err(e) = stderr.flush() {
                warn!(node = ?node_handle, error = %e, "failed to flush stderr");
                break;
            }
        } else if n == 0 {
            // EOF
            debug!(node = ?node_handle, "stderr attachment EOF");
            break;
        } else {
            // Error
            warn!(node = ?node_handle, "read error in stderr attachment");
            break;
        }
    }

    reader.close();
    debug!(node = ?node_handle, "stderr attachment finished");
}
