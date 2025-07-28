//! DAG Operations Module
//!
//! This module provides abstractions for integrating function calls with
//! Directed Acyclic Graph (DAG) operations. It enables function calls to be
//! processed as part of a larger workflow system with dependency tracking,
//! pipe management, and alias creation.

use crate::fcw_trait::FunCallsWrite;
use actor_io::AWriter;
use std::io::Write;

// =============================================================================
// Core DAG Operations Trait
// =============================================================================

/// Trait defining the core DAG operations interface
///
/// This trait abstracts the low-level DAG operations required for workflow
/// management, including node creation, aliasing, pipe management, and
/// workflow instantiation.
pub trait DagOpsTrait {
    /// Creates a value node in the DAG with the given data and explanation
    ///
    /// # Arguments
    /// * `value` - The data to store in the node
    /// * `explain` - Human-readable explanation of the node's purpose
    ///
    /// # Returns
    /// Handle to the created node
    ///
    /// # Errors
    /// Returns error from the underlying host system
    fn value_node(&mut self, value: &[u8], explain: &str) -> Result<u32, String>;

    /// Creates an alias pointing to the specified node
    ///
    /// # Arguments
    /// * `alias` - The alias name
    /// * `node_handle` - Handle to the node to alias
    ///
    /// # Returns
    /// Handle to the alias
    ///
    /// # Errors
    /// Returns error from the underlying host system
    fn alias(&mut self, alias: &str, node_handle: u32) -> Result<u32, String>;

    /// Detaches an alias, breaking its connection to the underlying node
    ///
    /// # Arguments
    /// * `alias` - The alias name to detach
    ///
    /// # Errors
    /// Returns error from the underlying host system
    fn detach_from_alias(&mut self, alias: &str) -> Result<(), String>;

    /// Instantiates a workflow with the given dependencies
    ///
    /// # Arguments
    /// * `workflow_name` - Name of the workflow to instantiate
    /// * `deps` - Iterator of (dependency_name, node_handle) pairs
    ///
    /// # Returns
    /// Handle to the instantiated workflow
    ///
    /// # Errors
    /// Returns error from the underlying host system
    fn instantiate_with_deps(
        &mut self,
        workflow_name: &str,
        deps: impl Iterator<Item = (String, u32)>,
    ) -> Result<u32, String>;

    /// Opens a write pipe for streaming data
    ///
    /// # Arguments
    /// * `explain` - Optional explanation of the pipe's purpose
    ///
    /// # Returns
    /// File descriptor for the write pipe
    ///
    /// # Errors
    /// Returns error from the underlying host system
    fn open_write_pipe(&mut self, explain: Option<&str>) -> Result<i32, String>;

    /// Creates an alias for a file descriptor
    ///
    /// # Arguments
    /// * `alias` - The alias name
    /// * `fd` - The file descriptor to alias
    ///
    /// # Returns
    /// Handle to the alias
    ///
    /// # Errors
    /// Returns error from the underlying host system
    fn alias_fd(&mut self, alias: &str, fd: u32) -> Result<u32, String>;

    /// Opens a writer for the specified write pipe
    ///
    /// # Arguments
    /// * `fd` - File descriptor of the write pipe
    ///
    /// # Returns
    /// A writer that can be used to write data incrementally
    ///
    /// # Errors
    /// Returns error from the underlying host system or I/O operations
    fn open_writer_to_pipe(&mut self, fd: i32) -> Result<Box<dyn Write>, String>;
}

// =============================================================================
// Concrete DAG Operations Implementation
// =============================================================================

/// Concrete implementation of DAG operations using the actor runtime
///
/// This struct provides the actual implementation of DAG operations by
/// delegating to the actor runtime system.
pub struct DagOps;

impl DagOps {
    /// Creates a new DAG operations instance
    ///
    /// # Returns
    /// A new `DagOps` instance ready to perform DAG operations
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Default for DagOps {
    fn default() -> Self {
        Self::new()
    }
}

impl DagOpsTrait for DagOps {
    fn value_node(&mut self, value: &[u8], explain: &str) -> Result<u32, String> {
        actor_runtime::value_node(value, explain)
    }

    fn alias(&mut self, alias: &str, node_handle: u32) -> Result<u32, String> {
        actor_runtime::alias(alias, node_handle)
    }

    fn detach_from_alias(&mut self, alias: &str) -> Result<(), String> {
        actor_runtime::detach_from_alias(alias)
    }

    fn instantiate_with_deps(
        &mut self,
        workflow_name: &str,
        deps: impl Iterator<Item = (String, u32)>,
    ) -> Result<u32, String> {
        actor_runtime::instantiate_with_deps(workflow_name, deps)
    }

    fn open_write_pipe(&mut self, explain: Option<&str>) -> Result<i32, String> {
        #[allow(clippy::cast_possible_wrap)]
        let result = actor_runtime::open_write_pipe(explain).map(|handle| handle as i32);
        result
    }

    fn alias_fd(&mut self, alias: &str, fd: u32) -> Result<u32, String> {
        actor_runtime::alias_fd(alias, fd)
    }

    fn open_writer_to_pipe(&mut self, fd: i32) -> Result<Box<dyn Write>, String> {
        let writer = AWriter::new_from_fd(fd).map_err(|e| e.to_string())?;
        Ok(Box::new(writer))
    }
}

// =============================================================================
// Injection Abstraction
// =============================================================================

/// Abstraction layer for injecting DAG operations into function call processing
///
/// This trait provides one level of indirection to enable testing that function
/// calls are collected and processed correctly within the DAG system.
pub trait InjectDagOpsTrait {
    /// Process function calls using the `FunCallsWrite` trait implementation
    ///
    /// This method integrates function call processing with DAG operations,
    /// allowing function calls to participate in the larger workflow system.
    ///
    /// # Arguments
    /// * `writer` - The function call writer to process calls with
    ///
    /// # Errors
    /// Promotes errors from the underlying host system and I/O operations
    fn process_with_funcalls_write(
        &mut self,
        writer: &mut dyn FunCallsWrite,
    ) -> Result<(), Box<dyn std::error::Error>>;
}

/// Concrete implementation of the injection abstraction
///
/// This struct wraps a `DagOpsTrait` implementation and provides the
/// injection functionality for integrating with function call processing.
pub struct InjectDagOps<T: DagOpsTrait> {
    dagops: T,
}

impl<T: DagOpsTrait> InjectDagOps<T> {
    /// Creates a new DAG operations injector
    ///
    /// # Arguments
    /// * `dagops` - The DAG operations implementation to wrap
    ///
    /// # Returns
    /// A new injector that can integrate DAG operations with function calls
    #[must_use]
    pub fn new(dagops: T) -> Self {
        Self { dagops }
    }
}

impl<T: DagOpsTrait> InjectDagOpsTrait for InjectDagOps<T> {
    fn process_with_funcalls_write(
        &mut self,
        writer: &mut dyn FunCallsWrite,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Create a FunCallsToDag and delegate to it
        let mut dag_writer = crate::fcw_dag::FunCallsToDag::new(&mut self.dagops);

        // The actual processing logic would be implemented here
        // For now, just call end() to finalize any processing
        writer.end()?;
        dag_writer.end()?;

        Ok(())
    }
}
