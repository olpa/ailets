//! MergeReader - sequential reader over multiple dependency inputs
//!
//! MergeReader provides a unified stdin stream for actors by reading from
//! dependencies sequentially. When one dependency's reader reaches EOF,
//! it transparently switches to the next dependency.
//!
//! Uses `OwnedDependencyIterator` from the DAG to lazily iterate through
//! dependencies, resolving aliases automatically.

use std::sync::Arc;

use crate::dag::OwnedDependencyIterator;
use crate::idgen::{Handle, IdGen};
use crate::io::KVBuffers;
use crate::pipe::Reader;
use crate::pipepool::PipePool;

/// A reader that sequentially reads from multiple dependency inputs.
///
/// MergeReader uses an `OwnedDependencyIterator` to lazily traverse dependencies.
/// When the current reader returns EOF, it automatically advances to the next
/// dependency and continues reading. Alias nodes are resolved by the iterator.
///
/// # Usage
///
/// MergeReader is always used for stdin, regardless of dependency count:
/// - 0 dependencies: immediately returns EOF
/// - 1 dependency: reads from that single dependency
/// - N dependencies: reads from each in sequence
pub struct MergeReader<K: KVBuffers> {
    /// The currently active reader (None before first read or between deps)
    current_reader: Option<Reader>,
    /// Iterator over dependency handles (resolves aliases)
    dep_iterator: OwnedDependencyIterator,
    /// Pool of pipes to get ReaderSharedData from dependency handles
    pipe_pool: Arc<PipePool<K>>,
    /// ID generator for creating reader handles
    id_gen: Arc<IdGen>,
}

impl<K: KVBuffers> MergeReader<K> {
    /// Create a new MergeReader with a dependency iterator and pipe pool.
    ///
    /// # Arguments
    ///
    /// * `dep_iterator` - Iterator over dependency handles (from DAG)
    /// * `pipe_pool` - Pool of pipes for converting handles to readers
    /// * `id_gen` - ID generator for creating reader handles
    #[must_use]
    pub fn new(
        dep_iterator: OwnedDependencyIterator,
        pipe_pool: Arc<PipePool<K>>,
        id_gen: Arc<IdGen>,
    ) -> Self {
        Self {
            current_reader: None,
            dep_iterator,
            pipe_pool,
            id_gen,
        }
    }

    /// Create a reader for the next dependency from the iterator.
    ///
    /// # Panics
    ///
    /// Panics if the dependency's pipe doesn't exist.
    /// TODO: async pipe creation, see #227
    fn create_next_reader(&mut self) -> Option<Reader> {
        self.dep_iterator.next().map(|dep_handle| {
            // Panic if pipe doesn't exist - see #227 for async pipe creation
            assert!(
                self.pipe_pool.has_pipe(dep_handle),
                "Dependency pipe for {:?} doesn't exist. TODO: async pipe creation, see #227",
                dep_handle
            );

            let handle = Handle::new(self.id_gen.get_next());
            let pipe = self.pipe_pool.get_pipe(dep_handle);
            let shared_data = pipe.writer().share_with_reader();
            Reader::new(handle, shared_data)
        })
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
    pub async fn read(&mut self, buf: &mut [u8]) -> isize {
        loop {
            // Ensure we have a reader for the current dependency
            if self.current_reader.is_none() {
                match self.create_next_reader() {
                    Some(reader) => {
                        self.current_reader = Some(reader);
                    }
                    None => {
                        // No more dependencies available
                        return 0;
                    }
                }
            }

            // Read from current reader
            // SAFETY: We just ensured current_reader is Some above
            #[allow(clippy::unwrap_used)]
            let reader = self.current_reader.as_mut().unwrap();
            let n = reader.read(buf).await;

            match n.cmp(&0) {
                std::cmp::Ordering::Greater => {
                    // Successfully read data
                    return n;
                }
                std::cmp::Ordering::Equal => {
                    // EOF from current reader, move to next dependency
                    self.current_reader = None;
                    // Loop continues to try the next dependency
                }
                std::cmp::Ordering::Less => {
                    // Error from reader
                    return n;
                }
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
        write!(
            f,
            "MergeReader(current_reader={:?})",
            self.current_reader
        )
    }
}

impl<K: KVBuffers> Drop for MergeReader<K> {
    fn drop(&mut self) {
        if self.current_reader.is_some() {
            self.close();
        }
    }
}
