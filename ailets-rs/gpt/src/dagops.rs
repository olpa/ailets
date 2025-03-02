//! DAG Operations Module

use crate::funcalls::ContentItemFunction;
use actor_runtime::DagOpsTrait;
use std::collections::HashMap;

/// One level of indirection to test that funcalls are collected correctly
pub trait InjectDagOpsTrait {
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

/// Inject function calls into the workflow DAG
///
/// # Errors
/// Promotes errors from the host.
pub fn inject_tool_calls(
    dagops: &mut impl DagOpsTrait,
    tool_calls: &[ContentItemFunction],
) -> Result<(), String> {
    // Create chat history value node
    //         tool_calls_message: ChatMessage = {
    //             "role": "assistant",
    //             "content": self.tool_calls,
    //         }
    let node = dagops.value_node(
        b"tool_calls",
        "Feed \"tool_calls\" from llm output to chat history",
    )?;
    dagops.alias(".chat_messages", node)?;

    // Process each tool call
    for tool_call in tool_calls {
        // Create tool spec value node
        //         tool_calls_message: ChatMessage = {
        //"role": "assistant",
        //"content": self.tool_calls,
        //}
        let tool_spec_handle = dagops.value_node(
            tool_call.function_arguments.as_bytes(),
            "Tool call spec from llm",
        )?;

        // Instantiate tool workflow
        let tool_name = &tool_call.function_name;
        let tool_handle = dagops.instantiate_with_deps(
            &format!(".tool.{tool_name}"),
            HashMap::from([(".tool_input".to_string(), tool_spec_handle)]).into_iter(),
        )?;

        // Convert tool output to messages
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
    let rerun_handle = dagops.instantiate_with_deps(".gpt4o", HashMap::new().into_iter())?;
    dagops.alias(".model_output", rerun_handle)?;

    Ok(())
}
