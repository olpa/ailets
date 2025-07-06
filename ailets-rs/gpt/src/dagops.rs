//! DAG Operations Module

use crate::funcalls::ContentItemFunction;
use actor_runtime::DagOpsTrait;
use serde_json::json;
use std::collections::HashMap;

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
    // Don't interfere with previous model workflow
    //
    dagops.detach_from_alias(".chat_messages")?;

    //
    // Put tool calls in chat history
    //
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
    let node = dagops.value_node(tcch.as_bytes(), &explain)?;
    dagops.alias(".chat_messages", node)?;

    //
    // Process each tool call
    //
    for tool_call in tool_calls {
        //
        // Run the tool
        //
        let explain = format!("tool input - {}", tool_call.function_name);
        let tool_input = dagops.value_node(tool_call.function_arguments.as_bytes(), &explain)?;

        let tool_name = &tool_call.function_name;
        let tool_handle = dagops.instantiate_with_deps(
            &format!(".tool.{tool_name}"),
            HashMap::from([(".tool_input".to_string(), tool_input)]).into_iter(),
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
        let tool_spec_handle = dagops.value_node(
            serde_json::to_string(&tool_spec)
                .map_err(|e| e.to_string())?
                .as_bytes(),
            &explain,
        )?;

        let msg_handle = dagops.instantiate_with_deps(
            ".toolcall_to_messages",
            HashMap::from([
                (".llm_tool_spec".to_string(), tool_spec_handle),
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
