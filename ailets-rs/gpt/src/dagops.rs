//! DAG Operations Module
//!
//! This module provides abstractions for integrating function calls with
//! Directed Acyclic Graph (DAG) operations. It enables mocking.

use actor_io::AWriter;
use std::io::Write;

pub trait DagOpsTrait {
    fn value_node(&mut self, value: &[u8], explain: &str) -> Result<i32, String>;
    fn alias(&mut self, alias: &str, node_handle: i32) -> Result<i32, String>;
    fn detach_from_alias(&mut self, alias: &str) -> Result<(), String>;
    fn instantiate_with_deps(
        &mut self,
        workflow_name: &str,
        deps: impl Iterator<Item = (String, i32)>,
    ) -> Result<i32, String>;
    fn open_write_pipe(&mut self, explain: Option<&str>) -> Result<i32, String>;
    fn alias_fd(&mut self, alias: &str, fd: i32) -> Result<i32, String>;
    fn open_writer_to_pipe(&mut self, fd: i32) -> Result<Box<dyn Write>, String>;
}

pub struct DagOps;

impl DagOps {
    /// Creates a new DAG operations instance
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

    fn open_writer_to_pipe(&mut self, fd: i32) -> Result<Box<dyn Write>, String> {
        let writer = AWriter::new_from_fd(fd).map_err(|e| e.to_string())?;
        Ok(Box::new(writer))
    }
}
