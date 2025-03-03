#[cfg(test)]
#[macro_use]
extern crate hamcrest;
use crate::dagops_mock::TrackedDagOps;
use gpt::dagops::inject_tool_calls;
use gpt::funcalls::ContentItemFunction;
use hamcrest::prelude::*;
use serde_json::json;
use std::collections::HashMap;
pub mod dagops_mock;

#[test]
fn inject_tool_calls_to_dag() {
    // Arrange
    let mut tracked_dagops = TrackedDagOps::default();

    let tool_calls = vec![
        ContentItemFunction::new("call_1", "get_weather", "{\"city\":\"London\"}"),
        ContentItemFunction::new("call_2", "get_forecast", "{\"days\":5}"),
    ];

    // Act
    inject_tool_calls(&mut tracked_dagops, &tool_calls).unwrap();

    // Assert that the value nodes are created:
    // - 1 for chat history, with 2 tool calls
    // - 2 for tool calls input
    // - 2 for output to chat history
    let value_nodes = &tracked_dagops.value_nodes;
    assert_that!(value_nodes.len(), is(equal_to(5)));

    //
    // Assert: tool calls are in the chat history
    //
    let (handle_tcch, explain_tcch, value_tcch) =
        tracked_dagops.parse_value_node(&tracked_dagops.value_nodes[0]);
    assert_that!(
        &explain_tcch,
        matches_regex("tool calls in chat history - get_weather - get_forecast")
    );

    let expected_tcch = json!([
        {
            "role": "assistant",
            "tool_calls": [
                {
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": "get_weather",
                        "arguments": "{\"city\":\"London\"}"
                    }
                },
                {
                    "id": "call_2",
                    "type": "function",
                    "function": {
                        "name": "get_forecast",
                        "arguments": "{\"days\":5}"
                    }
                }
            ]
        }
    ]);
    let value_tcch =
        serde_json::from_str(&value_tcch).expect(&format!("Failed to parse JSON: {value_tcch}"));
    assert_that!(value_tcch, is(equal_to(expected_tcch)));

    //
    // Assert: `get_weather` tool input and call spec
    //
    let (handle_tool_input1, explain_tool_input1, value_tool_input1) =
        tracked_dagops.parse_value_node(&tracked_dagops.value_nodes[1]);
    assert_that!(
        &explain_tool_input1,
        matches_regex("tool input - get_weather")
    );
    let expected_tool_input1 = json!({"city": "London"});
    let value_tool_input1 = serde_json::from_str(&value_tool_input1)
        .expect(&format!("Failed to parse JSON: {value_tool_input1}"));
    assert_that!(value_tool_input1, is(equal_to(expected_tool_input1)));

    //

    let (handle_toolspec_input1, explain_toolspec_input1, value_toolspec_input1) =
        tracked_dagops.parse_value_node(&tracked_dagops.value_nodes[2]);
    assert_that!(
        &explain_toolspec_input1,
        matches_regex("tool call spec - get_weather")
    );
    let expected_toolspec_input1 = json!(
        {
            "id": "call_1",
            "type": "function",
            "function": {
                "name": "get_weather",
                "arguments": "{\"city\":\"London\"}"
            }
        }
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
        tracked_dagops.parse_value_node(&tracked_dagops.value_nodes[3]);
    assert_that!(
        &explain_tool_input2,
        matches_regex("tool input - get_forecast")
    );
    let expected_tool_input2 = json!({"days": 5});
    let value_tool_input2 = serde_json::from_str(&value_tool_input2)
        .expect(&format!("Failed to parse JSON: {value_tool_input2}"));
    assert_that!(value_tool_input2, is(equal_to(expected_tool_input2)));

    //

    let (handle_toolspec_input2, explain_toolspec_input2, value_toolspec_input2) =
        tracked_dagops.parse_value_node(&tracked_dagops.value_nodes[4]);
    assert_that!(
        &explain_toolspec_input2,
        matches_regex("tool call spec - get_forecast")
    );
    let expected_toolspec_input2 = json!(
        {
            "id": "call_2",
            "type": "function",
            "function": {
                "name": "get_forecast",
                "arguments": "{\"days\":5}"
            }
        }
    );
    let value_toolspec_input2 = serde_json::from_str(&value_toolspec_input2)
        .expect(&format!("Failed to parse JSON: {value_toolspec_input2}"));
    assert_that!(
        value_toolspec_input2,
        is(equal_to(expected_toolspec_input2))
    );

    // Assert that the workflows are created:
    // - 4 for tools: 2x tools itself and 2x output to chat history
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
    let (handle_aftercall_1, aftercall_workflow_1, deps_aftercall_1) =
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

    let (handle_aftercall_2, aftercall_workflow_2, deps_aftercall_2) =
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
    let (handle_rerun, rerun_workflow, deps_rerun) = tracked_dagops.parse_workflow(&workflows[4]);
    assert_that!(rerun_workflow, is(equal_to(format!(".gpt4o"))));
    assert_that!(deps_rerun, is(equal_to(HashMap::from([]))));

    //
    // Assert: aliases
    // - 1 for chat history
    // - 2 for tool calls
    // - 1 for model output
    //
    assert_eq!(tracked_dagops.aliases.len(), 4);

    let (_, alias_name, alias_handle) = tracked_dagops.parse_alias(&tracked_dagops.aliases[0]);
    assert_that!(&alias_name, is(equal_to(".chat_messages")));
    assert_that!(alias_handle, is(equal_to(handle_tcch)));

    let (_, alias_name, alias_handle) = tracked_dagops.parse_alias(&tracked_dagops.aliases[1]);
    assert_that!(&alias_name, is(equal_to(".chat_messages")));
    assert_that!(alias_handle, is(equal_to(handle_aftercall_1)));

    let (_, alias_name, alias_handle) = tracked_dagops.parse_alias(&tracked_dagops.aliases[2]);
    assert_that!(&alias_name, is(equal_to(".chat_messages")));
    assert_that!(alias_handle, is(equal_to(handle_aftercall_2)));

    let (_, alias_name, alias_handle) = tracked_dagops.parse_alias(&tracked_dagops.aliases[3]);
    assert_that!(&alias_name, is(equal_to(".model_output")));
    assert_that!(alias_handle, is(equal_to(handle_rerun)));
}

#[test]
fn inject_empty_tool_calls_to_dag() {
    // Arrange
    let mut tracked_dagops = TrackedDagOps::default();
    let tool_calls: Vec<ContentItemFunction> = vec![];

    // Act
    inject_tool_calls(&mut tracked_dagops, &tool_calls).unwrap();

    // Assert no operations were performed
    assert_that!(tracked_dagops.value_nodes.len(), is(equal_to(0)));
    assert_that!(tracked_dagops.workflows.len(), is(equal_to(0)));
    assert_that!(tracked_dagops.aliases.len(), is(equal_to(0)));
}
