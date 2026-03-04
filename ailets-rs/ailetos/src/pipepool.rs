//! `PipePool` - manages output pipes for actors
//!
//! Each actor has one output pipe. Readers are created on-demand
//! when consuming actors need to read from dependencies.

use std::sync::Arc;

use parking_lot::Mutex;
use tracing::error;

use crate::idgen::{Handle, IdGen};
use crate::io::{KVBuffers, OpenMode};
use crate::notification_queue::NotificationQueueArc;
use crate::pipe::{Pipe, Reader, Writer};

/// Pool of output pipes, one per actor (identified by Handle)
///
/// Uses interior mutability via `Mutex` to allow shared access through `Arc<PipePool>`.
pub struct PipePool<K: KVBuffers> {
    /// Output pipes indexed by producing actor's handle
    pipes: Mutex<Vec<(Handle, Pipe)>>,
    /// Shared notification queue for all pipes
    notification_queue: NotificationQueueArc,
    /// Key-value store for pipe buffers
    kv: Arc<K>,
}

impl<K: KVBuffers> PipePool<K> {
    /// Create a new empty pipe pool
    #[must_use]
    pub fn new(kv: Arc<K>, notification_queue: NotificationQueueArc) -> Self {
        Self {
            pipes: Mutex::new(Vec::new()),
            notification_queue,
            kv,
        }
    }

    /// Create an output pipe for the given actor
    ///
    /// # Errors
    /// Returns an error if creating the buffer fails or if the actor already has a pipe
    pub async fn create_output_pipe(
        &self,
        actor_handle: Handle,
        name: &str,
        id_gen: &IdGen,
    ) -> Result<Handle, crate::io::KVError> {
        // Check if actor already has a pipe (lock scope)
        {
            let pipes = self.pipes.lock();
            if pipes.iter().any(|(h, _)| *h == actor_handle) {
                return Err(crate::io::KVError::AlreadyExists(format!(
                    "Actor {actor_handle:?} already has an output pipe"
                )));
            }
        }

        let writer_handle = Handle::new(id_gen.get_next());

        // Get buffer from KV store (outside lock)
        let buffer = self.kv.open(name, OpenMode::Write).await?;

        let pipe = Pipe::new(writer_handle, self.notification_queue.clone(), name, buffer);

        // Insert pipe (lock scope)
        let mut pipes = self.pipes.lock();
        pipes.push((actor_handle, pipe));

        Ok(writer_handle)
    }

    /// Check if a pipe exists for the given actor
    #[must_use]
    pub fn has_pipe(&self, actor_handle: Handle) -> bool {
        let pipes = self.pipes.lock();
        pipes.iter().any(|(h, _)| *h == actor_handle)
    }

    /// Get the pipe for the given actor
    ///
    /// Returns `None` if the actor doesn't have an output pipe.
    #[must_use]
    pub fn get_pipe(&self, actor_handle: Handle) -> Option<PipeRef<'_>> {
        let pipes = self.pipes.lock();
        let index = pipes.iter().position(|(h, _)| *h == actor_handle)?;
        Some(PipeRef {
            guard: pipes,
            index,
        })
    }

    /// Open a reader for the given actor's output pipe
    ///
    /// Creates a new Reader instance. Multiple readers can be created
    /// for the same pipe.
    ///
    /// Returns `None` if the actor doesn't have an output pipe.
    #[must_use]
    pub fn open_reader(&self, actor_handle: Handle, id_gen: &IdGen) -> Option<Reader> {
        let pipe_ref = self.get_pipe(actor_handle)?;
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
    pub async fn flush_buffer(&self, actor_handle: Handle) -> Result<(), crate::io::KVError> {
        let buffer = {
            let pipe_ref = self.get_pipe(actor_handle).ok_or_else(|| {
                crate::io::KVError::NotFound(format!("Pipe for actor {actor_handle:?}"))
            })?;
            let writer = pipe_ref.writer().ok_or_else(|| {
                crate::io::KVError::NotFound(format!("Writer for actor {actor_handle:?}"))
            })?;
            writer.buffer()
        }; // pipe_ref dropped here, lock released
        self.kv.flush_buffer(&buffer).await
    }
}

/// A reference to a pipe in the pool, holding the lock
pub struct PipeRef<'a> {
    guard: parking_lot::MutexGuard<'a, Vec<(Handle, Pipe)>>,
    index: usize,
}

impl PipeRef<'_> {
    /// Get the writer side of the pipe
    ///
    /// Returns `None` if the index is invalid (should never happen in practice,
    /// as the index is validated in `get_pipe()` and the lock is held).
    #[must_use]
    pub fn writer(&self) -> Option<&Writer> {
        if let Some((_, pipe)) = self.guard.get(self.index) {
            Some(pipe.writer())
        } else {
            error!(
                index = self.index,
                "CRITICAL: PipeRef index invalid despite lock held"
            );
            None
        }
    }

    /// Get a reader for this pipe with an explicit handle
    ///
    /// Returns `None` if the index is invalid (should never happen in practice,
    /// as the index is validated in `get_pipe()` and the lock is held).
    #[must_use]
    pub fn get_reader(&self, reader_handle: Handle) -> Option<Reader> {
        if let Some((_, pipe)) = self.guard.get(self.index) {
            Some(pipe.get_reader(reader_handle))
        } else {
            error!(
                index = self.index,
                "CRITICAL: PipeRef index invalid despite lock held"
            );
            None
        }
    }
}
