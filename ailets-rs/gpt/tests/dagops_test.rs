use gpt::funcalls::{ContentItemFunction, FunCalls};
use gpt::dagops::InjectDagOpsTrait;
use crate::dagops_mock::{TrackedDagOps, TrackedInjectDagOps};

mod dagops_mock;

#[test]
fn test_inject_funcalls_to_dag() {
    // Arrange
    let tracked_dagops = TrackedDagOps::default();
    let inject_dagops = InjectDagOps::new(tracked_dagops);

    let tool_calls = vec![
        ContentItemFunction::new("call_1", "get_weather", "{\"city\":\"London\"}"),
        ContentItemFunction::new("call_2", "get_forecast", "{\"days\":5}"),
    ];

    // Act
    inject_dagops.inject_tool_calls(&tool_calls).unwrap();


    // Assert
    let injected_calls = tracked_inject.get_funcalls();
    assert_eq!(injected_calls.len(), 2);
    
    // Verify first function call
    assert_eq!(injected_calls[0].id, "call_1");
    assert_eq!(injected_calls[0].function_name, "get_weather");
    assert_eq!(injected_calls[0].function_arguments, "{\"city\":\"London\"}");
    
    // Verify second function call
    assert_eq!(injected_calls[1].id, "call_2");
    assert_eq!(injected_calls[1].function_name, "get_forecast");
    assert_eq!(injected_calls[1].function_arguments, "{\"days\":5}");

    // Get tracked DAG operations
    let tracked_dagops = tracked_inject.get_dagops();

    // Verify value nodes
    assert_eq!(tracked_dagops.value_nodes.len(), 3); // 1 for chat history + 2 for tool specs
    assert!(tracked_dagops.value_nodes[0].contains("Feed \"tool_calls\" from output to input"));
    assert!(tracked_dagops.value_nodes[1].contains("Tool call spec from llm"));
    assert!(tracked_dagops.value_nodes[2].contains("Tool call spec from llm"));

    // Verify tool workflow instantiation
    assert_eq!(tracked_dagops.workflows.len(), 4); // 2 tools + 2 message conversions
    assert!(tracked_dagops.workflows[0].contains(".tool.get_weather"));
    assert!(tracked_dagops.workflows[1].contains(".toolcall_to_messages"));
    assert!(tracked_dagops.workflows[2].contains(".tool.get_forecast"));
    assert!(tracked_dagops.workflows[3].contains(".toolcall_to_messages"));

    // Verify aliases
    assert_eq!(tracked_dagops.aliases.len(), 4); // 1 initial + 2 for chat_messages + 1 for model_output
    assert!(tracked_dagops.aliases.iter().any(|a| a.contains(".chat_messages:0"))); // Initial chat history
    assert!(tracked_dagops.aliases.iter().any(|a| a.contains(".chat_messages:3"))); // First tool message
    assert!(tracked_dagops.aliases.iter().any(|a| a.contains(".chat_messages:5"))); // Second tool message
    assert!(tracked_dagops.aliases.iter().any(|a| a.contains(".model_output:6"))); // Model rerun
} 