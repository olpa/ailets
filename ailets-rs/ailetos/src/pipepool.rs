//! `PipePool` - manages output pipes for actors
//!
//! Each (actor, StdHandle) pair can have its own output pipe. Readers are created on-demand
//! when consuming actors need to read from dependencies.
//!
//! ## Problem: Dependency Race Conditions
//!
//! Actors can start reading from dependencies before those dependencies have created their
//! output pipes. This creates a coordination problem:
//!
//! - **Consumer** (e.g., `cat`) needs to read from dependency's stdout
//! - **Producer** (e.g., upstream actor) hasn't created its output pipe yet
//! - Consumer should **wait** for the pipe to be created, not fail
//!
//! ## Solution: Latent Pipes
//!
//! When a reader requests a pipe that doesn't exist yet:
//! 1. Create a **latent writer** placeholder with a `tokio::Notify`
//! 2. Reader awaits on the notify
//! 3. When writer is eventually created, notify all waiting readers
//! 4. Readers wake up and get their `Reader` instances
//!
//! ## Design: Two Vectors
//!
//! PipePool stores writers in two states:
//!
//! - **`latent_writers: Vec<LatentWriter>`** - Placeholders for pipes not yet created
//!   - Contains `(Handle, StdHandle)` key, state (Waiting/Closed), and notify handle
//!   - Created when reader requests non-existent pipe with `allow_latent=true`
//!   - Removed when writer is created (transitions to `writers`)
//!   - Set to `Closed` state if actor exits without writing (prevents infinite wait)
//!
//! - **`writers: Vec<(Handle, StdHandle, Writer)>`** - Active pipes with data buffers
//!   - Created on first write or explicit pipe creation
//!   - Notifies all waiting readers when created
//!   - Supports multiple readers via `Writer::share_with_reader()`
//!
//! **Readers are not stored** - created on-demand from writers and returned to callers.
//!
//! ## Key Operations
//!
//! ### Creating Readers (async)
//!
//! ```ignore
//! // For dependencies: wait if pipe doesn't exist yet
//! let reader = pool.get_or_create_reader(key, allow_latent=true, id_gen).await?;
//!
//! // For explicit access: return None if pipe doesn't exist
//! let reader = pool.get_or_create_reader(key, allow_latent=false, id_gen).await?;
//! ```
//!
//! ### Creating Writers
//!
//! ```ignore
//! // First write from actor - realizes the pipe
//! pool.realize_pipe(actor_handle, std_handle, writer_handle).await?;
//!
//! // Get writer and write directly
//! let writer = pool.get_writer((actor_handle, std_handle))?;
//! writer.write(data);
//! ```
//!
//! ### Closing Writers
//!
//! ```ignore
//! // Normal close (after writing)
//! pool.close_writer((actor_handle, std_handle));
//!
//! // Abnormal close (latent pipe never realized) - logs warning
//! pool.close_writer((actor_handle, std_handle));  // Wakes readers with None
//! ```
//!
//! ## Coordination via Notify
//!
//! All latent pipe coordination uses `tokio::sync::Notify`:
//!
//! 1. **Reader path**: `get_or_create_reader()` creates latent entry, awaits notify
//! 2. **Writer path**: `create_writer()` removes latent entry, calls `notify_waiters()`
//! 3. **Close path**: `close_writer()` marks latent as Closed, calls `notify_waiters()`
//!
//! This ensures readers never miss notifications and wake up exactly once.

use std::sync::Arc;

use actor_runtime::StdHandle;
use parking_lot::Mutex;
use tracing::{debug, warn};

use crate::idgen::{Handle, IdGen};
use crate::io::{KVBuffers, OpenMode};
use crate::notification_queue::NotificationQueueArc;
use crate::pipe::{Reader, Writer};

/// State of a latent writer
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LatentState {
    /// Waiting for writer to be created
    Waiting,
    /// Closed without ever being realized
    Closed,
}

/// A placeholder for a writer that hasn't been created yet
pub struct LatentWriter {
    key: (Handle, StdHandle),
    state: LatentState,
    notify: Arc<tokio::sync::Notify>,
}

/// Inner state of the pipe pool
struct PoolInner {
    latent_writers: Vec<LatentWriter>,
    writers: Vec<(Handle, StdHandle, Writer)>,
}

impl PoolInner {
    fn new() -> Self {
        Self {
            latent_writers: Vec::new(),
            writers: Vec::new(),
        }
    }

    /// Find a writer by key
    fn find_writer(&self, key: (Handle, StdHandle)) -> Option<&Writer> {
        self.writers
            .iter()
            .find(|(h, s, _)| (*h, *s) == key)
            .map(|(_, _, w)| w)
    }

    /// Find a latent writer by key
    fn find_latent_writer(&self, key: (Handle, StdHandle)) -> Option<&LatentWriter> {
        self.latent_writers.iter().find(|lw| lw.key == key)
    }

    /// Remove a latent writer by key and return it
    fn remove_latent_writer(&mut self, key: (Handle, StdHandle)) -> Option<LatentWriter> {
        if let Some(pos) = self.latent_writers.iter().position(|lw| lw.key == key) {
            Some(self.latent_writers.remove(pos))
        } else {
            None
        }
    }
}

/// Pool of output pipes, indexed by (actor handle, StdHandle) pair
///
/// Uses interior mutability via `Mutex` to allow shared access through `Arc<PipePool>`.
pub struct PipePool<K: KVBuffers> {
    /// Inner state (readers, latent_writers, writers)
    inner: Mutex<PoolInner>,
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
            inner: Mutex::new(PoolInner::new()),
            notification_queue,
            kv,
        }
    }

    /// Get or create a reader for a pipe
    ///
    /// If writer exists: creates reader immediately
    /// If latent writer exists:
    ///   - If closed: returns None
    ///   - If waiting: awaits until realized or closed
    /// If allow_latent: creates latent writer and awaits
    /// Otherwise: returns None
    pub async fn get_or_create_reader(
        &self,
        key: (Handle, StdHandle),
        allow_latent: bool,
        id_gen: &IdGen,
    ) -> Option<Reader> {
        loop {
            // Check what state we're in
            let notify_arc = {
                let mut inner = self.inner.lock();

                // Check if writer exists
                if let Some(writer) = inner.find_writer(key) {
                    let shared_data = writer.share_with_reader();
                    let reader_handle = Handle::new(id_gen.get_next());
                    return Some(Reader::new(reader_handle, shared_data));
                }

                // Check if latent writer exists
                if let Some(latent) = inner.find_latent_writer(key) {
                    match latent.state {
                        LatentState::Closed => {
                            return None;
                        }
                        LatentState::Waiting => {
                            let notify = Arc::clone(&latent.notify);
                            Some(notify)
                        }
                    }
                } else if allow_latent {
                    // Create latent writer
                    let notify = Arc::new(tokio::sync::Notify::new());
                    let latent = LatentWriter {
                        key,
                        state: LatentState::Waiting,
                        notify: Arc::clone(&notify),
                    };
                    inner.latent_writers.push(latent);
                    debug!(key = ?key, "created latent writer");
                    Some(notify)
                } else {
                    return None;
                }
            };

            // If we got a notify arc, wait on it
            if let Some(notify) = notify_arc {
                notify.notified().await;
                // Loop back to check state again
            } else {
                // No notify arc means we returned already
                break;
            }
        }

        None
    }

    /// Create a writer for a pipe
    ///
    /// If a latent writer exists, removes it and notifies waiters
    pub fn create_writer(&self, key: (Handle, StdHandle), writer: Writer) {
        let mut inner = self.inner.lock();

        // Check if latent writer exists
        let notify_arc = inner.remove_latent_writer(key).map(|lw| lw.notify);

        // Add writer
        inner.writers.push((key.0, key.1, writer));
        debug!(key = ?key, "created writer");

        // Drop lock before notifying
        drop(inner);

        // Notify waiters
        if let Some(notify) = notify_arc {
            notify.notify_waiters();
            debug!(key = ?key, "notified waiters after creating writer");
        }
    }

    /// Close a writer
    ///
    /// If latent writer exists: marks it as closed and notifies waiters
    /// If writer exists: closes it
    pub fn close_writer(&self, key: (Handle, StdHandle)) {
        let mut inner = self.inner.lock();

        // Check for latent writer
        if let Some(latent) = inner
            .latent_writers
            .iter_mut()
            .find(|lw| lw.key == key)
        {
            if latent.state == LatentState::Waiting {
                latent.state = LatentState::Closed;
                let notify = Arc::clone(&latent.notify);
                drop(inner);
                notify.notify_waiters();
                warn!(key = ?key, "closed latent writer without realizing (abnormal)");
                return;
            }
        }

        // Check for realized writer
        if let Some((_, _, writer)) = inner.writers.iter().find(|(h, s, _)| (*h, *s) == key) {
            writer.close();
            debug!(key = ?key, "closed writer");
        }
    }

    /// Get a writer by key
    ///
    /// Returns a clone of the writer if it exists (realized).
    /// Returns None if the pipe doesn't exist or is still latent.
    ///
    /// The returned writer shares the same underlying buffer (via Arc),
    /// so writes through the clone affect the same pipe.
    pub fn get_writer(&self, key: (Handle, StdHandle)) -> Option<Writer> {
        let inner = self.inner.lock();
        inner.find_writer(key).cloned()
    }

    /// Check if a pipe exists (writer or latent writer)
    #[must_use]
    pub fn has_pipe(&self, actor_handle: Handle, std_handle: StdHandle) -> bool {
        let inner = self.inner.lock();
        let key = (actor_handle, std_handle);
        inner.find_writer(key).is_some() || inner.find_latent_writer(key).is_some()
    }

    /// Create output pipe (legacy compatibility method)
    ///
    /// Creates a writer immediately (no latent state)
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
        // Check if pipe already exists
        if self.has_pipe(actor_handle, std_handle) {
            return Err(crate::io::KVError::AlreadyExists(format!(
                "Pipe for actor {actor_handle:?} handle {std_handle:?} already exists"
            )));
        }

        let writer_handle = Handle::new(id_gen.get_next());
        self.realize_pipe(actor_handle, std_handle, writer_handle)
            .await?;
        Ok(writer_handle)
    }

    /// Realize a pipe (create writer with buffer)
    ///
    /// If pipe doesn't exist or is latent, creates the writer
    /// If writer already exists, this is a no-op
    ///
    /// # Errors
    /// Returns error if buffer allocation fails
    pub async fn realize_pipe(
        &self,
        actor_handle: Handle,
        std_handle: StdHandle,
        writer_handle: Handle,
    ) -> Result<(), crate::io::KVError> {
        let key = (actor_handle, std_handle);
        let name = format!("pipes/actor-{}-{:?}", actor_handle.id(), std_handle);

        // Check if writer already exists
        {
            let inner = self.inner.lock();
            if inner.find_writer(key).is_some() {
                return Ok(()); // Already realized
            }
        }

        // Allocate buffer
        let buffer = self.kv.open(&name, OpenMode::Write).await?;

        // Create writer
        let writer = Writer::new(
            writer_handle,
            self.notification_queue.clone(),
            &name,
            buffer,
        );

        self.create_writer(key, writer);
        Ok(())
    }

    /// Open a reader for a pipe (creates latent pipe if needed)
    ///
    /// This is the primary method for getting readers, including for attachments.
    pub async fn open_reader(
        &self,
        actor_handle: Handle,
        std_handle: StdHandle,
        id_gen: &IdGen,
    ) -> Option<Reader> {
        let key = (actor_handle, std_handle);
        self.get_or_create_reader(key, true, id_gen).await
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
            let inner = self.inner.lock();
            let key = (actor_handle, std_handle);
            inner
                .find_writer(key)
                .map(|writer| writer.buffer())
                .ok_or_else(|| {
                    crate::io::KVError::NotFound(format!(
                        "Pipe for actor {actor_handle:?} handle {std_handle:?}"
                    ))
                })?
        };
        self.kv.flush_buffer(&buffer).await
    }
}
