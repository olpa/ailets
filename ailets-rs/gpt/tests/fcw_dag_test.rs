#[cfg(test)]
#[macro_use]
extern crate hamcrest;
use crate::dagops_mock::TrackedDagOps;
use actor_runtime_mocked::RcWriter;
use gpt::fcw_chat::FunCallsToChat;
use gpt::fcw_dag::FunCallsToDag;
use gpt::fcw_trait::FunCallsWrite;
use hamcrest::prelude::*;
use serde_json::json;
use std::collections::HashMap;
pub mod dagops_mock;

// Testing not only "fcw_dag" but also the complete functionality,
// including the chat history and tool calls.

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
        let mut chat_writer = FunCallsToChat::new(writer.clone());
        let mut dagops_writer = FunCallsToDag::new(&mut tracked_dagops);

        // Write to both chat and DAG
        chat_writer.new_item("call_1", "get_weather").unwrap();
        dagops_writer.new_item("call_1", "get_weather").unwrap();

        chat_writer
            .arguments_chunk("{\\\"city\\\":\\\"London\\\"}")
            .unwrap();
        dagops_writer
            .arguments_chunk("{\"city\":\"London\"}")
            .unwrap();

        chat_writer.end_item().unwrap();
        dagops_writer.end_item().unwrap();

        chat_writer.new_item("call_2", "get_forecast").unwrap();
        dagops_writer.new_item("call_2", "get_forecast").unwrap();

        chat_writer.arguments_chunk("{\\\"days\\\":5}").unwrap();
        dagops_writer.arguments_chunk("{\"days\":5}").unwrap();

        chat_writer.end_item().unwrap();
        dagops_writer.end_item().unwrap();

        chat_writer.end().unwrap();
        dagops_writer.end().unwrap();
    } // writers are dropped here, releasing the borrow

    //
    // Assert
    //

    //
    // Detached old nodes
    //
    let expected_detached = vec![".chat_messages".to_string()];
    assert_that!(tracked_dagops.detached(), is(equal_to(&expected_detached)));

    // Assert that the value nodes are created:
    // - 2 for tool calls input (one per tool call)
    // - 2 for tool call specs (one per tool call)
    let value_nodes = tracked_dagops.value_nodes();
    assert_that!(value_nodes.len(), is(equal_to(4)));

    //
    // Assert: tool calls are in the chat history (written by FunCallsToChat)
    //
    let expected_chat_output = r#"[{"type":"function","id":"call_1","name":"get_weather"},{"arguments":"{\"city\":\"London\"}"}]
[{"type":"function","id":"call_2","name":"get_forecast"},{"arguments":"{\"days\":5}"}]
"#;
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
    let workflows = tracked_dagops.workflows();
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
    assert_eq!(tracked_dagops.aliases().len(), 7);

    // Aliases for first tool call
    let (_, alias_name, alias_handle) = tracked_dagops.parse_alias(&tracked_dagops.aliases()[0]);
    assert_that!(&alias_name, is(equal_to(".tool_input")));
    assert_that!(alias_handle, is(equal_to(handle_tool_input1)));

    let (_, alias_name, alias_handle) = tracked_dagops.parse_alias(&tracked_dagops.aliases()[1]);
    assert_that!(&alias_name, is(equal_to(".llm_tool_spec")));
    assert_that!(alias_handle, is(equal_to(handle_toolspec_input1)));

    let (_, alias_name, _alias_handle) = tracked_dagops.parse_alias(&tracked_dagops.aliases()[2]);
    assert_that!(&alias_name, is(equal_to(".chat_messages")));

    // Aliases for second tool call
    let (_, alias_name, alias_handle) = tracked_dagops.parse_alias(&tracked_dagops.aliases()[3]);
    assert_that!(&alias_name, is(equal_to(".tool_input")));
    assert_that!(alias_handle, is(equal_to(handle_tool_input2)));

    let (_, alias_name, alias_handle) = tracked_dagops.parse_alias(&tracked_dagops.aliases()[4]);
    assert_that!(&alias_name, is(equal_to(".llm_tool_spec")));
    assert_that!(alias_handle, is(equal_to(handle_toolspec_input2)));

    let (_, alias_name, _alias_handle) = tracked_dagops.parse_alias(&tracked_dagops.aliases()[5]);
    assert_that!(&alias_name, is(equal_to(".chat_messages")));

    // Final alias for model output
    let (_, alias_name, _alias_handle) = tracked_dagops.parse_alias(&tracked_dagops.aliases()[6]);
    assert_that!(&alias_name, is(equal_to(".output_messages")));
}

#[test]
fn inject_empty_tool_calls_to_dag() {
    // Arrange
    let mut tracked_dagops = TrackedDagOps::default();
    let writer = RcWriter::new();

    {
        let _dagops_writer = FunCallsToDag::new(&mut tracked_dagops);

        // Act - Don't call any methods on the writer (equivalent to empty tool calls)
    } // dagops_writer is dropped here, releasing the borrow

    // Assert no operations were performed
    assert_that!(tracked_dagops.value_nodes().len(), is(equal_to(0)));
    assert_that!(tracked_dagops.workflows().len(), is(equal_to(0)));
    assert_that!(tracked_dagops.aliases().len(), is(equal_to(0)));
    assert_that!(tracked_dagops.detached().len(), is(equal_to(0)));
}

#[test]
fn multiple_arguments_chunks() {
    //
    // Arrange
    //
    let mut tracked_dagops = TrackedDagOps::default();
    let writer = RcWriter::new();

    //
    // Act
    //
    {
        let mut chat_writer = FunCallsToChat::new(writer.clone());
        let mut dagops_writer = FunCallsToDag::new(&mut tracked_dagops);

        // Write to both chat and DAG
        chat_writer.new_item("call_1", "get_weather").unwrap();
        dagops_writer.new_item("call_1", "get_weather").unwrap();

        // Call arguments_chunk multiple times with different chunks
        chat_writer.arguments_chunk("{\\\"city\\\":").unwrap();
        dagops_writer.arguments_chunk("{\"city\":").unwrap();

        chat_writer.arguments_chunk("\\\"London\\\",").unwrap();
        dagops_writer.arguments_chunk("\"London\",").unwrap();

        chat_writer.arguments_chunk("\\\"country\\\":\\\"UK\\\"}").unwrap();
        dagops_writer
            .arguments_chunk("\"country\":\"UK\"}")
            .unwrap();

        chat_writer.end_item().unwrap();
        dagops_writer.end_item().unwrap();

        chat_writer.end().unwrap();
        dagops_writer.end().unwrap();
    } // writers are dropped here, releasing the borrow

    //
    // Assert
    //

    // Expected complete arguments after all chunks
    let expected_complete_args = r#"{"city":"London","country":"UK"}"#;
    let expected_complete_args_escaped = r#"{\"city\":\"London\",\"country\":\"UK\"}"#;

    //
    // Assert: chat output contains complete arguments
    //
    let expected_chat_output = format!(
        "[{{\"type\":\"function\",\"id\":\"call_1\",\"name\":\"get_weather\"}},{{\"arguments\":\"{}\"}}]\n",
        expected_complete_args_escaped
    );
    assert_eq!(writer.get_output(), expected_chat_output);

    // Assert that we have the expected value nodes (tool input and tool spec)
    let value_nodes = tracked_dagops.value_nodes();
    assert_that!(value_nodes.len(), is(equal_to(2)));

    //
    // Assert: tool input contains complete arguments
    //
    let (_, _, tool_input_value) = tracked_dagops.parse_value_node(&value_nodes[0]);
    let tool_input_json: serde_json::Value = serde_json::from_str(&tool_input_value).expect(
        &format!("Failed to parse tool input JSON: {tool_input_value}"),
    );
    let expected_input_json: serde_json::Value =
        serde_json::from_str(expected_complete_args).expect("Failed to parse expected input JSON");
    assert_that!(tool_input_json, is(equal_to(expected_input_json)));

    //
    // Assert: tool spec contains complete arguments
    //
    let (_, _, tool_spec_value) = tracked_dagops.parse_value_node(&value_nodes[1]);
    let tool_spec_json: serde_json::Value = serde_json::from_str(&tool_spec_value).expect(
        &format!("Failed to parse tool spec JSON: {tool_spec_value}"),
    );

    let expected_tool_spec = serde_json::json!([
        {
            "type": "function",
            "id": "call_1",
            "name": "get_weather",
        },
        {
            "arguments": expected_complete_args
        }
    ]);
    assert_that!(tool_spec_json, is(equal_to(expected_tool_spec)));
}
