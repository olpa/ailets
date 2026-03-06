//! `PipePool` - manages output pipes for actors
//!
//! Each (actor, StdHandle) pair can have its own output pipe. Readers are created on-demand
//! when consuming actors need to read from dependencies.

use std::collections::HashMap;
use std::sync::Arc;

use actor_runtime::StdHandle;
use parking_lot::Mutex;
use tracing::error;

use crate::idgen::{Handle, IdGen};
use crate::io::{KVBuffers, OpenMode};
use crate::notification_queue::NotificationQueueArc;
use crate::pipe::{Pipe, Reader, Writer};

/// Controls how pipe access behaves when pipe doesn't exist
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipeAccess {
    /// Only access existing pipes
    /// Returns None if pipe doesn't exist
    ExistingOnly,

    /// Create a latent pipe if it doesn't exist
    /// Always returns Some (creates latent pipe on miss)
    OrCreateLatent,
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
}

impl<K: KVBuffers> PipePool<K> {
    /// Create a new empty pipe pool
    #[must_use]
    pub fn new(kv: Arc<K>, notification_queue: NotificationQueueArc) -> Self {
        Self {
            pipes: Mutex::new(HashMap::new()),
            notification_queue,
            kv,
        }
    }

    /// Create output pipe (legacy method - now realizes immediately)
    ///
    /// This maintains compatibility with existing code that creates
    /// pipes eagerly. New code should use realize_pipe instead.
    ///
    /// # Errors
    /// Returns an error if creating the buffer fails or if the pipe already exists
    pub async fn create_output_pipe(
        &self,
        actor_handle: Handle,
        std_handle: StdHandle,
        _name: &str,
        id_gen: &IdGen,
    ) -> Result<Handle, crate::io::KVError> {
        let key = (actor_handle, std_handle);

        // Check if pipe already exists
        {
            let pipes = self.pipes.lock();
            if pipes.contains_key(&key) {
                return Err(crate::io::KVError::AlreadyExists(format!(
                    "Pipe for actor {actor_handle:?} handle {std_handle:?} already exists"
                )));
            }
        }

        let writer_handle = Handle::new(id_gen.get_next());
        self.realize_pipe(actor_handle, std_handle, writer_handle)
            .await?;
        Ok(writer_handle)
    }

    /// Check if a pipe exists for the given actor and handle
    #[must_use]
    pub fn has_pipe(&self, actor_handle: Handle, std_handle: StdHandle) -> bool {
        let pipes = self.pipes.lock();
        pipes.contains_key(&(actor_handle, std_handle))
    }

    /// Get a pipe with controlled latent pipe creation
    ///
    /// # Arguments
    ///
    /// * `actor_handle` - The actor that owns the pipe
    /// * `std_handle` - Which standard handle (Stdout/Stderr/Log)
    /// * `access` - Whether to create latent pipe if missing
    ///
    /// # Returns
    ///
    /// * `Some(PipeRef)` - Pipe exists (realized or latent)
    /// * `None` - Pipe doesn't exist and `access == ExistingOnly`
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Get existing pipe only (for closing)
    /// let pipe = pool.get_pipe(handle, StdHandle::Stdout, PipeAccess::ExistingOnly)?;
    ///
    /// // Get or create latent pipe (for reading dependencies or attachments)
    /// let pipe = pool.get_pipe(handle, StdHandle::Stdout, PipeAccess::OrCreateLatent)?;
    /// ```
    #[must_use]
    pub fn get_pipe(
        &self,
        actor_handle: Handle,
        std_handle: StdHandle,
        access: PipeAccess,
    ) -> Option<PipeRef<'_>> {
        let mut pipes = self.pipes.lock();
        let key = (actor_handle, std_handle);

        match access {
            PipeAccess::ExistingOnly => {
                // Only return if exists
                if pipes.contains_key(&key) {
                    Some(PipeRef { guard: pipes, key })
                } else {
                    None
                }
            }
            PipeAccess::OrCreateLatent => {
                // Create latent if missing
                pipes.entry(key).or_insert_with(|| {
                    let name = format!("pipes/actor-{}-{:?}", actor_handle.id(), std_handle);
                    Pipe::new_latent(name, self.notification_queue.clone())
                });
                Some(PipeRef { guard: pipes, key })
            }
        }
    }

    /// Open a reader for a pipe (creates latent pipe if needed)
    ///
    /// This is the primary method for getting readers, including for attachments.
    /// If the pipe doesn't exist, a latent pipe is created and the reader will
    /// block until the pipe is realized.
    #[must_use]
    pub fn open_reader(
        &self,
        actor_handle: Handle,
        std_handle: StdHandle,
        id_gen: &IdGen,
    ) -> Option<Reader> {
        // Always use OrCreateLatent for readers
        let pipe_ref = self.get_pipe(actor_handle, std_handle, PipeAccess::OrCreateLatent)?;
        let reader_handle = Handle::new(id_gen.get_next());
        pipe_ref.get_reader(reader_handle)
    }

    /// Realize a latent pipe (called on first write)
    ///
    /// If pipe doesn't exist, creates it as realized directly.
    /// If pipe is latent, transitions it to realized.
    /// If pipe is already realized, this is a no-op.
    ///
    /// # Errors
    ///
    /// Returns error if buffer allocation fails
    pub async fn realize_pipe(
        &self,
        actor_handle: Handle,
        std_handle: StdHandle,
        writer_handle: Handle,
    ) -> Result<(), crate::io::KVError> {
        let name = format!("pipes/actor-{}-{:?}", actor_handle.id(), std_handle);
        let key = (actor_handle, std_handle);

        // Check if pipe exists and determine what action to take
        let needs_realization = {
            let pipes = self.pipes.lock();
            if let Some(pipe) = pipes.get(&key) {
                !pipe.is_realized()
            } else {
                false // Will create new pipe below
            }
        };

        if needs_realization {
            // Pipe exists and is latent - realize it
            let buffer = self.kv.open(&name, OpenMode::Write).await?;
            let mut pipes = self.pipes.lock();
            if let Some(pipe) = pipes.get_mut(&key) {
                pipe.realize(writer_handle, buffer);
            }
        } else {
            // Check if pipe exists now (might have been created elsewhere)
            let exists = {
                let pipes = self.pipes.lock();
                pipes.contains_key(&key)
            };

            if !exists {
                // Pipe doesn't exist - create as realized directly
                let buffer = self.kv.open(&name, OpenMode::Write).await?;
                let pipe = Pipe::new_realized(
                    writer_handle,
                    self.notification_queue.clone(),
                    name,
                    buffer,
                );

                let mut pipes = self.pipes.lock();
                pipes.insert(key, pipe);
            }
        }

        Ok(())
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
            let pipe_ref =
                self.get_pipe(actor_handle, std_handle, PipeAccess::ExistingOnly)
                    .ok_or_else(|| {
                        crate::io::KVError::NotFound(format!(
                            "Pipe for actor {actor_handle:?} handle {std_handle:?}"
                        ))
                    })?;
            pipe_ref.buffer().ok_or_else(|| {
                crate::io::KVError::NotFound(format!(
                    "Buffer for actor {actor_handle:?} handle {std_handle:?} (pipe not realized)"
                ))
            })?
        }; // pipe_ref dropped here, lock released
        self.kv.flush_buffer(&buffer).await
    }
}

/// A reference to a pipe in the pool, holding the lock
pub struct PipeRef<'a> {
    guard: parking_lot::MutexGuard<'a, HashMap<(Handle, StdHandle), Pipe>>,
    key: (Handle, StdHandle),
}

impl PipeRef<'_> {
    /// Get the state of the pipe for performing operations
    ///
    /// Returns `None` if the key is invalid (should never happen in practice,
    /// as the key is validated in `get_pipe()` and the lock is held).
    #[must_use]
    pub fn state(&self) -> Option<crate::pipe::PipeStateArc> {
        if let Some(pipe) = self.guard.get(&self.key) {
            Some(pipe.state())
        } else {
            error!(
                key = ?self.key,
                "CRITICAL: PipeRef key invalid despite lock held"
            );
            None
        }
    }

    /// Get the buffer (only for realized pipes)
    ///
    /// Returns `None` if the key is invalid or if the pipe is not realized.
    #[must_use]
    pub fn buffer(&self) -> Option<crate::io::Buffer> {
        if let Some(pipe) = self.guard.get(&self.key) {
            pipe.buffer()
        } else {
            error!(
                key = ?self.key,
                "CRITICAL: PipeRef key invalid despite lock held"
            );
            None
        }
    }

    /// Write data to the pipe (only works on realized pipes)
    ///
    /// Returns the number of bytes written, or -1 on error.
    /// Returns None if the pipe is not realized or doesn't exist.
    #[must_use]
    pub fn write(&self, data: &[u8]) -> Option<isize> {
        if let Some(pipe) = self.guard.get(&self.key) {
            let state = pipe.state();
            let state_guard = state.lock();
            if let crate::pipe::PipeState::Realized { writer, .. } = &*state_guard {
                Some(writer.write(data))
            } else {
                None
            }
        } else {
            error!(
                key = ?self.key,
                "CRITICAL: PipeRef key invalid despite lock held"
            );
            None
        }
    }

    /// Close the writer (handles both latent and realized pipes)
    pub fn close_writer(&self) {
        if let Some(pipe) = self.guard.get(&self.key) {
            let state = pipe.state();
            let state_guard = state.lock();
            match &*state_guard {
                crate::pipe::PipeState::Realized { writer, .. } => {
                    // Close realized pipe
                    writer.close();
                }
                crate::pipe::PipeState::Latent { .. } => {
                    // Drop lock before calling close_latent (it acquires the same lock)
                    drop(state_guard);
                    // Close latent pipe (transitions to ClosedWithoutData)
                    pipe.close_latent();
                }
                crate::pipe::PipeState::ClosedWithoutData => {
                    // Already closed - no-op
                }
            }
        } else {
            error!(
                key = ?self.key,
                "CRITICAL: PipeRef key invalid despite lock held"
            );
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
