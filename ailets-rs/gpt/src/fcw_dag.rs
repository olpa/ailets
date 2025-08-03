//! Function call writer that integrates with DAG operations
//!
//! This module provides a function call writer implementation that creates
//! DAG nodes, workflows, and pipes for each function call, enabling function
//! calls to participate in the larger workflow system.

use crate::dagops::DagOpsTrait;
use crate::fcw_trait::FunCallsWrite;
use std::collections::HashMap;
use std::io::Write;

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

/// Function call writer that integrates with DAG operations
///
/// This writer implements the `FunCallsWrite` trait while simultaneously
/// creating DAG nodes, workflows, and pipes for each function call. It
/// enables function calls to participate in the larger workflow system.
pub struct FunCallsToDag<'a, T: DagOpsTrait> {
    /// Reference to the DAG operations implementation
    dagops: &'a mut T,
    /// Whether we've detached from the chat messages alias
    detached: bool,
    /// Writer for the current tool's input data
    tool_input_writer: Option<Box<dyn Write>>,
    /// Writer for the current tool's specification (JSON)
    tool_spec_writer: Option<Box<dyn Write>>,
}

impl<'a, T: DagOpsTrait> FunCallsToDag<'a, T> {
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

impl<'a, T: DagOpsTrait> FunCallsWrite for FunCallsToDag<'a, T> {
    fn new_item(&mut self, id: &str, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        if !self.detached {
            self.setup_loop_iteration_in_dag()?;
            self.detached = true;
        }

        // Create the tool input pipe and writer
        let explain = format!("tool input - {name}");
        let tool_input_fd = self.dagops.open_write_pipe(Some(&explain))?;
        let tool_input_writer = self.dagops.open_writer_to_pipe(tool_input_fd)?;

        self.tool_input_writer = Some(tool_input_writer);

        // Create the tool spec pipe and writer
        let explain = format!("tool call spec - {name}");
        let tool_spec_handle_fd = self.dagops.open_write_pipe(Some(&explain))?;
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
        self.dagops.alias_fd(".tool_input", tool_input_fd)?;
        self.dagops
            .alias_fd(".llm_tool_spec", tool_spec_handle_fd)?;

        // Run the tool
        let tool_handle = self.dagops.instantiate_with_deps(
            &format!(".tool.{name}"),
            HashMap::from([(".tool_input".to_string(), tool_input_fd)]).into_iter(),
        )?;

        // Convert tool output to messages
        let msg_handle = self.dagops.instantiate_with_deps(
            ".toolcall_to_messages",
            HashMap::from([
                (".llm_tool_spec".to_string(), tool_spec_handle_fd),
                (".tool_output".to_string(), tool_handle),
            ])
            .into_iter(),
        )?;

        // Create the chat messages alias immediately
        self.dagops.alias(".chat_messages", msg_handle)?;

        Ok(())
    }

    fn arguments_chunk(&mut self, args: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        // Write to tool input writer - unescape the JSON string first
        if let Some(ref mut writer) = self.tool_input_writer {
            // Create a new slice with quotes around the content to make it a valid JSON string
            let mut quoted_args = Vec::with_capacity(args.len() + 2);
            quoted_args.push(b'"');
            quoted_args.extend_from_slice(args);
            quoted_args.push(b'"');

            // Create a new Jiter and call known_str to unescape
            let mut jiter = scan_json::rjiter::jiter::Jiter::new(&quoted_args);
            let unescaped_str = jiter
                .known_str()
                .map_err(|e| format!("Failed to unescape JSON string: {e}"))?;

            writer
                .write_all(unescaped_str.as_bytes())
                .map_err(|e| e.to_string())?;
        }

        // Write to tool spec writer (arguments are already correctly escaped JSON)
        if let Some(ref mut writer) = self.tool_spec_writer {
            writer.write_all(args).map_err(|e| e.to_string())?;
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

impl<'a, T: DagOpsTrait> FunCallsToDag<'a, T> {
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
