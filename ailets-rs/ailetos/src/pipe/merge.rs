//! `MergeReader` - sequential reader over multiple dependency inputs
//!
//! `MergeReader` provides a unified stdin stream for actors by reading from
//! dependencies sequentially. When one dependency's reader reaches EOF,
//! it transparently switches to the next dependency.
//!
//! Uses `OwnedDependencyIterator` from the DAG to lazily iterate through
//! dependencies, resolving aliases automatically.

use std::sync::Arc;

use tracing::{trace, warn};

use super::pool::PipePool;
use super::reader::Reader;
use crate::dag::{NodeState, OwnedDependencyIterator};
use crate::idgen::IdGen;
use crate::storage::KVBuffers;

/// A reader that sequentially reads from multiple dependency inputs.
///
/// `MergeReader` uses an `OwnedDependencyIterator` to lazily traverse dependencies.
/// When the current reader returns EOF, it automatically advances to the next
/// dependency and continues reading. Alias nodes are resolved by the iterator.
///
/// ## Data Source Resolution (Pipe-first, KV-fallback)
///
/// For each dependency, MergeReader resolves the data source:
/// 1. Try to get reader from PipePool (live pipe data)
/// 2. If pipe doesn't exist AND producer is Terminated, check KV storage as fallback
/// 3. This handles both running actors (pipes) and completed actors (KV)
///
/// # Usage
///
/// `MergeReader` is always used for stdin, regardless of dependency count:
/// - 0 dependencies: immediately returns EOF
/// - 1 dependency: reads from that single dependency
/// - N dependencies: reads from each in sequence
pub struct MergeReader<K: KVBuffers> {
    /// The currently active reader (None before first read or between deps)
    current_reader: Option<Reader>,
    /// Iterator over dependency handles (resolves aliases)
    dep_iterator: OwnedDependencyIterator,
    /// Pool of pipes to get `ReaderSharedData` from dependency handles
    pipe_pool: Arc<PipePool<K>>,
    /// Key-value storage for completed actor output
    kv: Arc<K>,
    /// ID generator for creating reader handles
    id_gen: Arc<IdGen>,
}

impl<K: KVBuffers> MergeReader<K> {
    /// Create a new `MergeReader` with a dependency iterator, pipe pool, and KV storage.
    ///
    /// # Arguments
    ///
    /// * `dep_iterator` - Iterator over dependency handles (from DAG)
    /// * `pipe_pool` - Pool of pipes for converting handles to readers
    /// * `kv` - Key-value storage for completed actor output
    /// * `id_gen` - ID generator for creating reader handles
    #[must_use]
    pub fn new(
        dep_iterator: OwnedDependencyIterator,
        pipe_pool: Arc<PipePool<K>>,
        kv: Arc<K>,
        id_gen: Arc<IdGen>,
    ) -> Self {
        Self {
            current_reader: None,
            dep_iterator,
            pipe_pool,
            kv,
            id_gen,
        }
    }

    /// Create a reader for the next dependency from the iterator.
    ///
    /// Implements pipe-first, KV-fallback strategy:
    /// 1. Check DAG state to determine if latent pipe is appropriate
    /// 2. Try to get reader from PipePool (live pipe data)
    /// 3. If WouldBlock AND producer is Terminated, try KV storage as fallback
    ///
    /// Returns `Ok(None)` if there are no more dependencies.
    /// Returns `Err` if there was an error getting the reader.
    async fn create_next_reader(&mut self) -> Result<Option<Reader>, crate::pipe::pool::PipeError> {
        let Some((dep_handle, dep_state)) = self.dep_iterator.next() else {
            return Ok(None);
        };

        // Don't create latent pipes for already-terminated actors
        // (close_actor_writers has already run, so latent would wait forever)
        let allow_latent = dep_state != NodeState::Terminated;

        // Try pipe pool first (dependencies always output to stdout)
        match self.pipe_pool
            .get_or_await_reader(
                (dep_handle, actor_runtime::StdHandle::Stdout),
                allow_latent,
                &self.id_gen,
            )
            .await
        {
            Ok(reader) => Ok(Some(reader)),
            Err(crate::pipe::pool::PipeError::WouldBlock) if dep_state == NodeState::Terminated => {
                // Pipe doesn't exist and producer is terminated - try KV as fallback
                trace!(dep = ?dep_handle, "pipe doesn't exist for terminated actor, checking KV");
                self.get_reader_from_kv(dep_handle).await.map(Some)
            }
            Err(e) => Err(e),
        }
    }

    /// Get a reader from KV storage for a completed actor's output
    async fn get_reader_from_kv(
        &self,
        actor_handle: crate::idgen::Handle,
    ) -> Result<Reader, crate::pipe::pool::PipeError> {
        use super::allocator::create_reader_from_completed;

        let path = format!("pipes/actor-{}-{:?}", actor_handle.id(), actor_runtime::StdHandle::Stdout);
        let reader_handle = crate::idgen::Handle::new(self.id_gen.get_next());

        create_reader_from_completed(
            self.kv.as_ref(),
            reader_handle,
            &path,
        )
        .await
        .map_err(crate::pipe::pool::PipeError::Storage)
    }

    /// Read data from the merged dependency stream.
    ///
    /// Reads from the current dependency. When EOF is reached, automatically
    /// advances to the next dependency and continues reading.
    ///
    /// # Returns
    ///
    /// - Positive value: number of bytes read
    /// - 0: EOF (all dependencies exhausted)
    /// - -1: error (check underlying reader's error)
    ///
    pub async fn read(&mut self, buf: &mut [u8]) -> isize {
        loop {
            // Ensure we have a reader for the current dependency
            if self.current_reader.is_none() {
                match self.create_next_reader().await {
                    Ok(Some(reader)) => {
                        self.current_reader = Some(reader);
                    }
                    Ok(None) => {
                        // No more dependencies available
                        return 0;
                    }
                    Err(e) => {
                        // Error getting reader from dependency
                        warn!(error = ?e, "MergeReader: failed to get reader for dependency");
                        return -1;
                    }
                }
            }

            // Read from current reader
            if let Some(reader) = self.current_reader.as_mut() {
                let n = reader.read(buf).await;

                match n.cmp(&0) {
                    std::cmp::Ordering::Equal => {
                        // EOF from current reader, move to next dependency
                        self.current_reader = None;
                        // Loop continues to try the next dependency
                    }
                    std::cmp::Ordering::Greater | std::cmp::Ordering::Less => {
                        // Successfully read data (positive) or error (negative)
                        return n;
                    }
                }
            } else {
                // Should not happen: we checked is_none() above
                warn!("MergeReader: current_reader is None unexpectedly");
                return 0;
            }
        }
    }

    /// Close the merge reader.
    pub fn close(&mut self) {
        if let Some(ref mut reader) = self.current_reader {
            reader.close();
        }
        self.current_reader = None;
    }

    /// Check if the merge reader is closed (no active reader and iterator exhausted).
    ///
    /// Note: This is a heuristic check. The iterator may still have items,
    /// but we can't peek without consuming. Returns true only when we know
    /// for certain that we're done (no current reader and we've hit EOF).
    #[must_use]
    pub fn is_closed(&self) -> bool {
        // We're closed if there's no current reader.
        // The iterator state is opaque, so we can't check if it's exhausted
        // without consuming it. The reader being None after read() returns 0
        // indicates we've exhausted all dependencies.
        self.current_reader.is_none()
    }
}

impl<K: KVBuffers> std::fmt::Debug for MergeReader<K> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MergeReader(current_reader={:?})", self.current_reader)
    }
}

impl<K: KVBuffers> Drop for MergeReader<K> {
    fn drop(&mut self) {
        trace!("MergeReader: destroying (drop)");
        if self.current_reader.is_some() {
            self.close();
        }
    }
}
