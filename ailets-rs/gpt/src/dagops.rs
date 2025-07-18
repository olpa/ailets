//! DAG Operations Module

use crate::funcalls::ContentItemFunction;
use actor_io::AWriter;
use serde_json::json;
use std::collections::HashMap;
use std::io::Write;

pub trait DagOpsTrait {
    /// # Errors
    /// From the host
    fn value_node(&mut self, value: &[u8], explain: &str) -> Result<u32, String>;

    /// # Errors
    /// From the host
    fn alias(&mut self, alias: &str, node_handle: u32) -> Result<u32, String>;

    /// # Errors
    /// From the host
    fn detach_from_alias(&mut self, alias: &str) -> Result<(), String>;

    /// # Errors
    /// From the host
    fn instantiate_with_deps(
        &mut self,
        workflow_name: &str,
        deps: impl Iterator<Item = (String, u32)>,
    ) -> Result<u32, String>;

    /// # Errors
    /// From the host
    fn open_write_pipe(&mut self, explain: Option<&str>) -> Result<i32, String>;

    /// # Errors
    /// From the host
    fn alias_fd(&mut self, alias: &str, fd: u32) -> Result<u32, String>;

    /// Open a writer to a write pipe.
    /// Returns a writer that can be used to write data incrementally.
    /// # Errors
    /// From the host or I/O operations
    fn open_writer_to_pipe(&mut self, fd: i32) -> Result<Box<dyn Write>, String>;
}

pub struct DagOps;

impl DagOps {
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

/// One level of indirection to test that funcalls are collected correctly
pub trait InjectDagOpsTrait {
    /// Inject function calls into the workflow DAG.
    /// Do nothing if there are no tool calls.
    ///
    /// # Errors
    /// Promotes errors from the host.
    fn inject_tool_calls(&mut self, tool_calls: &[ContentItemFunction]) -> Result<(), String>;
}

pub struct InjectDagOps<T: DagOpsTrait> {
    dagops: T,
}

impl<T: DagOpsTrait> InjectDagOps<T> {
    #[must_use]
    pub fn new(dagops: T) -> Self {
        Self { dagops }
    }
}

impl<T: DagOpsTrait> InjectDagOpsTrait for InjectDagOps<T> {
    fn inject_tool_calls(&mut self, tool_calls: &[ContentItemFunction]) -> Result<(), String> {
        inject_tool_calls(&mut self.dagops, tool_calls)
    }
}

/// Put tool calls in chat history by creating a write pipe and aliasing it to `.chat_messages`
///
/// # Errors
/// Promotes errors from the host.
fn put_tool_calls_in_chat_history(
    dagops: &mut impl DagOpsTrait,
    tool_calls: &[ContentItemFunction],
) -> Result<(), String> {
    let mut tcch_lines = vec![
        serde_json::to_string(&json!([{"type":"ctl"},{"role":"assistant"}]))
            .map_err(|e| e.to_string())?,
    ];
    tcch_lines.extend(
        tool_calls
            .iter()
            .map(|tc| {
                serde_json::to_string(&json!([{
                    "type": "function",
                    "id": tc.id,
                    "name": tc.function_name,
                  },{
                    "arguments": tc.function_arguments
                }]))
                .map_err(|e| e.to_string())
            })
            .collect::<Result<Vec<_>, String>>()?,
    );
    let tcch = tcch_lines.join("\n");
    let explain = format!(
        "tool calls in chat history - {}",
        tool_calls
            .iter()
            .map(|tc| tc.function_name.as_str())
            .collect::<Vec<_>>()
            .join(" - ")
    );
    {
        let fd = dagops.open_write_pipe(Some(&explain))?;
        let mut writer = dagops.open_writer_to_pipe(fd)?;
        writer
            .write_all(tcch.as_bytes())
            .map_err(|e| e.to_string())?;
        #[allow(clippy::cast_sign_loss)]
        let fd_u32 = fd as u32;
        dagops.alias_fd(".chat_messages", fd_u32)?
    };
    Ok(())
}

/// Inject function calls into the workflow DAG.
/// Do nothing if there are no tool calls.
///
/// # Errors
/// Promotes errors from the host.
pub fn inject_tool_calls(
    dagops: &mut impl DagOpsTrait,
    tool_calls: &[ContentItemFunction],
) -> Result<(), String> {
    if tool_calls.is_empty() {
        return Ok(());
    }

    //
    // Don't interfere with previous model workflow:
    //
    // - We are going to update the chat history, by adding more to the alias ".chat_messages"
    // - The old finished run of "user prompt to messages" depended on ".chat_messages"
    // - If we don't detach, the dependency graph will show that the old "user prompt to messages"
    //   run depends on the new items in ".chat_messages". It's very very confusing,
    //   even despite the current runner implementation ignores the dependency changes
    //   of a finished step.
    //
    dagops.detach_from_alias(".chat_messages")?;

    //
    // Put tool calls in chat history
    //
    put_tool_calls_in_chat_history(dagops, tool_calls)?;

    //
    // Process each tool call
    //
    for tool_call in tool_calls {
        //
        // Run the tool
        //
        let explain = format!("tool input - {}", tool_call.function_name);
        let tool_input_fd = dagops.open_write_pipe(Some(&explain))?;
        #[allow(clippy::cast_sign_loss)]
        let tool_input_fd_u32 = tool_input_fd as u32;
        {
            let mut writer = dagops.open_writer_to_pipe(tool_input_fd)?;
            writer
                .write_all(tool_call.function_arguments.as_bytes())
                .map_err(|e| e.to_string())?;
            dagops.alias_fd(".tool_input", tool_input_fd_u32)?
        };

        let tool_name = &tool_call.function_name;
        let tool_handle = dagops.instantiate_with_deps(
            &format!(".tool.{tool_name}"),
            HashMap::from([(".tool_input".to_string(), tool_input_fd_u32)]).into_iter(),
        )?;

        //
        // Convert tool output to messages
        //
        let tool_spec = json!([{
            "type": "function",
            "id": tool_call.id,
            "name": tool_call.function_name,
        },{
            "arguments": tool_call.function_arguments
        }]);
        let explain = format!("tool call spec - {}", tool_call.function_name);
        let tool_spec_handle_fd = dagops.open_write_pipe(Some(&explain))?;
        #[allow(clippy::cast_sign_loss)]
        let tool_spec_handle_fd_u32 = tool_spec_handle_fd as u32;
        {
            let mut writer = dagops.open_writer_to_pipe(tool_spec_handle_fd)?;
            writer
                .write_all(
                    serde_json::to_string(&tool_spec)
                        .map_err(|e| e.to_string())?
                        .as_bytes(),
                )
                .map_err(|e| e.to_string())?;
            dagops.alias_fd(".llm_tool_spec", tool_spec_handle_fd_u32)?
        };

        let msg_handle = dagops.instantiate_with_deps(
            ".toolcall_to_messages",
            HashMap::from([
                (".llm_tool_spec".to_string(), tool_spec_handle_fd_u32),
                (".tool_output".to_string(), tool_handle),
            ])
            .into_iter(),
        )?;
        dagops.alias(".chat_messages", msg_handle)?;
    }

    // Rerun model
    let rerun_handle = dagops.instantiate_with_deps(
        ".gpt",
        HashMap::from([
            (".chat_messages.media".to_string(), 0),
            (".chat_messages.toolspecs".to_string(), 0),
        ])
        .into_iter(),
    )?;
    dagops.alias(".output_messages", rerun_handle)?;

    Ok(())
}
