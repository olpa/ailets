#[cfg(test)]
#[macro_use]
extern crate hamcrest;
use crate::dagops_mock::TrackedDagOps;
use gpt::funcalls::{FunCallsWrite, FunCallsGpt};
use hamcrest::prelude::*;
use serde_json::json;
use std::collections::HashMap;
use actor_runtime_mocked::RcWriter;
pub mod dagops_mock;

#[test]
fn inject_tool_calls_to_dag() {
    //
    // Arrange
    //
    let mut tracked_dagops = TrackedDagOps::default();
    let writer = RcWriter::new();
    
    //
    // Act
    //
    {
        let mut dagops_writer = FunCallsGpt::new(writer.clone(), &mut tracked_dagops);

        dagops_writer.new_item(0, "call_1".to_string(), "get_weather".to_string()).unwrap();
        dagops_writer.arguments_chunk("{\"city\":\"London\"}".to_string()).unwrap();
        dagops_writer.end_item().unwrap();

        dagops_writer.new_item(1, "call_2".to_string(), "get_forecast".to_string()).unwrap();
        dagops_writer.arguments_chunk("{\"days\":5}".to_string()).unwrap();
        dagops_writer.end_item().unwrap();
        
        dagops_writer.end().unwrap();
    } // dagops_writer is dropped here, releasing the borrow

    //
    // Assert
    //

    //
    // Detached old nodes
    //
    let expected_detached = vec![".chat_messages".to_string()];
    assert_that!(&tracked_dagops.detached, is(equal_to(&expected_detached)));

    // Assert that the value nodes are created:
    // - 2 for tool calls input (one per tool call)
    // - 2 for tool call specs (one per tool call)
    let value_nodes = &tracked_dagops.value_nodes;
    assert_that!(value_nodes.len(), is(equal_to(4)));

    //
    // Assert: tool calls are in the chat history (written by FunCallsToChat)
    //
    let expected_chat_output = "[{\"type\":\"tool_call\"},{\"id\":\"call_1\",\"function_name\":\"get_weather\",\"function_arguments\":\"{\"city\":\"London\"}\"}]\n[{\"type\":\"tool_call\"},{\"id\":\"call_2\",\"function_name\":\"get_forecast\",\"function_arguments\":\"{\"days\":5}\"}]\n";
    assert_eq!(writer.get_output(), expected_chat_output);

    //
    // Assert: `get_weather` tool input and call spec
    //
    let (handle_tool_input1, explain_tool_input1, value_tool_input1) =
        tracked_dagops.parse_value_node(&value_nodes[0]);
    assert_that!(
        &explain_tool_input1,
        matches_regex("tool input - get_weather")
    );
    let expected_tool_input1 = json!({"city": "London"});
    let value_tool_input1 = serde_json::from_str(&value_tool_input1)
        .expect(&format!("Failed to parse JSON: {value_tool_input1}"));
    assert_that!(value_tool_input1, is(equal_to(expected_tool_input1)));

    let (handle_toolspec_input1, explain_toolspec_input1, value_toolspec_input1) =
        tracked_dagops.parse_value_node(&value_nodes[1]);
    assert_that!(
        &explain_toolspec_input1,
        matches_regex("tool call spec - get_weather")
    );
    let expected_toolspec_input1 = json!(
        [{
            "type": "function",
            "id": "call_1",
            "name": "get_weather",
        },{
            "arguments": "{\"city\":\"London\"}"
        }]
    );
    let value_toolspec_input1 = serde_json::from_str(&value_toolspec_input1)
        .expect(&format!("Failed to parse JSON: {value_toolspec_input1}"));
    assert_that!(
        value_toolspec_input1,
        is(equal_to(expected_toolspec_input1))
    );

    //
    // Assert: `get_forecast` tool input and call spec
    //
    let (handle_tool_input2, explain_tool_input2, value_tool_input2) =
        tracked_dagops.parse_value_node(&value_nodes[2]);
    assert_that!(
        &explain_tool_input2,
        matches_regex("tool input - get_forecast")
    );
    let expected_tool_input2 = json!({"days": 5});
    let value_tool_input2 = serde_json::from_str(&value_tool_input2)
        .expect(&format!("Failed to parse JSON: {value_tool_input2}"));
    assert_that!(value_tool_input2, is(equal_to(expected_tool_input2)));

    let (handle_toolspec_input2, explain_toolspec_input2, value_toolspec_input2) =
        tracked_dagops.parse_value_node(&value_nodes[3]);
    assert_that!(
        &explain_toolspec_input2,
        matches_regex("tool call spec - get_forecast")
    );
    let expected_toolspec_input2 = json!(
        [{
            "type": "function",
            "id": "call_2",
            "name": "get_forecast",
          },{
            "arguments": "{\"days\":5}"
        }]
    );
    let value_toolspec_input2 = serde_json::from_str(&value_toolspec_input2)
        .expect(&format!("Failed to parse JSON: {value_toolspec_input2}"));
    assert_that!(
        value_toolspec_input2,
        is(equal_to(expected_toolspec_input2))
    );

    // Assert that the workflows are created:
    // - 2 for tools themselves
    // - 2 for output to chat history
    // - 1 to re-run the model
    let workflows = &tracked_dagops.workflows;
    assert_eq!(workflows.len(), 5);

    //
    // Assert: call tools
    //
    let (handle_tool_1, tool_workflow_1, deps_tool_1) =
        tracked_dagops.parse_workflow(&workflows[0]);
    assert_that!(tool_workflow_1, is(equal_to(format!(".tool.get_weather"))));
    assert_that!(
        deps_tool_1,
        is(equal_to(HashMap::from([(
            ".tool_input".to_string(),
            handle_tool_input1
        )])))
    );

    let (handle_tool_2, tool_workflow_2, deps_tool_2) =
        tracked_dagops.parse_workflow(&workflows[2]);
    assert_that!(tool_workflow_2, is(equal_to(format!(".tool.get_forecast"))));
    assert_that!(
        deps_tool_2,
        is(equal_to(HashMap::from([(
            ".tool_input".to_string(),
            handle_tool_input2
        )])))
    );

    //
    // Assert: tool output to chat history
    //
    let (_handle_aftercall_1, aftercall_workflow_1, deps_aftercall_1) =
        tracked_dagops.parse_workflow(&workflows[1]);
    assert_that!(
        aftercall_workflow_1,
        is(equal_to(format!(".toolcall_to_messages")))
    );
    assert_that!(
        deps_aftercall_1,
        is(equal_to(HashMap::from([
            (".tool_output".to_string(), handle_tool_1),
            (".llm_tool_spec".to_string(), handle_toolspec_input1),
        ])))
    );

    let (_handle_aftercall_2, aftercall_workflow_2, deps_aftercall_2) =
        tracked_dagops.parse_workflow(&workflows[3]);
    assert_that!(
        aftercall_workflow_2,
        is(equal_to(format!(".toolcall_to_messages")))
    );
    assert_that!(
        deps_aftercall_2,
        is(equal_to(HashMap::from([
            (".tool_output".to_string(), handle_tool_2),
            (".llm_tool_spec".to_string(), handle_toolspec_input2),
        ])))
    );

    //
    // Assert: re-run the model
    //
    let (_handle_rerun, rerun_workflow, deps_rerun) = tracked_dagops.parse_workflow(&workflows[4]);
    assert_that!(rerun_workflow, is(equal_to(format!(".gpt"))));
    assert_that!(
        deps_rerun,
        is(equal_to(HashMap::from([
            (".chat_messages.media".to_string(), 0),
            (".chat_messages.toolspecs".to_string(), 0),
        ])))
    );

    //
    // Assert: aliases
    // - For each tool call (2): 1 for tool_input + 1 for llm_tool_spec + 1 for chat_messages
    // - 1 for model output
    // Total: (3 * 2) + 1 = 7
    //
    assert_eq!(tracked_dagops.aliases.len(), 7);

    // Aliases for first tool call
    let (_, alias_name, alias_handle) = tracked_dagops.parse_alias(&tracked_dagops.aliases[0]);
    assert_that!(&alias_name, is(equal_to(".tool_input")));
    assert_that!(alias_handle, is(equal_to(handle_tool_input1)));

    let (_, alias_name, alias_handle) = tracked_dagops.parse_alias(&tracked_dagops.aliases[1]);
    assert_that!(&alias_name, is(equal_to(".llm_tool_spec")));
    assert_that!(alias_handle, is(equal_to(handle_toolspec_input1)));

    let (_, alias_name, _alias_handle) = tracked_dagops.parse_alias(&tracked_dagops.aliases[2]);
    assert_that!(&alias_name, is(equal_to(".chat_messages")));

    // Aliases for second tool call
    let (_, alias_name, alias_handle) = tracked_dagops.parse_alias(&tracked_dagops.aliases[3]);
    assert_that!(&alias_name, is(equal_to(".tool_input")));
    assert_that!(alias_handle, is(equal_to(handle_tool_input2)));

    let (_, alias_name, alias_handle) = tracked_dagops.parse_alias(&tracked_dagops.aliases[4]);
    assert_that!(&alias_name, is(equal_to(".llm_tool_spec")));
    assert_that!(alias_handle, is(equal_to(handle_toolspec_input2)));

    let (_, alias_name, _alias_handle) = tracked_dagops.parse_alias(&tracked_dagops.aliases[5]);
    assert_that!(&alias_name, is(equal_to(".chat_messages")));

    // Final alias for model output
    let (_, alias_name, _alias_handle) = tracked_dagops.parse_alias(&tracked_dagops.aliases[6]);
    assert_that!(&alias_name, is(equal_to(".output_messages")));
}

#[test]
fn inject_empty_tool_calls_to_dag() {
    // Arrange
    let mut tracked_dagops = TrackedDagOps::default();
    let writer = RcWriter::new();
    
    {
        let _dagops_writer = FunCallsGpt::new(writer.clone(), &mut tracked_dagops);

        // Act - Don't call any methods on the writer (equivalent to empty tool calls)
    } // dagops_writer is dropped here, releasing the borrow

    // Assert no operations were performed
    assert_that!(tracked_dagops.value_nodes.len(), is(equal_to(0)));
    assert_that!(tracked_dagops.workflows.len(), is(equal_to(0)));
    assert_that!(tracked_dagops.aliases.len(), is(equal_to(0)));
    assert_that!(tracked_dagops.detached.len(), is(equal_to(0)));
}