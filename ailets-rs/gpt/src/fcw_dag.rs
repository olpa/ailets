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
pub struct FunCallsToDag {
    /// Writer for the current tool's input data
    tool_input_writer: Option<Box<dyn Write>>,
    /// Writer for the current tool's specification (JSON)
    tool_spec_writer: Option<Box<dyn Write>>,
}

impl FunCallsToDag {
    /// Creates a new DAG-integrated function call writer
    ///
    /// # Returns
    /// A new writer that will create DAG operations for each function call
    #[must_use]
    pub fn new() -> Self {
        Self {
            tool_input_writer: None,
            tool_spec_writer: None,
        }
    }
}

impl FunCallsWrite for FunCallsToDag {
    fn new_item<T: DagOpsTrait>(&mut self, id: &str, name: &str, dagops: &mut T) -> Result<(), Box<dyn std::error::Error>> {
        // Create the tool input pipe and writer
        let explain = format!("tool input - {name}");
        let tool_input_fd = dagops.open_write_pipe(Some(&explain))?;
        let tool_input_writer = dagops.open_writer_to_pipe(tool_input_fd)?;

        self.tool_input_writer = Some(tool_input_writer);

        // Create the tool spec pipe and writer
        let explain = format!("tool call spec - {name}");
        let tool_spec_handle_fd = dagops.open_write_pipe(Some(&explain))?;
        let mut tool_spec_writer = dagops.open_writer_to_pipe(tool_spec_handle_fd)?;

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
        dagops.alias_fd(".tool_input", tool_input_fd)?;
        dagops
            .alias_fd(".llm_tool_spec", tool_spec_handle_fd)?;

        // Run the tool
        let tool_handle = dagops.instantiate_with_deps(
            &format!(".tool.{name}"),
            HashMap::from([(".tool_input".to_string(), tool_input_fd)]).into_iter(),
        )?;

        // Convert tool output to messages
        let msg_handle = dagops.instantiate_with_deps(
            ".toolcall_to_messages",
            HashMap::from([
                (".llm_tool_spec".to_string(), tool_spec_handle_fd),
                (".tool_output".to_string(), tool_handle),
            ])
            .into_iter(),
        )?;

        // Create the chat messages alias immediately
        dagops.alias(".chat_messages", msg_handle)?;

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
        // Placeholder - actual DAG workflow ending is handled by FunCallsBuilder
        Ok(())
    }
}

impl Default for FunCallsToDag {
    fn default() -> Self {
        Self::new()
    }
}

