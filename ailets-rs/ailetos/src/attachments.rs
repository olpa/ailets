//! Stream attachments - forward actor output to host stdout/stderr
//!
//! Attachments run as background tasks that read from actor pipes
//! and write to host stdout/stderr. They are spawned dynamically when
//! pipes are realized by `PipePool`.

use std::io::Write as StdWrite;
use std::sync::Arc;

use actor_runtime::StdHandle;
use parking_lot::Mutex;
use tracing::{debug, error, trace, warn};

use crate::idgen::{Handle, IdGen};
use crate::io::KVBuffers;
use crate::pipe::{PipePool, Reader};

/// Configuration for attachment behavior
#[derive(Debug, Clone)]
pub struct AttachmentConfig {
    /// Actors whose stdout should be attached to host stdout
    stdout_actors: Vec<Handle>,
}

impl Default for AttachmentConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl AttachmentConfig {
    /// Create a new empty attachment configuration
    #[must_use]
    pub fn new() -> Self {
        Self {
            stdout_actors: Vec::with_capacity(1),
        }
    }

    /// Add an actor whose stdout should be attached to host stdout
    pub fn attach_stdout(&mut self, actor_handle: Handle) {
        if !self.stdout_actors.contains(&actor_handle) {
            self.stdout_actors.push(actor_handle);
        }
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
            _ => {
                error!(node = ?node_handle, std = ?std_handle, "internal error: unexpected std_handle in attachment target");
                return;
            }
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
                AttachmentTarget::Stdout => {
                    attach_to_stream(node_handle, reader, std::io::stdout(), target).await;
                }
                AttachmentTarget::Stderr => {
                    attach_to_stream(node_handle, reader, std::io::stderr(), target).await;
                }
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
            if let Err(e) = task.await {
                warn!(error = %e, "attachment task failed during shutdown");
            }
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

/// Attach a pipe reader to a host output stream
///
/// Continuously reads from the pipe and forwards data to the target stream.
/// Exits when EOF is reached or an error occurs.
async fn attach_to_stream<W: StdWrite>(
    node_handle: Handle,
    mut reader: Reader,
    mut writer: W,
    target: AttachmentTarget,
) {
    debug!(node = ?node_handle, ?target, "attachment started");

    let mut buf = vec![0u8; 4096];

    loop {
        let n = reader.read(&mut buf).await;

        match n.cmp(&0) {
            std::cmp::Ordering::Greater => {
                let bytes_written = n.cast_unsigned();
                let Some(slice) = buf.get(..bytes_written) else {
                    warn!(node = ?node_handle, ?target, "buffer slice out of bounds");
                    break;
                };
                if let Err(e) = writer.write_all(slice) {
                    warn!(node = ?node_handle, ?target, error = %e, "failed to write");
                    break;
                }
                if let Err(e) = writer.flush() {
                    warn!(node = ?node_handle, ?target, error = %e, "failed to flush");
                    break;
                }
            }
            std::cmp::Ordering::Equal => {
                debug!(node = ?node_handle, ?target, "attachment EOF");
                break;
            }
            std::cmp::Ordering::Less => {
                warn!(node = ?node_handle, ?target, "read error in attachment");
                break;
            }
        }
    }

    reader.close();
    debug!(node = ?node_handle, ?target, "attachment finished");
}
