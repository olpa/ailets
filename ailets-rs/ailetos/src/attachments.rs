//! Stream attachments - forward actor output to host stdout/stderr
//!
//! Attachments run as background tasks that read from actor pipes
//! and write to host stdout/stderr. They are spawned dynamically when
//! pipes are realized by `PipePool`.

use std::io::Write as StdWrite;
use std::sync::Arc;

use actor_runtime::StdHandle;
use parking_lot::{Mutex, RwLock};
use tracing::{debug, trace, warn};

use crate::idgen::{Handle, IdGen};
use crate::pipe::{copy_to_writer, FlushMode, PipePool};

/// Configuration for attachment behavior
pub struct AttachmentConfig {
    /// Actors whose stdout should be routed to a custom writer (consumed on first realization)
    custom_sinks: Vec<(Handle, Box<dyn StdWrite + Send + Sync>)>,
}

impl std::fmt::Debug for AttachmentConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let handles: Vec<Handle> = self.custom_sinks.iter().map(|(h, _)| *h).collect();
        f.debug_struct("AttachmentConfig")
            .field("custom_sinks", &handles)
            .finish()
    }
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
            custom_sinks: Vec::new(),
        }
    }

    /// Add an actor whose stdout should be routed to a custom writer.
    /// Multiple sinks per actor are allowed; each gets its own independent reader.
    pub fn attach_to_sink(&mut self, actor_handle: Handle, sink: Box<dyn StdWrite + Send + Sync>) {
        self.custom_sinks.push((actor_handle, sink));
    }

    /// Take all custom sinks for this actor, removing them from the config.
    pub fn take_custom_sinks(
        &mut self,
        actor_handle: Handle,
    ) -> Vec<Box<dyn StdWrite + Send + Sync>> {
        let original = std::mem::take(&mut self.custom_sinks);
        let (matching, remaining): (Vec<_>, Vec<_>) = original
            .into_iter()
            .partition(|(h, _)| *h == actor_handle);
        self.custom_sinks = remaining;
        matching.into_iter().map(|(_, sink)| sink).collect()
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
    config: Arc<RwLock<AttachmentConfig>>,
    /// Active attachment task handles
    tasks: Mutex<Vec<tokio::task::JoinHandle<Result<(), String>>>>,
}

impl AttachmentManager {
    /// Create a new attachment manager sharing the given live config.
    #[must_use]
    pub fn new(config: Arc<RwLock<AttachmentConfig>>) -> Self {
        Self {
            config,
            tasks: Mutex::new(Vec::new()),
        }
    }

    /// Handle a writer realization event from `PipePool`
    ///
    /// This is called synchronously when a writer is created.
    /// Determines if attachment is needed and spawns the task.
    pub fn on_writer_realized(
        &self,
        node_handle: Handle,
        fd: isize,
        pipe_pool: Arc<PipePool>,
        id_gen: Arc<IdGen>,
    ) {
        let Ok(std_handle) = StdHandle::try_from(fd) else {
            return;
        };

        match std_handle {
            StdHandle::Stdout => {
                // Take all custom sinks for this actor
                let custom_sinks = {
                    let mut config = self.config.write();
                    config.take_custom_sinks(node_handle)
                };

                if custom_sinks.is_empty() {
                    return;
                }

                let mut tasks = self.tasks.lock();

                // One independent task per custom sink (fan-out)
                // Each task gets its own independent Reader from the same pipe,
                // allowing multiple destinations to receive the same data.
                //
                // Note: We pass pipe_pool/id_gen to each task instead of getting readers here
                // because on_writer_realized is synchronous but get_or_await_reader is async.
                // Each spawned task will call get_or_await_reader independently.
                for sink in custom_sinks {
                    let pool = Arc::clone(&pipe_pool);
                    let gen = Arc::clone(&id_gen);
                    debug!(node = ?node_handle, fd = fd, "spawning custom-sink attachment");
                    tasks.push(tokio::spawn(async move {
                        spawn_attachment(node_handle, fd, pool, gen, sink).await
                    }));
                }
            }
            StdHandle::Log | StdHandle::Metrics | StdHandle::Trace => {
                // Note: We pass pipe_pool/id_gen instead of getting reader here
                // because on_writer_realized is synchronous but get_or_await_reader is async.
                debug!(node = ?node_handle, fd = fd, "spawning stderr attachment");
                let pool = Arc::clone(&pipe_pool);
                let gen = Arc::clone(&id_gen);
                self.tasks.lock().push(tokio::spawn(async move {
                    spawn_attachment(node_handle, fd, pool, gen, std::io::stderr()).await
                }));
            }
            StdHandle::Stdin | StdHandle::Env | StdHandle::_Count => {}
        }
    }

    /// Wait for all attachment tasks to complete.
    ///
    /// # Errors
    /// Returns `Err` with a summary if any task failed or panicked.
    pub async fn shutdown(&self) -> Result<(), String> {
        let tasks = std::mem::take(&mut *self.tasks.lock());
        trace!(
            "AttachmentManager::waiting_shutdown: entering loop, tasks count = {}",
            tasks.len()
        );
        let mut failed_count = 0;
        for task in tasks {
            match task.await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    warn!(error = %e, "attachment task error");
                    failed_count += 1;
                }
                Err(e) => {
                    warn!(error = %e, "attachment task panicked");
                    failed_count += 1;
                }
            }
        }
        trace!("AttachmentManager::waiting_shutdown: exited loop");
        if failed_count > 0 {
            Err(format!(
                "attachment shutdown failed for {failed_count} tasks"
            ))
        } else {
            Ok(())
        }
    }
}

/// Spawn an attachment task that copies data from a pipe to a writer.
///
/// This function is called once per attachment (one per sink for fan-out).
/// It gets an independent Reader from the pipe pool, then copies all data
/// to the writer. Multiple calls with the same (node_handle, fd) create
/// independent readers that all read from the same pipe (fan-out).
///
/// # Arguments
/// * `node_handle` - Handle of the actor whose output is being attached
/// * `fd` - File descriptor of the pipe (stdout, stderr, etc.)
/// * `pipe_pool` - Pool to get readers from
/// * `id_gen` - ID generator for creating readers
/// * `writer` - Destination writer (custom sink, host stdout, or host stderr)
async fn spawn_attachment<W: StdWrite>(
    node_handle: Handle,
    fd: isize,
    pipe_pool: Arc<PipePool>,
    id_gen: Arc<IdGen>,
    writer: W,
) -> Result<(), String> {
    // Get an independent reader for this attachment.
    // Multiple calls with the same (node_handle, fd) create independent readers
    // that all read from the same pipe (fan-out).
    let reader = match pipe_pool
        .get_or_await_reader((node_handle, fd), false, &id_gen)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            warn!(node = ?node_handle, fd = fd, error = ?e, "failed to get reader for attachment");
            return Err(format!("failed to get reader for attachment: {e:?}"));
        }
    };

    debug!(node = ?node_handle, fd = fd, "attachment started");
    let result = copy_to_writer(reader, writer, FlushMode::AfterEachWrite).await;

    if let Err(ref e) = result {
        warn!(node = ?node_handle, fd = fd, error = %e, "attachment failed");
    } else {
        debug!(node = ?node_handle, fd = fd, "attachment finished");
    }

    result
}
