//! `PipePool` - manages output pipes for actors
//!
//! Each (actor, StdHandle) pair can have its own output pipe. Readers are created on-demand
//! when consuming actors need to read from dependencies.

use std::collections::HashMap;
use std::sync::Arc;

use actor_runtime::StdHandle;
use parking_lot::Mutex;
use tokio::sync::mpsc;
use tracing::error;

use crate::idgen::{Handle, IdGen};
use crate::io::{KVBuffers, OpenMode};
use crate::notification_queue::NotificationQueueArc;
use crate::pipe::{Pipe, Reader, Writer};

/// Event sent when a pipe is created
#[derive(Debug, Clone)]
pub struct PipeCreatedEvent {
    pub node_handle: Handle,
    pub std_handle: StdHandle,
}

/// Pool of output pipes, indexed by (actor handle, StdHandle) pair
///
/// Uses interior mutability via `Mutex` to allow shared access through `Arc<PipePool>`.
pub struct PipePool<K: KVBuffers> {
    /// Output pipes indexed by (actor handle, StdHandle) pair
    pipes: Mutex<HashMap<(Handle, StdHandle), Pipe>>,
    /// Shared notification queue for all pipes
    notification_queue: NotificationQueueArc,
    /// Key-value store for pipe buffers
    kv: Arc<K>,
    /// Notifies when pipes are created (for lazy attachment spawning)
    pipe_created_tx: Mutex<Option<mpsc::UnboundedSender<PipeCreatedEvent>>>,
}

impl<K: KVBuffers> PipePool<K> {
    /// Create a new empty pipe pool
    #[must_use]
    pub fn new(
        kv: Arc<K>,
        notification_queue: NotificationQueueArc,
        pipe_created_tx: Option<mpsc::UnboundedSender<PipeCreatedEvent>>,
    ) -> Self {
        Self {
            pipes: Mutex::new(HashMap::new()),
            notification_queue,
            kv,
            pipe_created_tx: Mutex::new(pipe_created_tx),
        }
    }

    /// Create an output pipe for the given actor and handle
    ///
    /// # Errors
    /// Returns an error if creating the buffer fails or if the pipe already exists
    pub async fn create_output_pipe(
        &self,
        actor_handle: Handle,
        std_handle: StdHandle,
        name: &str,
        id_gen: &IdGen,
    ) -> Result<Handle, crate::io::KVError> {
        let key = (actor_handle, std_handle);

        // Check if pipe already exists (lock scope)
        {
            let pipes = self.pipes.lock();
            if pipes.contains_key(&key) {
                return Err(crate::io::KVError::AlreadyExists(format!(
                    "Pipe for actor {actor_handle:?} handle {std_handle:?} already exists"
                )));
            }
        }

        let writer_handle = Handle::new(id_gen.get_next());

        // Get buffer from KV store (outside lock)
        let buffer = self.kv.open(name, OpenMode::Write).await?;

        let pipe = Pipe::new(writer_handle, self.notification_queue.clone(), name, buffer);

        // Insert pipe (lock scope)
        {
            let mut pipes = self.pipes.lock();
            pipes.insert(key, pipe);
        }

        // Send notification (outside pipes lock)
        {
            let tx_guard = self.pipe_created_tx.lock();
            if let Some(ref tx) = *tx_guard {
                let _ = tx.send(PipeCreatedEvent {
                    node_handle: actor_handle,
                    std_handle,
                });
            }
        }

        Ok(writer_handle)
    }

    /// Check if a pipe exists for the given actor and handle
    #[must_use]
    pub fn has_pipe(&self, actor_handle: Handle, std_handle: StdHandle) -> bool {
        let pipes = self.pipes.lock();
        pipes.contains_key(&(actor_handle, std_handle))
    }

    /// Get the pipe for the given actor and handle
    ///
    /// Returns `None` if the pipe doesn't exist.
    #[must_use]
    pub fn get_pipe(&self, actor_handle: Handle, std_handle: StdHandle) -> Option<PipeRef<'_>> {
        let pipes = self.pipes.lock();
        let key = (actor_handle, std_handle);
        if pipes.contains_key(&key) {
            Some(PipeRef {
                guard: pipes,
                key,
            })
        } else {
            None
        }
    }

    /// Open a reader for the given actor's output pipe
    ///
    /// Creates a new Reader instance. Multiple readers can be created
    /// for the same pipe.
    ///
    /// Returns `None` if the pipe doesn't exist.
    #[must_use]
    pub fn open_reader(
        &self,
        actor_handle: Handle,
        std_handle: StdHandle,
        id_gen: &IdGen,
    ) -> Option<Reader> {
        let pipe_ref = self.get_pipe(actor_handle, std_handle)?;
        let reader_handle = Handle::new(id_gen.get_next());
        pipe_ref.get_reader(reader_handle)
    }

    /// Create a standalone Writer backed by KV storage (not wrapped in a Pipe).
    ///
    /// Used to create merge writers for actors with multiple dependencies.
    ///
    /// # Errors
    /// Returns an error if creating the buffer fails
    pub async fn create_merge_writer(
        &self,
        name: &str,
        id_gen: &IdGen,
    ) -> Result<Writer, crate::io::KVError> {
        let writer_handle = Handle::new(id_gen.get_next());
        let buffer = self.kv.open(name, OpenMode::Write).await?;
        Ok(Writer::new(
            writer_handle,
            self.notification_queue.clone(),
            name,
            buffer,
        ))
    }

    /// Flush the buffer for the given actor's pipe
    ///
    /// # Errors
    /// Returns an error if flushing fails or if the pipe doesn't exist
    pub async fn flush_buffer(
        &self,
        actor_handle: Handle,
        std_handle: StdHandle,
    ) -> Result<(), crate::io::KVError> {
        let buffer = {
            let pipe_ref = self.get_pipe(actor_handle, std_handle).ok_or_else(|| {
                crate::io::KVError::NotFound(format!(
                    "Pipe for actor {actor_handle:?} handle {std_handle:?}"
                ))
            })?;
            let writer = pipe_ref.writer().ok_or_else(|| {
                crate::io::KVError::NotFound(format!(
                    "Writer for actor {actor_handle:?} handle {std_handle:?}"
                ))
            })?;
            writer.buffer()
        }; // pipe_ref dropped here, lock released
        self.kv.flush_buffer(&buffer).await
    }

    /// Drop the pipe creation notification sender
    ///
    /// This should be called when all actors have finished and no more pipes will be created.
    /// Dropping the sender allows the notification receiver to close, enabling clean shutdown.
    pub fn drop_pipe_created_tx(&self) {
        let mut tx_guard = self.pipe_created_tx.lock();
        *tx_guard = None;
    }
}

/// A reference to a pipe in the pool, holding the lock
pub struct PipeRef<'a> {
    guard: parking_lot::MutexGuard<'a, HashMap<(Handle, StdHandle), Pipe>>,
    key: (Handle, StdHandle),
}

impl PipeRef<'_> {
    /// Get the writer side of the pipe
    ///
    /// Returns `None` if the key is invalid (should never happen in practice,
    /// as the key is validated in `get_pipe()` and the lock is held).
    #[must_use]
    pub fn writer(&self) -> Option<&Writer> {
        if let Some(pipe) = self.guard.get(&self.key) {
            Some(pipe.writer())
        } else {
            error!(
                key = ?self.key,
                "CRITICAL: PipeRef key invalid despite lock held"
            );
            None
        }
    }

    /// Get a reader for this pipe with an explicit handle
    ///
    /// Returns `None` if the key is invalid (should never happen in practice,
    /// as the key is validated in `get_pipe()` and the lock is held).
    #[must_use]
    pub fn get_reader(&self, reader_handle: Handle) -> Option<Reader> {
        if let Some(pipe) = self.guard.get(&self.key) {
            Some(pipe.get_reader(reader_handle))
        } else {
            error!(
                key = ?self.key,
                "CRITICAL: PipeRef key invalid despite lock held"
            );
            None
        }
    }
}
