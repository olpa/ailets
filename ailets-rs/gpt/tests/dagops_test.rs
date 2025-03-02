#[cfg(test)]
#[macro_use]
extern crate hamcrest;
use hamcrest::prelude::*;

use crate::dagops_mock::TrackedDagOps;
use gpt::dagops::inject_tool_calls;
use gpt::funcalls::ContentItemFunction;
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
    let value_nodes = tracked_dagops.value_nodes;
    assert_that!(value_nodes.len(), is(equal_to(3)));
    assert_that!(
        &value_nodes[0],
        matches_regex("Feed \"tool_calls\" from output to input")
    );
    assert_that!(&value_nodes[1], matches_regex("Tool call spec from llm"));
    assert_that!(&value_nodes[2], matches_regex("Tool call spec from llm"));

    // Assert that the workflows are created:
    // - 2 for tools
    // - 1 to re-run the model
    let workflows = tracked_dagops.workflows;
    assert_eq!(workflows.len(), 3);
    let tool_workflow_1 = &workflows[0];
    assert!(tool_workflow_1.contains("X .tool.get_weather"));
    let tool_workflow_2 = &workflows[1];
    assert!(tool_workflow_2.contains("X .toolcall_to_messages"));
    let rerun_workflow = &workflows[2];
    assert!(rerun_workflow.contains("X .gpt4o"));

    // Verify aliases
    // - 1 for chat history
    // - 2 for tool calls
    // - 1 for model output
    assert_eq!(tracked_dagops.aliases.len(), 4);
    assert!(tracked_dagops
        .aliases
        .iter()
        .any(|a| a.contains("X .chat_messages")));
    assert!(tracked_dagops
        .aliases
        .iter()
        .any(|a| a.contains("X .tool.get_weather")));
    assert!(tracked_dagops
        .aliases
        .iter()
        .any(|a| a.contains("X .tool.get_forecast")));
    assert!(tracked_dagops
        .aliases
        .iter()
        .any(|a| a.contains("X .model_output")));
}
