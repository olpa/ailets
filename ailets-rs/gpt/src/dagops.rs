//! DAG Operations Module

use crate::funcalls::FunCalls;

pub trait InjectDagOpsTrait {
    /// # Errors
    /// Promotes errors from the host.
    fn inject_funcalls(&self, funcalls: &FunCalls) -> Result<(), String>;
}

pub struct InjectDagOps;

impl InjectDagOpsTrait for InjectDagOps {
    fn inject_funcalls(&self, _funcalls: &FunCalls) -> Result<(), String> {
        Ok(())
    }
}

/// Inject function calls into a DagOpsTrait implementation
pub fn inject_funcalls_to_dagops(dagops: &impl DagOpsTrait, funcalls: &FunCalls) -> Result<(), String> {
    // Create chat history value node
    dagops.value_node(b"tool_calls", "Feed \"tool_calls\" from output to input")?;
    dagops.alias(".chat_messages", 0)?;

    // Process each tool call
    for tool_call in funcalls.get_tool_calls() {
        // Create tool spec value node
        let tool_spec_handle = dagops.value_node(
            tool_call.function_arguments.as_bytes(),
            "Tool call spec from llm",
        )?;

        // Instantiate tool workflow
        let tool_name = &tool_call.function_name;
        let tool_handle = dagops.instantiate_with_deps(
            &format!(".tool.{tool_name}"),
            &HashMap::from([(".tool_input".to_string(), tool_spec_handle)]),
        )?;

        // Convert tool output to messages
        let msg_handle = dagops.instantiate_with_deps(
            ".toolcall_to_messages",
            &HashMap::from([
                (".llm_tool_spec".to_string(), tool_spec_handle),
                (".tool_output".to_string(), tool_handle),
            ]),
        )?;
        dagops.alias(".chat_messages", msg_handle)?;
    }

    // Rerun model
    let rerun_handle = dagops.instantiate_with_deps(".gpt4o", &HashMap::new())?;
    dagops.alias(".model_output", rerun_handle)?;

    Ok(())
}
