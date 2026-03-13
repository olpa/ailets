//! `PipePool` - manages output pipes for actors
//!
//! Each (actor, `StdHandle`) pair can have its own output pipe. Readers are created on-demand
//! when consuming actors need to read from dependencies.
//!
//! ## Race-Free Pipe Lifecycle Design
//!
//! This implementation prevents race conditions between producer shutdown and consumer pipe
//! opening. See `pipe-lifecycle-implementation-guide.md` for the complete specification.
//!
//! ### Critical Design Decisions
//!
//! **Why: Single Mutex for All Operations**
//!
//! All pipe state operations (check existence, create latent, realize writer, close) happen
//! under the same `Mutex<PoolInner>`. This ensures atomic check-and-register:
//! - Consumer checks if writer exists AND registers latent waiter atomically
//! - Producer checks if latent exists AND notifies waiters atomically
//! - Shutdown extracts all waiters AND marks them closed atomically
//!
//! **Why: Notify Outside Lock**
//!
//! After extracting notify handles under lock, we call `notify_waiters()` AFTER releasing
//! the lock. This prevents deadlock when notified readers immediately try to re-acquire
//! the lock to get their reader instances.
//!
//! **Why: Latent State Machine (Waiting → Closed)**
//!
//! Latent writers track whether the producer is still capable of creating the pipe:
//! - `Waiting`: Producer is running, pipe may still be created
//! - `Closed`: Producer terminated without creating pipe, readers get EOF
//!
//! This state prevents orphaned waiters when actors crash or exit early.
//!
//! **Why: Loop-and-Recheck in `get_or_await_reader()`**
//!
//! After being notified, readers loop back to recheck state under lock. This handles:
//! - Spurious wakeups from `tokio::Notify`
//! - Race where writer is created just as we're setting up to wait
//! - Multiple readers waking up from same notification
//!
//! **Integration with Actor Lifecycle**
//!
//! The race-free guarantee depends on `SystemRuntime` calling operations in this order:
//! 1. Set actor state to TERMINATING (blocks new pipe requests)
//! 2. Call `close_actor_writers()` (wakes all waiting readers)
//! 3. Set actor state to TERMINATED (signals cleanup complete)
//!
//! See `system_runtime.rs` `ActorShutdown` handler for the implementation.
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
//! `PipePool` stores writers in two states:
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
//! let reader = pool.get_or_await_reader(key, allow_latent=true, id_gen).await?;
//!
//! // For explicit access: return None if pipe doesn't exist
//! let reader = pool.get_or_await_reader(key, allow_latent=false, id_gen).await?;
//! ```
//!
//! ### Getting Writers
//!
//! ```ignore
//! // Get or create writer (idempotent, always works)
//! let writer = pool.touch_writer(actor_handle, std_handle, id_gen).await?;
//! writer.write(data);
//! ```
//!
//! ### Closing Writers
//!
//! ```ignore
//! // Normal close (after writing) - call close() on the writer directly
//! let writer = pool.get_already_realized_writer((actor_handle, std_handle)).unwrap();
//! writer.close();
//!
//! // On actor shutdown - close all writers (realized and latent) for the actor
//! pool.close_actor_writers(actor_handle);
//! ```
//!
//! ## Coordination via Notify
//!
//! All latent pipe coordination uses `tokio::sync::Notify`:
//!
//! 1. **Reader path**: `get_or_await_reader()` creates latent entry, awaits notify
//! 2. **Writer path**: `touch_writer()` removes latent entry, calls `notify_waiters()`
//! 3. **Shutdown path**: `close_actor_writers()` closes all writers for an actor
//!
//! This ensures readers never miss notifications and wake up exactly once.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use actor_runtime::StdHandle;
use parking_lot::Mutex;
use tracing::{debug, trace};

use crate::idgen::{Handle, IdGen};
use crate::io::{KVBuffers, OpenMode};
use crate::notification_queue::NotificationQueueArc;
use crate::pipe::{Reader, Writer};

/// Callback type for writer realization events
pub type WriterRealizedCallback = Arc<
    dyn Fn(Handle, StdHandle) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync,
>;

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
    writers: Vec<(Handle, StdHandle, Arc<Writer>)>,
}

impl PoolInner {
    fn new() -> Self {
        Self {
            latent_writers: Vec::new(),
            writers: Vec::new(),
        }
    }

    /// Find a writer by key
    fn find_writer(&self, key: (Handle, StdHandle)) -> Option<&Arc<Writer>> {
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

/// Pool of output pipes, indexed by (actor handle, `StdHandle`) pair
///
/// Uses interior mutability via `Mutex` to allow shared access through `Arc<PipePool>`.
pub struct PipePool<K: KVBuffers> {
    /// Inner state (readers, `latent_writers`, writers)
    inner: Mutex<PoolInner>,
    /// Shared notification queue for all pipes
    notification_queue: NotificationQueueArc,
    /// Key-value store for pipe buffers
    kv: Arc<K>,
    /// Callback invoked when a writer is realized (uses interior mutability)
    on_writer_realized: parking_lot::RwLock<Option<WriterRealizedCallback>>,
}

impl<K: KVBuffers> PipePool<K> {
    /// Create a new empty pipe pool
    #[must_use]
    pub fn new(kv: Arc<K>, notification_queue: NotificationQueueArc) -> Self {
        Self {
            inner: Mutex::new(PoolInner::new()),
            notification_queue,
            kv,
            on_writer_realized: parking_lot::RwLock::new(None),
        }
    }

    /// Set the callback for writer realization events
    ///
    /// This can be called after construction to set up the callback.
    pub fn set_writer_realized_callback(&self, callback: WriterRealizedCallback) {
        *self.on_writer_realized.write() = Some(callback);
    }

    /// Clear the callback to break circular references before shutdown
    pub fn clear_writer_realized_callback(&self) {
        *self.on_writer_realized.write() = None;
    }

    /// Get or create a reader for a pipe
    ///
    /// If writer exists: creates reader immediately
    /// If latent writer exists:
    ///   - If closed: returns None
    ///   - If waiting: awaits until realized or closed
    ///
    /// If `allow_latent`: creates latent writer and awaits.
    /// Otherwise: returns None.
    pub async fn get_or_await_reader(
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
                            // Only wait on latent writer if allow_latent is true
                            if allow_latent {
                                let notify = Arc::clone(&latent.notify);
                                Some(notify)
                            } else {
                                return None;
                            }
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

    /// Touch a writer - get existing or create new (idempotent)
    ///
    /// This is the primary method for getting a writer. It:
    /// - Returns existing writer if already realized
    /// - Realizes latent writer if it exists
    /// - Creates new writer if none exists
    ///
    /// Always returns a writer ready to use.
    ///
    /// # Errors
    /// Returns error if buffer allocation fails
    pub async fn touch_writer(
        &self,
        actor_handle: Handle,
        std_handle: StdHandle,
        id_gen: &IdGen,
    ) -> Result<Arc<Writer>, crate::io::KVError> {
        let key = (actor_handle, std_handle);

        // Fast path: writer already exists
        {
            let inner = self.inner.lock();
            if let Some(writer) = inner.find_writer(key) {
                return Ok(Arc::clone(writer));
            }
        }

        // Slow path: need to realize or create
        let writer_handle = Handle::new(id_gen.get_next());
        let name = format!("pipes/actor-{}-{:?}", actor_handle.id(), std_handle);

        // Allocate buffer
        let buffer = self.kv.open(&name, OpenMode::Write).await?;

        // Create writer
        let writer = Writer::new(
            writer_handle,
            self.notification_queue.clone(),
            &name,
            buffer,
        );

        // Add writer to pool and notify waiters (if latent existed)
        let (writer_arc, was_newly_created) = {
            let mut inner = self.inner.lock();

            // Check if latent writer exists - remove it and get notify handle
            let notify_arc = inner.remove_latent_writer(key).map(|lw| lw.notify);
            let was_newly_created = true; // We're in the slow path, so this is a new writer

            // Add writer wrapped in Arc
            let writer_arc = Arc::new(writer);
            inner.writers.push((key.0, key.1, Arc::clone(&writer_arc)));
            debug!(key = ?key, "created writer");

            // Drop lock before notifying
            drop(inner);

            // Notify waiters (outside lock)
            if let Some(notify) = notify_arc {
                notify.notify_waiters();
                debug!(key = ?key, "notified waiters after creating writer");
            }

            (writer_arc, was_newly_created)
        };

        // Invoke callback if writer was newly created
        if was_newly_created {
            let callback = self.on_writer_realized.read().clone();
            if let Some(callback) = callback {
                callback(actor_handle, std_handle).await;
            }
        }

        Ok(writer_arc)
    }

    /// Close all writers (realized and latent) for an actor
    ///
    /// Called on actor shutdown to clean up all pipes for the actor.
    /// - For realized writers: calls `close()` on each
    /// - For latent writers: marks as closed and notifies waiting readers
    pub fn close_actor_writers(&self, actor_handle: Handle) {
        let (writers_to_close, notifies) = {
            let mut inner = self.inner.lock();

            // Collect latent notifies
            let mut notifies = Vec::new();
            for latent in &mut inner.latent_writers {
                if latent.key.0 == actor_handle && latent.state == LatentState::Waiting {
                    latent.state = LatentState::Closed;
                    notifies.push(Arc::clone(&latent.notify));
                    debug!(key = ?latent.key, "closed latent writer on actor shutdown");
                }
            }

            // Collect realized writers to close (clone Arc)
            let writers_to_close: Vec<_> = inner
                .writers
                .iter()
                .filter(|(h, _, _)| *h == actor_handle)
                .map(|(h, s, w)| (*h, *s, Arc::clone(w)))
                .collect();

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
        let inner = self.inner.lock();
        inner.find_writer(key).cloned()
    }
}
