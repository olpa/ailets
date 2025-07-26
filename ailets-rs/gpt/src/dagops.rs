//! DAG Operations Module
//!
//! This module provides abstractions for integrating function calls with 
//! Directed Acyclic Graph (DAG) operations. It enables function calls to be
//! processed as part of a larger workflow system with dependency tracking,
//! pipe management, and alias creation.

use crate::funcalls_write::FunCallsWrite;
use actor_io::AWriter;
use std::collections::HashMap;
use std::io::Write;

// =============================================================================
// Utilities
// =============================================================================

/// Escapes a string for safe inclusion in JSON by escaping backslashes and quotes
/// 
/// # Arguments
/// * `s` - The string to escape
/// 
/// # Returns
/// A new string with `\` escaped as `\\` and `"` escaped as `\"`
fn escape_json_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

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
        // Create a DagOpsWrite and delegate to it
        let mut dag_writer = DagOpsWrite::new(&mut self.dagops);

        // The actual processing logic would be implemented here
        // For now, just call end() to finalize any processing
        writer.end()?;
        dag_writer.end()?;

        Ok(())
    }
}

// =============================================================================
// DAG-Integrated Function Call Writer
// =============================================================================

/// Function call writer that integrates with DAG operations
///
/// This writer implements the `FunCallsWrite` trait while simultaneously
/// creating DAG nodes, workflows, and pipes for each function call. It
/// enables function calls to participate in the larger workflow system.
pub struct DagOpsWrite<'a, T: DagOpsTrait> {
    /// Reference to the DAG operations implementation
    dagops: &'a mut T,
    /// Whether we've detached from the chat messages alias
    detached: bool,
    /// Writer for the current tool's input data
    tool_input_writer: Option<Box<dyn Write>>,
    /// Writer for the current tool's specification (JSON)
    tool_spec_writer: Option<Box<dyn Write>>,
}

impl<'a, T: DagOpsTrait> DagOpsWrite<'a, T> {
    /// Creates a new DAG-integrated function call writer
    /// 
    /// # Arguments
    /// * `dagops` - Mutable reference to the DAG operations implementation
    /// 
    /// # Returns
    /// A new writer that will create DAG operations for each function call
    #[must_use]
    pub fn new(dagops: &'a mut T) -> Self {
        Self {
            dagops,
            detached: false,
            tool_input_writer: None,
            tool_spec_writer: None,
        }
    }
}

impl<'a, T: DagOpsTrait> FunCallsWrite for DagOpsWrite<'a, T> {
    fn new_item(&mut self, id: &str, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        if !self.detached {
            self.setup_loop_iteration_in_dag()?;
            self.detached = true;
        }

        // Create the tool input pipe and writer
        let explain = format!("tool input - {}", name);
        let tool_input_fd = self.dagops.open_write_pipe(Some(&explain))?;
        #[allow(clippy::cast_sign_loss)]
        let tool_input_fd_u32 = tool_input_fd as u32;
        let tool_input_writer = self.dagops.open_writer_to_pipe(tool_input_fd)?;

        self.tool_input_writer = Some(tool_input_writer);

        // Create the tool spec pipe and writer
        let explain = format!("tool call spec - {}", name);
        let tool_spec_handle_fd = self.dagops.open_write_pipe(Some(&explain))?;
        #[allow(clippy::cast_sign_loss)]
        let tool_spec_handle_fd_u32 = tool_spec_handle_fd as u32;
        let mut tool_spec_writer = self.dagops.open_writer_to_pipe(tool_spec_handle_fd)?;

        // Write the beginning of the tool spec JSON structure up to the arguments value
        let json_start = format!(
            r#"[{{"type":"function","id":"{}","name":"{}"}},{{"arguments":""#,
            escape_json_string(id),
            escape_json_string(name)
        );
        tool_spec_writer
            .write_all(json_start.as_bytes())
            .map_err(|e| e.to_string())?;

        self.tool_spec_writer = Some(tool_spec_writer);

        // Create aliases for the pipes
        self.dagops.alias_fd(".tool_input", tool_input_fd_u32)?;
        self.dagops
            .alias_fd(".llm_tool_spec", tool_spec_handle_fd_u32)?;

        // Run the tool
        let tool_handle = self.dagops.instantiate_with_deps(
            &format!(".tool.{}", name),
            HashMap::from([(".tool_input".to_string(), tool_input_fd_u32)]).into_iter(),
        )?;

        // Convert tool output to messages
        let msg_handle = self.dagops.instantiate_with_deps(
            ".toolcall_to_messages",
            HashMap::from([
                (".llm_tool_spec".to_string(), tool_spec_handle_fd_u32),
                (".tool_output".to_string(), tool_handle),
            ])
            .into_iter(),
        )?;

        // Create the chat messages alias immediately
        self.dagops.alias(".chat_messages", msg_handle)?;

        Ok(())
    }

    fn arguments_chunk(&mut self, args: &str) -> Result<(), Box<dyn std::error::Error>> {
        // Write to tool input writer
        if let Some(ref mut writer) = self.tool_input_writer {
            writer
                .write_all(args.as_bytes())
                .map_err(|e| e.to_string())?;
        }

        // Write to tool spec writer (as part of the arguments JSON string value)
        // Need to escape JSON special characters when writing to the tool spec
        if let Some(ref mut writer) = self.tool_spec_writer {
            writer
                .write_all(escape_json_string(args).as_bytes())
                .map_err(|e| e.to_string())?;
        }

        Ok(())
    }

    fn end_item(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Drop/finalize the tool input writer
        drop(self.tool_input_writer.take());

        // Finalize the tool spec JSON by writing the closing structure
        if let Some(mut writer) = self.tool_spec_writer.take() {
            // Close the arguments string and the JSON array
            let json_end = r#""}]"#;
            writer
                .write_all(json_end.as_bytes())
                .map_err(|e| e.to_string())?;
            drop(writer);
        }

        Ok(())
    }

    fn end(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Rerun model if we processed any tool calls (indicated by detached flag)
        if self.detached {
            let rerun_handle = self.dagops.instantiate_with_deps(
                ".gpt",
                HashMap::from([
                    (".chat_messages.media".to_string(), 0),
                    (".chat_messages.toolspecs".to_string(), 0),
                ])
                .into_iter(),
            )?;
            self.dagops.alias(".output_messages", rerun_handle)?;
        }

        Ok(())
    }
}

impl<'a, T: DagOpsTrait> DagOpsWrite<'a, T> {
    // =========================================================================
    // Private Helper Methods
    // =========================================================================

    /// Sets up the DAG for a new iteration by detaching from previous workflows
    ///
    /// This method ensures that new function calls don't interfere with
    /// previous model workflows by detaching from the chat messages alias.
    /// This prevents confusing dependency relationships in the DAG.
    ///
    /// # Errors
    /// Returns error if the detach operation fails
    fn setup_loop_iteration_in_dag(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Detach from previous chat messages to avoid dependency confusion
        // This prevents the old "user prompt to messages" workflow from
        // appearing to depend on new chat messages we're about to create
        self.dagops.detach_from_alias(".chat_messages")?;
        Ok(())
    }
}
