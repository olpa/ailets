//! `PipePool` - manages output pipes for actors
//!
//! Each (actor, fd) pair can have its own output pipe. Readers are created on-demand
//! when consuming actors need to read from dependencies.
//!
//! ## Latent Pipes
//!
//! Actors can start reading from dependencies before those dependencies have created their
//! output pipes. When a reader requests a non-existent pipe, a **latent writer** placeholder
//! is created. The reader waits on a `tokio::sync::watch` channel until the producer creates
//! the pipe or terminates.
//!
//! ## KV Storage for Terminated Actors
//!
//! Terminated actors store output in KV storage.
//! Value nodes are special: marked Terminated but never executed, with output only in KV.
//! Callers (like `MergeReader`) are responsible for checking KV when appropriate.

use std::sync::Arc;

use parking_lot::Mutex;
use tracing::{debug, warn};

use super::allocator::{create_writer, flush_and_close_writer};
use super::pipe_path;
use super::reader::Reader;
use super::writer::Writer;
use crate::idgen::{Handle, HandleKind, IdGen};
use crate::storage::KVBuffers;

/// Error type for pipe reader operations
#[derive(Debug)]
pub enum PipeError {
    /// Pipe was closed by producer (latent pipe marked Closed)
    PipeClosed,
    /// Would block waiting for pipe but `allow_latent=false`
    WouldBlock,
}

impl std::fmt::Display for PipeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PipeClosed => write!(f, "pipe closed by producer"),
            Self::WouldBlock => write!(f, "would block waiting for pipe"),
        }
    }
}

impl std::error::Error for PipeError {}

/// Inspection snapshot of a single pipe entry, returned by [`PipePool::inspect_entry`]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PipeEntryInspection {
    Realized { is_closed: bool, handle: Handle },
    Latent(LatentState),
}

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
        notify_tx: Arc<tokio::sync::watch::Sender<()>>,
    },
}

/// Pool of output pipes, indexed by (actor handle, fd) pair
///
/// Uses interior mutability via `Mutex` to allow shared access through `Arc<PipePool>`.
pub struct PipePool {
    /// Writers in various states (Realized or Latent)
    writers: Mutex<Vec<(Handle, isize, WriterState)>>,
    /// Key-value store for pipe buffers
    kv: Arc<dyn KVBuffers>,
}

impl PipePool {
    /// Create a new empty pipe pool
    ///
    /// # Parameters
    /// - `kv`: Key-value store for pipe buffers
    #[must_use]
    pub fn new(kv: Arc<dyn KVBuffers>) -> Self {
        Self {
            writers: Mutex::new(Vec::new()),
            kv,
        }
    }

    /// Get or await a pipe, then create a new independent reader for it
    ///
    /// This method **always creates a new Reader**, allowing multiple independent readers
    /// from the same pipe (fan-out). It handles pipes in various states:
    /// - **Realized**: Creates new reader immediately
    /// - **Latent (Waiting)**: Waits for pipe creation if `allow_latent=true`, then creates reader
    /// - **Latent (Closed)**: Returns `PipeClosed` error
    /// - **No entry**: Creates latent pipe if `allow_latent=true`, waits, then creates reader
    ///
    /// # Errors
    ///
    /// - `PipeClosed`: Producer closed latent pipe without creating it
    /// - `WouldBlock`: Pipe doesn't exist yet but `allow_latent=false`
    pub async fn get_or_await_new_reader(
        &self,
        key: (Handle, isize),
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
                        let reader_handle = id_gen.get_next_traced(
                            HandleKind::PipeReader,
                            key.0,
                            Some(writer.handle()),
                        );
                        return Ok(Reader::new(reader_handle, shared_data));
                    }
                    Some(WriterState::Latent {
                        state: LatentState::Closed,
                        ..
                    }) => {
                        // Case 2: Producer terminated without creating pipe
                        return Err(PipeError::PipeClosed);
                    }
                    Some(WriterState::Latent {
                        state: LatentState::Waiting,
                        notify_tx,
                    }) => {
                        // Case 3: Latent writer waiting for producer
                        if allow_latent {
                            Some(notify_tx.subscribe())
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
                        let (notify_tx, notify_rx) = tokio::sync::watch::channel(());
                        let notify_tx = Arc::new(notify_tx);
                        writers.push((
                            key.0,
                            key.1,
                            WriterState::Latent {
                                state: LatentState::Waiting,
                                notify_tx,
                            },
                        ));
                        debug!(key = ?key, "created latent writer");
                        Some(notify_rx)
                    }
                }
            }; // Lock released here

            // Wait for notification if needed, then loop back to recheck
            if let Some(mut rx) = wait_notify {
                // Err means the Sender was dropped; treat as wakeup.
                let _ = rx.changed().await;
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
        fd: isize,
        id_gen: &IdGen,
    ) -> Result<(Arc<Writer>, bool), crate::storage::KVError> {
        let key = (actor_handle, fd);

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
        let writer_handle = id_gen.get_next_traced(HandleKind::PipeWriter, actor_handle, None);
        let path = pipe_path(actor_handle, fd);

        // Create writer with buffer from KV storage
        let writer = create_writer(self.kv.as_ref(), writer_handle, &path).await?;

        // Replace state with Realized and notify waiters if needed
        let writer_arc = Arc::new(writer);
        let notify_tx = {
            let mut writers = self.writers.lock();

            // Remove old state and extract notify handle if it was Latent
            let notify_tx = if let Some(pos) = writers.iter().position(|(h, s, _)| (*h, *s) == key)
            {
                let (_, _, old_state) = writers.remove(pos);
                match old_state {
                    WriterState::Latent { notify_tx, .. } => Some(notify_tx),
                    WriterState::Realized(_) => None,
                }
            } else {
                None
            };

            // Insert Realized state
            writers.push((key.0, key.1, WriterState::Realized(Arc::clone(&writer_arc))));
            debug!(key = ?key, "created writer");

            notify_tx
        }; // Lock released here

        // Notify waiters outside lock
        if let Some(tx) = notify_tx {
            if tx.send(()).is_err() {
                warn!(key = ?key, "pool: notify after creating writer failed, no receivers");
            }
            debug!(key = ?key, "notified waiters after creating writer");
        }

        Ok((writer_arc, true))
    }

    /// Close all writers (realized and latent) for an actor, flushing buffers to storage.
    ///
    /// `exit_code`: 0 = clean termination, non-zero = POSIX errno.
    /// For realized writers with a non-zero exit code, sets the error before closing
    /// so readers see the error after consuming all written data.
    ///
    /// # Errors
    ///
    /// Returns `Err` if flushing a realized writer's buffer to storage fails.
    pub async fn flush_close_actor_writers(
        &self,
        actor_handle: Handle,
        exit_code: i32,
    ) -> Result<(), String> {
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
                        WriterState::Latent {
                            state: latent_state,
                            notify_tx,
                        } if *latent_state == LatentState::Waiting => {
                            *latent_state = LatentState::Closed;
                            notifies.push(Arc::clone(notify_tx));
                            debug!(key = ?(*h, *s), "closed latent writer on actor shutdown");
                        }
                        WriterState::Latent { .. } => {}
                    }
                }
            }

            (writers_to_close, notifies)
        }; // Lock released here

        // Flush and close writers outside lock
        let mut error_count = 0;
        for (h, s, writer) in writers_to_close {
            if exit_code != 0 {
                writer.set_error(exit_code);
            }
            match flush_and_close_writer(&*self.kv, &writer, "actor shutdown").await {
                Ok(()) => {
                    debug!(key = ?(h, s), exit_code, "flushed and closed realized writer on actor shutdown");
                }
                Err(errno) => {
                    warn!(key = ?(h, s), exit_code, errno, "flush/close failed on actor shutdown");
                    error_count += 1;
                }
            }
        }

        // Notify latent waiters outside lock
        for tx in notifies {
            if tx.send(()).is_err() {
                warn!(actor = ?actor_handle, "pool: notify latent waiter on actor shutdown failed, no receivers");
            }
        }

        if error_count == 0 {
            Ok(())
        } else {
            Err(format!("flush/close failed for {error_count} writers"))
        }
    }

    /// Build a future that copies the pipe at `key` to `writer`.
    ///
    /// The returned future can be spawned into a `JoinSet` (for tracked draining on
    /// shutdown) or directly onto a runtime handle via `spawn_reader_to`.
    pub fn reader_future<W>(
        self: &Arc<Self>,
        idgen: &Arc<IdGen>,
        key: (Handle, isize),
        writer: W,
    ) -> impl std::future::Future<Output = ()> + Send + 'static
    where
        W: std::io::Write + Send + 'static,
    {
        let pool = Arc::clone(self);
        let idgen = Arc::clone(idgen);
        let (node, fd) = key;
        async move {
            match pool.get_or_await_new_reader(key, true, &idgen).await {
                Ok(reader) => {
                    if let Err(e) = super::reader::drain_to_writer(
                        reader,
                        writer,
                        super::reader::FlushMode::AfterEachWrite,
                    )
                    .await
                    {
                        warn!(node = ?node, fd = fd, error = %e, "reader: copy failed");
                    }
                }
                Err(PipeError::PipeClosed) => {}
                Err(e) => warn!(node = ?node, fd = fd, error = %e, "reader: attach failed"),
            }
        }
    }

    /// Spawn a task that copies the pipe at `key` to `writer`.
    ///
    /// For callers that need shutdown-safe draining, use `reader_future` and spawn
    /// into a tracked `JoinSet` instead.
    pub fn spawn_reader_to<W>(
        self: &Arc<Self>,
        async_runtime: &tokio::runtime::Handle,
        idgen: &Arc<IdGen>,
        key: (Handle, isize),
        writer: W,
    ) -> tokio::task::JoinHandle<()>
    where
        W: std::io::Write + Send + 'static,
    {
        async_runtime.spawn(self.reader_future(idgen, key, writer))
    }

    /// Close all remaining latent writers still in Waiting state.
    ///
    /// Called at shutdown to unblock reader tasks waiting on pipes that were
    /// never realized (e.g. because their producer was killed before opening stdout).
    ///
    /// Returns the number of leftover entries that were closed.
    pub fn close_all_leftover_writers(&self) -> usize {
        let notifies = {
            let mut writers = self.writers.lock();
            let mut notifies = Vec::new();
            for (h, s, state) in &mut *writers {
                if let WriterState::Latent {
                    state: latent_state,
                    notify_tx,
                } = state
                {
                    if *latent_state == LatentState::Waiting {
                        *latent_state = LatentState::Closed;
                        notifies.push(Arc::clone(notify_tx));
                        debug!(key = ?(*h, *s), "closed leftover latent writer on shutdown");
                    }
                }
            }
            notifies
        };

        let count = notifies.len();
        for tx in notifies {
            if tx.send(()).is_err() {
                warn!("pool: notify leftover latent waiter on shutdown failed, no receivers");
            }
        }
        count
    }

    /// Get a writer by key (only if already realized)
    ///
    /// Returns an Arc to the writer if it exists and has been realized.
    /// Returns None if the pipe doesn't exist or is still latent.
    ///
    /// The returned Arc shares ownership of the writer, preventing premature closure.
    pub fn get_already_realized_writer(&self, key: (Handle, isize)) -> Option<Arc<Writer>> {
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

    fn inspect_state(state: &WriterState) -> PipeEntryInspection {
        match state {
            WriterState::Realized(w) => PipeEntryInspection::Realized {
                is_closed: w.is_closed(),
                handle: w.handle(),
            },
            WriterState::Latent { state, .. } => PipeEntryInspection::Latent(*state),
        }
    }

    /// Return all (actor, fd, inspection) triples in the pool.
    pub fn inspect_entries(&self) -> Vec<(Handle, isize, PipeEntryInspection)> {
        self.writers
            .lock()
            .iter()
            .map(|(actor, fd, state)| (*actor, *fd, Self::inspect_state(state)))
            .collect()
    }

    /// Return an inspection snapshot of the pipe entry for `key`, or `None` if no entry exists.
    pub fn inspect_entry(&self, key: (Handle, isize)) -> Option<PipeEntryInspection> {
        self.writers
            .lock()
            .iter()
            .find(|(h, s, _)| (*h, *s) == key)
            .map(|(_, _, state)| Self::inspect_state(state))
    }
}

impl Drop for PipePool {
    fn drop(&mut self) {
        let unclosed_count = self
            .writers
            .get_mut()
            .iter()
            .filter(|(_, _, s)| {
                let is_closed = match s {
                    WriterState::Realized(w) => w.is_closed(),
                    WriterState::Latent { state, .. } => *state == LatentState::Closed,
                };
                !is_closed
            })
            .count();
        if unclosed_count > 0 {
            warn!(
                unclosed_count,
                "pool dropped with unclosed writers; call flush_close_actor_writers and close_all_leftover_writers before dropping"
            );
        }
    }
}
