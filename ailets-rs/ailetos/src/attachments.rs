//! Stream attachments - forward actor output to host stdout/stderr
//!
//! Attachments run as background tasks that read from actor pipes
//! and write to host stdout/stderr. They are spawned dynamically when
//! pipes are realized by `PipePool`.

use std::io::Write as StdWrite;
use std::sync::Arc;

use actor_runtime::StdHandle;
use parking_lot::Mutex;
use tracing::{debug, trace, warn};

use crate::idgen::{Handle, IdGen};
use crate::io::KVBuffers;
use crate::pipe::Reader;
use crate::pipepool::PipePool;

/// Configuration for attachment behavior
#[derive(Debug, Clone, Default)]
pub struct AttachmentConfig {
    /// Actors whose stdout should be attached to host stdout
    stdout_actors: std::collections::HashSet<Handle>,
}

impl AttachmentConfig {
    /// Create a new empty attachment configuration
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an actor whose stdout should be attached to host stdout
    pub fn attach_stdout(&mut self, actor_handle: Handle) {
        self.stdout_actors.insert(actor_handle);
    }

    /// Check if an actor's stdout should be attached
    #[must_use]
    pub fn should_attach_stdout(&self, actor_handle: Handle) -> bool {
        self.stdout_actors.contains(&actor_handle)
    }
}

/// Manages dynamic attachment of actor streams to host stdout/stderr
///
/// When `PipePool` realizes a writer, it notifies the `AttachmentManager`,
/// which decides whether to attach the stream based on configuration.
///
/// Attachment rules:
/// - Stdout: attach to host stdout only for actors in the configuration
/// - Log, Metrics, Trace: always attach to host stderr for all actors
/// - Stdin, Env: never attach
pub struct AttachmentManager {
    config: AttachmentConfig,
    /// Active attachment task handles
    tasks: Mutex<Vec<tokio::task::JoinHandle<()>>>,
}

impl AttachmentManager {
    /// Create a new attachment manager with the given configuration
    #[must_use]
    pub fn new(config: AttachmentConfig) -> Self {
        Self {
            config,
            tasks: Mutex::new(Vec::new()),
        }
    }

    /// Handle a writer realization event from `PipePool`
    ///
    /// This is called synchronously when a writer is created.
    /// Determines if attachment is needed and spawns the task.
    pub fn on_writer_realized<K: KVBuffers + 'static>(
        &self,
        node_handle: Handle,
        std_handle: StdHandle,
        pipe_pool: Arc<PipePool<K>>,
        id_gen: Arc<IdGen>,
    ) {
        // Determine if attachment is needed
        let should_attach = match std_handle {
            StdHandle::Stdout => self.config.should_attach_stdout(node_handle),
            StdHandle::Log | StdHandle::Metrics | StdHandle::Trace => true,
            StdHandle::Stdin | StdHandle::Env | StdHandle::_Count => false,
        };

        if !should_attach {
            return;
        }

        // Determine attachment target
        let target = match std_handle {
            StdHandle::Stdout => AttachmentTarget::Stdout,
            StdHandle::Log | StdHandle::Metrics | StdHandle::Trace => AttachmentTarget::Stderr,
            _ => return, // Should not reach here
        };

        debug!(node = ?node_handle, std = ?std_handle, target = ?target, "spawning attachment for realized writer");

        // Spawn attachment task
        let task = tokio::spawn(async move {
            // Get reader for the realized writer
            let Some(reader) = pipe_pool
                .get_or_await_reader((node_handle, std_handle), false, &id_gen)
                .await
            else {
                warn!(node = ?node_handle, std = ?std_handle, "failed to get reader for attachment");
                return;
            };

            // Run attachment worker
            match target {
                AttachmentTarget::Stdout => attach_to_stdout(node_handle, reader).await,
                AttachmentTarget::Stderr => attach_to_stderr(node_handle, reader).await,
            }
        });

        // Store task handle
        self.tasks.lock().push(task);
    }

    /// Wait for all attachment tasks to complete
    ///
    /// This should be called during environment shutdown.
    pub async fn waiting_shutdown(&self) {
        let tasks = std::mem::take(&mut *self.tasks.lock());
        trace!(
            "AttachmentManager::waiting_shutdown: entering loop, tasks count = {}",
            tasks.len()
        );
        for task in tasks {
            let _ = task.await;
        }
        trace!("AttachmentManager::waiting_shutdown: exited loop");
    }
}

/// Target for attachment
#[derive(Debug, Clone, Copy)]
enum AttachmentTarget {
    Stdout,
    Stderr,
}

/// Attach a pipe reader to host stdout
///
/// Continuously reads from the pipe and forwards data to host stdout.
/// Exits when EOF is reached or an error occurs.
async fn attach_to_stdout(node_handle: Handle, mut reader: Reader) {
    debug!(node = ?node_handle, "stdout attachment started");

    let mut buf = vec![0u8; 4096];
    let mut stdout = std::io::stdout();

    loop {
        let n = reader.read(&mut buf).await;

        match n.cmp(&0) {
            std::cmp::Ordering::Greater => {
                // Got data, write to stdout
                let bytes_written = n.cast_unsigned();
                let Some(slice) = buf.get(..bytes_written) else {
                    warn!(node = ?node_handle, "buffer slice out of bounds");
                    break;
                };
                if let Err(e) = stdout.write_all(slice) {
                    warn!(node = ?node_handle, error = %e, "failed to write to stdout");
                    break;
                }
                if let Err(e) = stdout.flush() {
                    warn!(node = ?node_handle, error = %e, "failed to flush stdout");
                    break;
                }
            }
            std::cmp::Ordering::Equal => {
                // EOF
                debug!(node = ?node_handle, "stdout attachment EOF");
                break;
            }
            std::cmp::Ordering::Less => {
                // Error
                warn!(node = ?node_handle, "read error in stdout attachment");
                break;
            }
        }
    }

    reader.close();
    debug!(node = ?node_handle, "stdout attachment finished");
}

/// Attach a pipe reader to host stderr
///
/// Continuously reads from the pipe and forwards data to host stderr.
/// Exits when EOF is reached or an error occurs.
async fn attach_to_stderr(node_handle: Handle, mut reader: Reader) {
    debug!(node = ?node_handle, "stderr attachment started");

    let mut buf = vec![0u8; 4096];
    let mut stderr = std::io::stderr();

    loop {
        let n = reader.read(&mut buf).await;

        match n.cmp(&0) {
            std::cmp::Ordering::Greater => {
                // Got data, write to stderr
                let bytes_written = n.cast_unsigned();
                let Some(slice) = buf.get(..bytes_written) else {
                    warn!(node = ?node_handle, "buffer slice out of bounds");
                    break;
                };
                if let Err(e) = stderr.write_all(slice) {
                    warn!(node = ?node_handle, error = %e, "failed to write to stderr");
                    break;
                }
                if let Err(e) = stderr.flush() {
                    warn!(node = ?node_handle, error = %e, "failed to flush stderr");
                    break;
                }
            }
            std::cmp::Ordering::Equal => {
                // EOF
                debug!(node = ?node_handle, "stderr attachment EOF");
                break;
            }
            std::cmp::Ordering::Less => {
                // Error
                warn!(node = ?node_handle, "read error in stderr attachment");
                break;
            }
        }
    }

    reader.close();
    debug!(node = ?node_handle, "stderr attachment finished");
}
