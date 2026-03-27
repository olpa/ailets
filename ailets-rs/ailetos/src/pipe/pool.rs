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
//! ## Coordination via Notify
//!
//! All latent pipe coordination uses `tokio::sync::Notify`:
//!
//! 1. **Reader path**: `get_or_await_reader()` creates latent entry, awaits notify
//! 2. **Writer path**: `touch_writer()` removes latent entry, calls `notify_waiters()`
//! 3. **Shutdown path**: `close_actor_writers()` closes all writers for an actor
//!
//! This ensures readers never miss notifications and wake up exactly once.

use std::sync::Arc;

use actor_runtime::StdHandle;
use parking_lot::Mutex;
use tracing::debug;

use super::allocator::{create_reader_from_completed, create_writer};
use super::reader::Reader;
use super::writer::Writer;
use crate::dag::{Dag, NodeState};
use crate::idgen::{Handle, IdGen};
use crate::notification_queue::NotificationQueueArc;
use crate::storage::KVBuffers;
use parking_lot::RwLock;

/// Error type for pipe reader operations
#[derive(Debug)]
pub enum PipeError {
    /// Pipe was closed by producer (latent pipe marked Closed)
    PipeClosed,
    /// Would block waiting for pipe but allow_latent=false
    WouldBlock,
    /// Pipe not found (e.g., terminated actor with no KV data)
    NotFound,
    /// Actor in invalid state for pipe creation (Running/Terminating without pipe)
    InvalidState(NodeState),
    /// Actor not found in DAG
    ActorNotFound,
    /// KV storage error
    Storage(crate::storage::KVError),
}

impl std::fmt::Display for PipeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PipeClosed => write!(f, "pipe closed by producer"),
            Self::WouldBlock => write!(f, "would block waiting for pipe"),
            Self::NotFound => write!(f, "pipe not found"),
            Self::InvalidState(state) => write!(f, "actor in invalid state: {:?}", state),
            Self::ActorNotFound => write!(f, "actor not found in DAG"),
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
    /// KVCheck marker - producer terminated, readers should check KV storage
    KVCheck,
}

/// Pool of output pipes, indexed by (actor handle, `StdHandle`) pair
///
/// Uses interior mutability via `Mutex` to allow shared access through `Arc<PipePool>`.
pub struct PipePool<K: KVBuffers> {
    /// Writers in various states (Realized, Latent, or KVCheck)
    writers: Mutex<Vec<(Handle, StdHandle, WriterState)>>,
    /// Key-value store for pipe buffers
    kv: Arc<K>,
    /// Notification queue for pipe data events
    notification_queue: NotificationQueueArc,
    /// DAG for checking producer node state (spec://pipe/pool.md#fulfillable-open)
    dag: Arc<RwLock<Dag>>,
}

impl<K: KVBuffers> PipePool<K> {
    /// Create a new empty pipe pool
    ///
    /// # Parameters
    /// - `kv`: Key-value store for pipe buffers
    /// - `notification_queue`: Shared notification queue for pipe data events
    /// - `dag`: DAG for checking producer node state
    #[must_use]
    pub fn new(kv: Arc<K>, notification_queue: NotificationQueueArc, dag: Arc<RwLock<Dag>>) -> Self {
        Self {
            writers: Mutex::new(Vec::new()),
            kv,
            notification_queue,
            dag,
        }
    }

    /// Get or create a reader for a pipe
    ///
    /// # Simplified Logic
    ///
    /// 1. If pipe exists in pool (Realized/Latent/KVCheck): handle accordingly
    /// 2. If no pipe entry: check actor state in DAG
    ///    - No actor → `Err(ActorNotFound)`
    ///    - NotStarted → create latent (if allow_latent), wait
    ///    - Terminated → check KV storage
    ///    - Running/Terminating → `Err(InvalidState)` (pipe should already exist)
    ///
    /// # Errors
    ///
    /// - `PipeClosed`: Producer closed latent pipe without creating it
    /// - `WouldBlock`: Pipe doesn't exist yet but allow_latent=false
    /// - `NotFound`: Terminated actor has no output in KV
    /// - `InvalidState`: Actor is Running/Terminating but pipe doesn't exist
    /// - `ActorNotFound`: Actor not in DAG
    /// - `Storage`: KV storage error
    pub async fn get_or_await_reader(
        &self,
        key: (Handle, StdHandle),
        allow_latent: bool,
        id_gen: &IdGen,
    ) -> Result<Reader, PipeError> {
        let (actor_handle, _std_handle) = key;

        loop {
            // Check state under lock and decide action
            enum Action {
                Wait(Arc<tokio::sync::Notify>),
                CheckKV,
            }

            let action = {
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
                            Action::Wait(Arc::clone(notify))
                        } else {
                            return Err(PipeError::WouldBlock);
                        }
                    }
                    Some(WriterState::KVCheck) => {
                        // Case 4: Terminated node - check KV outside lock
                        Action::CheckKV
                    }
                    None => {
                        // Case 5: No entry exists - check DAG state
                        let node_state = {
                            let dag = self.dag.read();
                            dag.get_node(actor_handle).map(|n| n.state)
                        };

                        match node_state {
                            None => {
                                // Actor not in DAG
                                return Err(PipeError::ActorNotFound);
                            }
                            Some(NodeState::NotStarted) => {
                                // Producer not started yet - create latent writer
                                if !allow_latent {
                                    return Err(PipeError::WouldBlock);
                                }
                                let notify = Arc::new(tokio::sync::Notify::new());
                                writers.push((key.0, key.1, WriterState::Latent {
                                    state: LatentState::Waiting,
                                    notify: Arc::clone(&notify),
                                }));
                                debug!(key = ?key, "created latent writer for NotStarted actor");
                                Action::Wait(notify)
                            }
                            Some(NodeState::Terminated) => {
                                // Producer terminated - mark for KV check
                                writers.push((key.0, key.1, WriterState::KVCheck));
                                debug!(key = ?key, "created KVCheck marker for terminated node");
                                Action::CheckKV
                            }
                            Some(state @ (NodeState::Running | NodeState::Terminating)) => {
                                // Error: Running/Terminating actor should have already touched pipe
                                return Err(PipeError::InvalidState(state));
                            }
                        }
                    }
                }
            }; // Lock released here

            // Execute action outside lock
            match action {
                Action::Wait(notify) => {
                    // Wait for notification, then loop back to recheck
                    notify.notified().await;
                    continue;
                }
                Action::CheckKV => {
                    // Check KV storage for terminated node
                    let path = format!("pipes/actor-{}-{:?}", actor_handle.id(), key.1);
                    let reader_handle = Handle::new(id_gen.get_next());
                    let writer_handle = actor_handle;

                    match create_reader_from_completed(
                        self.kv.as_ref(),
                        self.notification_queue.clone(),
                        reader_handle,
                        writer_handle,
                        &path,
                    )
                    .await
                    {
                        Ok(reader) => {
                            debug!(key = ?key, "resolved terminated node from KV");
                            return Ok(reader);
                        }
                        Err(e) => {
                            // No output in KV - pipe not found
                            debug!(key = ?key, "terminated node has no output in KV");
                            return Err(PipeError::Storage(e));
                        }
                    }
                }
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
    /// - For latent writers: marks as closed and notifies waiting readers
    /// - For KVCheck markers: removes them (no cleanup needed)
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
    /// Returns None if the pipe doesn't exist or is still latent/KVCheck.
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
