//! MergeReader - sequential reader over multiple dependency inputs
//!
//! MergeReader provides a unified stdin stream for actors by reading from
//! dependencies sequentially. When one dependency's reader reaches EOF,
//! it transparently switches to the next dependency.
//!
//! This replaces the previous approach of spawning a background task to
//! merge inputs, making the merging lazy and inline during read operations.

use std::sync::Arc;

use crate::idgen::{Handle, IdGen};
use crate::pipe::{Reader, ReaderSharedData};

/// A reader that sequentially reads from multiple dependency inputs.
///
/// MergeReader maintains a current dependency index and wraps the underlying
/// reader. When the current reader returns EOF, it automatically advances to
/// the next dependency and continues reading.
///
/// # Usage
///
/// MergeReader is always used for stdin, regardless of dependency count:
/// - 0 dependencies: immediately returns EOF
/// - 1 dependency: reads from that single dependency
/// - N dependencies: reads from each in sequence
pub struct MergeReader {
    /// The currently active reader (None before first read or between deps)
    current_reader: Option<Reader>,
    /// Index of the current dependency being read
    dep_index: usize,
    /// Pre-fetched reader shared data for each known dependency
    deps_data: Vec<ReaderSharedData>,
    /// ID generator for creating reader handles
    id_gen: Arc<IdGen>,
    /// Callback to query for additional dependencies beyond initial set.
    /// Returns Some(ReaderSharedData) if a new dependency is available at the index,
    /// None if no more dependencies exist.
    /// Panics if the dependency exists but its pipe is not ready yet.
    query_more_deps: Option<Box<dyn Fn(usize) -> Option<ReaderSharedData> + Send>>,
}

impl MergeReader {
    /// Create a new MergeReader with pre-fetched dependency data.
    ///
    /// # Arguments
    ///
    /// * `deps_data` - Pre-fetched ReaderSharedData for each known dependency
    /// * `id_gen` - ID generator for creating reader handles
    #[must_use]
    pub(crate) fn new(deps_data: Vec<ReaderSharedData>, id_gen: Arc<IdGen>) -> Self {
        Self {
            current_reader: None,
            dep_index: 0,
            deps_data,
            id_gen,
            query_more_deps: None,
        }
    }

    /// Create a new MergeReader with a callback for querying additional dependencies.
    ///
    /// The callback is invoked when the initial deps_data is exhausted, allowing
    /// for dynamic DAG support where dependencies may appear after the actor starts.
    ///
    /// # Arguments
    ///
    /// * `deps_data` - Pre-fetched ReaderSharedData for initially known dependencies
    /// * `id_gen` - ID generator for creating reader handles
    /// * `query_more` - Callback to query for dependencies beyond the initial set
    #[must_use]
    #[allow(dead_code)] // Will be used when dynamic DAG support is implemented
    pub(crate) fn with_dynamic_deps(
        deps_data: Vec<ReaderSharedData>,
        id_gen: Arc<IdGen>,
        query_more: Box<dyn Fn(usize) -> Option<ReaderSharedData> + Send>,
    ) -> Self {
        Self {
            current_reader: None,
            dep_index: 0,
            deps_data,
            id_gen,
            query_more_deps: Some(query_more),
        }
    }

    /// Try to get ReaderSharedData for the given dependency index.
    ///
    /// First checks the pre-fetched deps_data, then falls back to the
    /// query_more_deps callback if available.
    fn get_dep_data(&self, index: usize) -> Option<ReaderSharedData> {
        if index < self.deps_data.len() {
            // Clone the shared data for this index
            Some(self.deps_data[index].clone())
        } else if let Some(ref query) = self.query_more_deps {
            // Query for additional dependencies
            query(index)
        } else {
            None
        }
    }

    /// Create a reader for the dependency at the given index.
    fn create_reader_for_index(&self, index: usize) -> Option<Reader> {
        self.get_dep_data(index).map(|shared_data| {
            let handle = Handle::new(self.id_gen.get_next());
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
                match self.create_reader_for_index(self.dep_index) {
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
                    self.dep_index += 1;
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

    /// Check if the merge reader is closed (no active reader and no more deps).
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.current_reader.is_none() && self.get_dep_data(self.dep_index).is_none()
    }
}

impl std::fmt::Debug for MergeReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "MergeReader(dep_index={}, known_deps={}, has_query={}, current_reader={:?})",
            self.dep_index,
            self.deps_data.len(),
            self.query_more_deps.is_some(),
            self.current_reader
        )
    }
}

impl Drop for MergeReader {
    fn drop(&mut self) {
        if self.current_reader.is_some() {
            self.close();
        }
    }
}
