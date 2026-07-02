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
use crate::errno::{EBADF, EIO};
use crate::idgen::{HandleKind, IdGen};
use crate::storage::KVBuffers;

/// A reader that sequentially reads from multiple dependency inputs.
///
/// `MergeReader` uses an `OwnedDependencyIterator` to lazily traverse dependencies.
/// When the current reader returns EOF, it automatically advances to the next
/// dependency and continues reading. Alias nodes are resolved by the iterator.
///
/// ## Data Source Resolution (Pipe-first, KV-fallback)
///
/// For each dependency, `MergeReader` resolves the data source:
/// 1. Try to get reader from `PipePool` (live pipe data)
/// 2. If pipe doesn't exist AND producer is Terminated, check KV storage as fallback
/// 3. This handles both running actors (pipes) and completed actors (KV)
///
/// # Usage
///
/// `MergeReader` is always used for stdin, regardless of dependency count:
/// - 0 dependencies: immediately returns EOF
/// - 1 dependency: reads from that single dependency
/// - N dependencies: reads from each in sequence
pub struct MergeReader {
    /// The currently active reader (None before first read or between deps)
    current_reader: Option<Reader>,
    /// Iterator over dependency handles (resolves aliases)
    dep_iterator: OwnedDependencyIterator,
    /// Pool of pipes to get `ReaderSharedData` from dependency handles
    pipe_pool: Arc<PipePool>,
    /// Key-value storage for completed actor output
    kv: Arc<dyn KVBuffers>,
    /// ID generator for creating reader handles
    id_gen: Arc<IdGen>,
    /// Whether this merge reader has been closed
    own_closed: bool,
}

impl MergeReader {
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
        pipe_pool: Arc<PipePool>,
        kv: Arc<dyn KVBuffers>,
        id_gen: Arc<IdGen>,
    ) -> Self {
        Self {
            current_reader: None,
            dep_iterator,
            pipe_pool,
            kv,
            id_gen,
            own_closed: false,
        }
    }

    /// Create a reader for the next dependency from the iterator.
    ///
    /// Implements pipe-first, KV-fallback strategy:
    /// 1. Check DAG state to determine if latent pipe is appropriate
    /// 2. Try to get reader from `PipePool` (live pipe data)
    /// 3. If `WouldBlock` AND producer is Terminated, try KV storage as fallback
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

        // Try pipe pool first (dependencies always output to stdout, fd=1)
        match self
            .pipe_pool
            .get_or_await_new_reader(
                (dep_handle, actor_runtime::StdHandle::Stdout as isize),
                allow_latent,
                &self.id_gen,
            )
            .await
        {
            Ok(reader) => Ok(Some(reader)),
            Err(crate::pipe::pool::PipeError::WouldBlock) if dep_state == NodeState::Terminated => {
                // Pipe doesn't exist and producer is terminated - try KV as fallback
                trace!(dep = ?dep_handle, "pipe doesn't exist for terminated actor, checking KV");
                match self.get_reader_from_kv(dep_handle).await {
                    Ok(reader) => Ok(Some(reader)),
                    Err(kv_error) => {
                        warn!(error = ?kv_error, dep = ?dep_handle, "failed to get reader from KV storage");
                        Err(crate::pipe::pool::PipeError::WouldBlock)
                    }
                }
            }
            Err(e) => Err(e),
        }
    }

    /// Get a reader from KV storage for a completed actor's output
    async fn get_reader_from_kv(
        &self,
        actor_handle: crate::idgen::Handle,
    ) -> Result<Reader, crate::storage::KVError> {
        use super::allocator::create_reader_from_completed;
        use super::pipe_path;

        // Dependencies always output to stdout (fd=1)
        let path = pipe_path(actor_handle, actor_runtime::StdHandle::Stdout as isize);
        let reader_handle = self
            .id_gen
            .get_next_traced(HandleKind::PipeReader, actor_handle, None);

        create_reader_from_completed(self.kv.as_ref(), reader_handle, &path).await
    }

    /// Read data from the merged dependency stream.
    ///
    /// Reads from the current dependency. When EOF is reached, automatically
    /// advances to the next dependency and continues reading.
    ///
    /// # Returns
    ///
    /// - `Ok(n)` where `n > 0`: number of bytes read
    /// - `Ok(0)`: EOF (all dependencies exhausted)
    ///
    /// # Errors
    /// Returns `EIO` if failed to get reader for dependency.
    pub async fn read(&mut self, buf: &mut [u8]) -> Result<usize, i32> {
        loop {
            // Ensure we have a reader for the current dependency
            if self.current_reader.is_none() {
                match self.create_next_reader().await {
                    Ok(Some(reader)) => {
                        self.current_reader = Some(reader);
                    }
                    Ok(None) => {
                        // No more dependencies available
                        return Ok(0);
                    }
                    Err(e) => {
                        warn!(error = ?e, "MergeReader: failed to get reader for dependency");
                        return Err(EIO);
                    }
                }
            }

            // Read from current reader
            if let Some(reader) = self.current_reader.as_mut() {
                match reader.read(buf).await {
                    Ok(0) => {
                        // EOF from current reader, move to next dependency
                        self.current_reader = None;
                        // Loop continues to try the next dependency
                    }
                    Ok(n) => {
                        return Ok(n);
                    }
                    Err(e) => {
                        return Err(e);
                    }
                }
            } else {
                // Should not happen: we checked is_none() above
                warn!("MergeReader: current_reader is None unexpectedly");
                return Ok(0);
            }
        }
    }

    /// Close the merge reader.
    ///
    /// # Errors
    /// Returns `EBADF` if already closed, or propagates error from inner reader close.
    pub fn close(&mut self) -> Result<(), i32> {
        if self.own_closed {
            warn!("MergeReader::close() called on already closed reader");
            return Err(EBADF);
        }
        self.own_closed = true;
        let result = if let Some(ref mut reader) = self.current_reader {
            reader.close()
        } else {
            Ok(())
        };
        self.current_reader = None;
        result
    }
}

impl std::fmt::Debug for MergeReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MergeReader(current_reader={:?})", self.current_reader)
    }
}

impl Drop for MergeReader {
    fn drop(&mut self) {
        trace!("MergeReader: destroying (drop)");
        if !self.own_closed {
            if let Err(errno) = self.close() {
                warn!(errno, "MergeReader::drop: close failed");
            }
        }
    }
}
