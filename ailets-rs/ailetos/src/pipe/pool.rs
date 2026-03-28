//! `PipePool` - manages output pipes for actors
//!
//! Each (actor, `StdHandle`) pair can have its own output pipe. Readers are created on-demand
//! when consuming actors need to read from dependencies.
//!
//! ## Latent Pipes
//!
//! Actors can start reading from dependencies before those dependencies have created their
//! output pipes. When a reader requests a non-existent pipe, a **latent writer** placeholder
//! is created. The reader waits on a `tokio::Notify` until the producer creates the pipe or
//! terminates.
//!
//! ## KV Storage for Terminated Actors
//!
//! Terminated actors store output in KV storage.
//! Value nodes are special: marked Terminated but never executed, with output only in KV.
//! Callers (like MergeReader) are responsible for checking KV when appropriate.

use std::sync::Arc;

use actor_runtime::StdHandle;
use parking_lot::Mutex;
use tracing::debug;

use super::allocator::create_writer;
use super::reader::Reader;
use super::writer::Writer;
use crate::idgen::{Handle, IdGen};
use crate::notification_queue::NotificationQueueArc;
use crate::storage::KVBuffers;

/// Error type for pipe reader operations
#[derive(Debug)]
pub enum PipeError {
    /// Pipe was closed by producer (latent pipe marked Closed)
    PipeClosed,
    /// Would block waiting for pipe but allow_latent=false
    WouldBlock,
    /// KV storage error
    Storage(crate::storage::KVError),
}

impl std::fmt::Display for PipeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PipeClosed => write!(f, "pipe closed by producer"),
            Self::WouldBlock => write!(f, "would block waiting for pipe"),
            Self::Storage(e) => write!(f, "storage error: {}", e),
        }
    }
}

impl std::error::Error for PipeError {}

/// State of a latent writer
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LatentState {
    /// Waiting for writer to be created
    Waiting,
    /// Producer terminated without creating the pipe - readers waiting for this pipe get None (EOF).
    /// Prevents readers from waiting forever when the producer crashes or exits early.
    Closed,
}

/// State of a writer in the pool
enum WriterState {
    /// Realized writer with data buffer
    Realized(Arc<Writer>),
    /// Latent writer - placeholder waiting for producer to create pipe
    Latent {
        state: LatentState,
        notify: Arc<tokio::sync::Notify>,
    },
}

/// Pool of output pipes, indexed by (actor handle, `StdHandle`) pair
///
/// Uses interior mutability via `Mutex` to allow shared access through `Arc<PipePool>`.
pub struct PipePool<K: KVBuffers> {
    /// Writers in various states (Realized or Latent)
    writers: Mutex<Vec<(Handle, StdHandle, WriterState)>>,
    /// Key-value store for pipe buffers
    kv: Arc<K>,
    /// Notification queue for pipe data events
    notification_queue: NotificationQueueArc,
}

impl<K: KVBuffers> PipePool<K> {
    /// Create a new empty pipe pool
    ///
    /// # Parameters
    /// - `kv`: Key-value store for pipe buffers
    #[must_use]
    pub fn new(kv: Arc<K>) -> Self {
        Self {
            writers: Mutex::new(Vec::new()),
            kv,
            notification_queue: NotificationQueueArc::new(),
        }
    }

    /// Get or create a reader for a pipe
    ///
    /// This method handles pipes in various states:
    /// - **Realized**: Returns reader immediately
    /// - **Latent (Waiting)**: Waits for pipe creation if allow_latent=true
    /// - **Latent (Closed)**: Returns PipeClosed error
    /// - **No entry**: Creates latent pipe if allow_latent=true, otherwise returns WouldBlock
    ///
    /// # Errors
    ///
    /// - `PipeClosed`: Producer closed latent pipe without creating it
    /// - `WouldBlock`: Pipe doesn't exist yet but allow_latent=false
    pub async fn get_or_await_reader(
        &self,
        key: (Handle, StdHandle),
        allow_latent: bool,
        id_gen: &IdGen,
    ) -> Result<Reader, PipeError> {
        loop {
            // Check state under lock and decide action
            let wait_notify = {
                let mut writers = self.writers.lock();

                // Find existing writer state
                let existing_state = writers
                    .iter()
                    .find(|(h, s, _)| (*h, *s) == key)
                    .map(|(_, _, state)| state);

                match existing_state {
                    Some(WriterState::Realized(writer)) => {
                        // Case 1: Writer exists - create reader immediately
                        let shared_data = writer.share_with_reader();
                        let reader_handle = Handle::new(id_gen.get_next());
                        return Ok(Reader::new(reader_handle, shared_data));
                    }
                    Some(WriterState::Latent { state: LatentState::Closed, .. }) => {
                        // Case 2: Producer terminated without creating pipe
                        return Err(PipeError::PipeClosed);
                    }
                    Some(WriterState::Latent { state: LatentState::Waiting, notify }) => {
                        // Case 3: Latent writer waiting for producer
                        if allow_latent {
                            Some(Arc::clone(notify))
                        } else {
                            return Err(PipeError::WouldBlock);
                        }
                    }
                    None => {
                        // Case 4: No entry exists
                        if !allow_latent {
                            return Err(PipeError::WouldBlock);
                        }
                        // Create latent writer
                        let notify = Arc::new(tokio::sync::Notify::new());
                        writers.push((key.0, key.1, WriterState::Latent {
                            state: LatentState::Waiting,
                            notify: Arc::clone(&notify),
                        }));
                        debug!(key = ?key, "created latent writer");
                        Some(notify)
                    }
                }
            }; // Lock released here

            // Wait for notification if needed, then loop back to recheck
            if let Some(notify) = wait_notify {
                notify.notified().await;
                continue;
            }
        }
    }

    /// Touch a writer - get existing or create new (idempotent)
    ///
    /// This is the primary method for getting a writer. It:
    /// - Returns existing writer if already realized
    /// - Realizes latent writer if it exists
    /// - Creates new writer if none exists
    ///
    /// Returns `(writer, was_newly_created)` where `was_newly_created` is true
    /// if this call created the writer (useful for triggering attachments).
    ///
    /// # Errors
    /// Returns error if buffer allocation fails
    pub async fn touch_writer(
        &self,
        actor_handle: Handle,
        std_handle: StdHandle,
        id_gen: &IdGen,
    ) -> Result<(Arc<Writer>, bool), crate::storage::KVError> {
        let key = (actor_handle, std_handle);

        // Fast path: writer already realized
        {
            let writers = self.writers.lock();
            let existing_state = writers
                .iter()
                .find(|(h, s, _)| (*h, *s) == key)
                .map(|(_, _, state)| state);

            if let Some(WriterState::Realized(writer)) = existing_state {
                return Ok((Arc::clone(writer), false));
            }
        }

        // Slow path: create writer
        let writer_handle = Handle::new(id_gen.get_next());
        let path = format!("pipes/actor-{}-{:?}", actor_handle.id(), std_handle);

        // Create writer with buffer from KV storage
        let writer = create_writer(
            self.kv.as_ref(),
            self.notification_queue.clone(),
            writer_handle,
            &path,
        )
        .await?;

        // Replace state with Realized and notify waiters if needed
        let writer_arc = Arc::new(writer);
        let notify_arc = {
            let mut writers = self.writers.lock();

            // Remove old state and extract notify handle if it was Latent
            let notify_arc = if let Some(pos) = writers.iter().position(|(h, s, _)| (*h, *s) == key) {
                let (_, _, old_state) = writers.remove(pos);
                match old_state {
                    WriterState::Latent { notify, .. } => Some(notify),
                    _ => None,
                }
            } else {
                None
            };

            // Insert Realized state
            writers.push((key.0, key.1, WriterState::Realized(Arc::clone(&writer_arc))));
            debug!(key = ?key, "created writer");

            notify_arc
        }; // Lock released here

        // Notify waiters outside lock
        if let Some(notify) = notify_arc {
            notify.notify_waiters();
            debug!(key = ?key, "notified waiters after creating writer");
        }

        Ok((writer_arc, true))
    }

    /// Close all writers (realized and latent) for an actor
    ///
    /// Called on actor shutdown to clean up all pipes for the actor.
    /// - For realized writers: calls `close()` on each
    /// - For latent writers (waiting): marks as closed and notifies waiting readers
    pub fn close_actor_writers(&self, actor_handle: Handle) {
        let (writers_to_close, notifies) = {
            let mut writers = self.writers.lock();

            let mut writers_to_close = Vec::new();
            let mut notifies = Vec::new();

            // Update states for this actor
            for (h, s, state) in &mut *writers {
                if *h == actor_handle {
                    match state {
                        WriterState::Realized(writer) => {
                            writers_to_close.push((*h, *s, Arc::clone(writer)));
                        }
                        WriterState::Latent { state: latent_state, notify } if *latent_state == LatentState::Waiting => {
                            *latent_state = LatentState::Closed;
                            notifies.push(Arc::clone(notify));
                            debug!(key = ?(*h, *s), "closed latent writer on actor shutdown");
                        }
                        _ => {}
                    }
                }
            }

            (writers_to_close, notifies)
        }; // Lock released here

        // Close writers outside lock
        for (h, s, writer) in writers_to_close {
            writer.close();
            debug!(key = ?(h, s), "closed realized writer on actor shutdown");
        }

        // Notify latent waiters outside lock
        for notify in notifies {
            notify.notify_waiters();
        }
    }

    /// Get a writer by key (only if already realized)
    ///
    /// Returns an Arc to the writer if it exists and has been realized.
    /// Returns None if the pipe doesn't exist or is still latent.
    ///
    /// The returned Arc shares ownership of the writer, preventing premature closure.
    pub fn get_already_realized_writer(&self, key: (Handle, StdHandle)) -> Option<Arc<Writer>> {
        let writers = self.writers.lock();
        let existing_state = writers
            .iter()
            .find(|(h, s, _)| (*h, *s) == key)
            .map(|(_, _, state)| state);

        match existing_state {
            Some(WriterState::Realized(writer)) => Some(Arc::clone(writer)),
            _ => None,
        }
    }
}
