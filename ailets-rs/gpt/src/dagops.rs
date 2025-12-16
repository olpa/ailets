//! DAG Operations Module
//!
//! This module provides abstractions for integrating function calls with
//! Directed Acyclic Graph (DAG) operations. It enables mocking.

pub trait DagOpsTrait {
    /// Writer type for this DAG operations implementation
    type Writer: embedded_io::Write<Error = embedded_io::ErrorKind>;

    /// Creates a value node in the DAG.
    ///
    /// # Errors
    /// Returns an error if the node creation fails.
    fn value_node(&mut self, value: &[u8], explain: &str) -> Result<i32, String>;

    /// Creates an alias for a node handle.
    ///
    /// # Errors
    /// Returns an error if the alias creation fails.
    fn alias(&mut self, alias: &str, node_handle: i32) -> Result<i32, String>;

    /// Detaches a node from its alias.
    ///
    /// # Errors
    /// Returns an error if the detachment fails.
    fn detach_from_alias(&mut self, alias: &str) -> Result<(), String>;

    /// Instantiates a workflow with dependencies.
    ///
    /// # Errors
    /// Returns an error if the instantiation fails.
    fn instantiate_with_deps(
        &mut self,
        workflow_name: &str,
        deps: impl Iterator<Item = (String, i32)>,
    ) -> Result<i32, String>;

    /// Opens a write pipe.
    ///
    /// # Errors
    /// Returns an error if the pipe creation fails.
    fn open_write_pipe(&mut self, explain: Option<&str>) -> Result<i32, String>;

    /// Creates an alias for a file descriptor.
    ///
    /// # Errors
    /// Returns an error if the alias creation fails.
    fn alias_fd(&mut self, alias: &str, fd: i32) -> Result<i32, String>;

    /// Opens a writer to a pipe for testing/mocking.
    ///
    /// # Errors
    /// Returns an error if the writer creation fails.
    fn open_writer_to_pipe(&mut self, fd: i32) -> Result<Self::Writer, String>;
}

pub struct DagOps<'a> {
    runtime: &'a actor_runtime::FfiActorRuntime,
}

impl<'a> DagOps<'a> {
    /// Creates a new DAG operations instance
    #[must_use]
    pub fn new(runtime: &'a actor_runtime::FfiActorRuntime) -> Self {
        Self { runtime }
    }
}

impl<'a> DagOpsTrait for DagOps<'a> {
    type Writer = actor_io::AWriter<'a>;

    fn value_node(&mut self, value: &[u8], explain: &str) -> Result<i32, String> {
        actor_runtime::value_node(value, explain)
    }

    fn alias(&mut self, alias: &str, node_handle: i32) -> Result<i32, String> {
        actor_runtime::alias(alias, node_handle)
    }

    fn detach_from_alias(&mut self, alias: &str) -> Result<(), String> {
        actor_runtime::detach_from_alias(alias)
    }

    fn instantiate_with_deps(
        &mut self,
        workflow_name: &str,
        deps: impl Iterator<Item = (String, i32)>,
    ) -> Result<i32, String> {
        actor_runtime::instantiate_with_deps(workflow_name, deps)
    }

    fn open_write_pipe(&mut self, explain: Option<&str>) -> Result<i32, String> {
        actor_runtime::open_write_pipe(explain)
    }

    fn alias_fd(&mut self, alias: &str, fd: i32) -> Result<i32, String> {
        actor_runtime::alias_fd(alias, fd)
    }

    fn open_writer_to_pipe(&mut self, fd: i32) -> Result<Self::Writer, String> {
        actor_io::AWriter::new_from_fd(self.runtime, fd)
            .map_err(|e| actor_io::error_kind_to_str(e).to_string())
    }
}
