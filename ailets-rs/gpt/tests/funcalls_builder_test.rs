use actor_runtime_mocked::RcWriter;
use gpt::fcw_chat::FunCallsToChat;
use gpt::fcw_tools::FunCallsToTools;
use gpt::funcalls_builder::FunCallsBuilder;

pub mod dagops_mock;
use dagops_mock::TrackedDagOps;

// Helper assertion functions
fn assert_writers(
    tracked_dagops: &TrackedDagOps,
    id: &str,
    name: &str,
    arguments: &str,
) {
    // Check that ChatWriter was used
    let expected_chat_output = format!(
        r#"[{{"type":"function","id":"{}","name":"{}"}},{{"arguments":"{}"}}]"#,
        id, name, arguments
    );
    
    // Get chat_output as the first value from tracked_dagops
    let value_nodes = tracked_dagops.value_nodes();
    assert!(!value_nodes.is_empty(), "Expected at least one value node for chat output");
    let first_node = &value_nodes[0];
    let (_, _, chat_output) = tracked_dagops.parse_value_node(first_node);
    
    assert!(
        chat_output.contains(&expected_chat_output),
        "Expected output to contain: {}\nActual output: {}",
        expected_chat_output,
        chat_output
    );

    // Check that ToolWriter was used
    // Check that DAG value nodes are created for this tool call
    let value_nodes = tracked_dagops.value_nodes();

    // Create a Vec of explanations from the Vec of value_nodes
    let explanations: Vec<String> = value_nodes
        .iter()
        .map(|node| {
            let (_, explanation, _) = tracked_dagops.parse_value_node(node);
            explanation
        })
        .collect();

    // Check tool input node
    let expected_tool_input_explanation = format!("tool input - {}", name);
    assert!(explanations.iter().any(|exp| exp.contains(&expected_tool_input_explanation)),
           "Expected to find tool input node with explanation containing '{}', found explanations: {:?}", 
           expected_tool_input_explanation, explanations);
}

fn assert_own_dagops(tracked_dagops: &TrackedDagOps, n_tools: usize) {
    // Assert detached from .chat_messages
    let expected_detached = vec![".chat_messages".to_string()];
    assert_eq!(*tracked_dagops.detached(), expected_detached);

    // Assert the workflow is restarted (2 workflows per a tool, plus the restarted one)
    let workflows = tracked_dagops.workflows();
    let expected_workflows = n_tools * 2 + 1;
    assert_eq!(workflows.len(), expected_workflows);
}

//
// "Happy path" style tests
//

// Terminology and differences:
// - "Direct" funcalls: without using "index", using "end_item_if_direct" to finalize
// - "Streaming" funcalls: using "index" to indicate progress

#[test]
fn single_funcall_direct() {
    // Arrange
    let writer = RcWriter::new();
    let tracked_dagops = TrackedDagOps::default();
    let mut funcalls = FunCallsBuilder::new(tracked_dagops.clone());

    // Act
    // Don't call "index"
    funcalls
        .id("call_9cFpsOXfVWMUoDz1yyyP1QXD")
        .unwrap();
    funcalls
        .name("get_user_name")
        .unwrap();
    funcalls
        .arguments_chunk(b"{}")
        .unwrap();
    funcalls
        .end_item_if_direct()
        .unwrap();

    // Assert output
    funcalls.end().unwrap();
    assert_writers(
        &tracked_dagops,
        "call_9cFpsOXfVWMUoDz1yyyP1QXD",
        "get_user_name",
        "{}",
    );
    assert_own_dagops(&tracked_dagops, 1);
}

#[test]
fn several_funcalls_direct() {
    // Arrange
    let writer = RcWriter::new();
    let tracked_dagops = TrackedDagOps::default();
    let mut funcalls = FunCallsBuilder::new(tracked_dagops.clone());

    // First tool call - Don't call "index"
    funcalls
        .id("call_foo")
        .unwrap();
    funcalls
        .name("get_foo")
        .unwrap();
    funcalls
        .arguments_chunk(b"{foo_args}")
        .unwrap();
    funcalls
        .end_item_if_direct()
        .unwrap();

    // Second tool call - Don't call "index"
    funcalls
        .id("call_bar")
        .unwrap();
    funcalls
        .name("get_bar")
        .unwrap();
    funcalls
        .arguments_chunk(b"{bar_args}")
        .unwrap();
    funcalls
        .end_item_if_direct()
        .unwrap();

    // Third tool call - Don't call "index"
    funcalls
        .id("call_baz")
        .unwrap();
    funcalls
        .name("get_baz")
        .unwrap();
    funcalls
        .arguments_chunk(b"{baz_args}")
        .unwrap();
    funcalls
        .end_item_if_direct()
        .unwrap();

    // Assert
    funcalls.end().unwrap();
    assert_writers(
        &tracked_dagops,
        "call_foo",
        "get_foo",
        "{foo_args}",
    );
    assert_writers(
        &tracked_dagops,
        "call_bar",
        "get_bar",
        "{bar_args}",
    );
    assert_writers(
        &tracked_dagops,
        "call_baz",
        "get_baz",
        "{baz_args}",
    );
    assert_own_dagops(&tracked_dagops, 3);
}

#[test]
fn single_element_streaming() {
    // Arrange
    let writer = RcWriter::new();
    let tracked_dagops = TrackedDagOps::default();
    let mut funcalls = FunCallsBuilder::new(tracked_dagops.clone());

    // Act - streaming mode with delta_index
    funcalls
        .index(0)
        .unwrap();
    funcalls
        .id("call_9cFpsOXfVWMUoDz1yyyP1QXD")
        .unwrap();
    funcalls
        .name("get_user_name")
        .unwrap();
    funcalls
        .arguments_chunk(b"{}")
        .unwrap();

    // Assert - streaming should auto-call end_item_if_direct
    funcalls.end().unwrap();
    assert_writers(
        &tracked_dagops,
        "call_9cFpsOXfVWMUoDz1yyyP1QXD",
        "get_user_name",
        "{}",
    );
    assert_own_dagops(&tracked_dagops, 1);
}

#[test]
fn several_elements_streaming() {
    // Arrange
    let writer = RcWriter::new();
    let tracked_dagops = TrackedDagOps::default();
    let mut funcalls = FunCallsBuilder::new(tracked_dagops.clone());

    // Act - streaming mode with delta_index, multiple elements in one round
    funcalls
        .index(0)
        .unwrap();

    funcalls
        .id("call_foo")
        .unwrap();
    funcalls
        .name("get_foo")
        .unwrap();
    funcalls
        .arguments_chunk(b"{foo_args}")
        .unwrap();

    funcalls
        .index(1)
        .unwrap();

    funcalls
        .id("call_bar")
        .unwrap();
    funcalls
        .name("get_bar")
        .unwrap();
    funcalls
        .arguments_chunk(b"{bar_args}")
        .unwrap();

    funcalls
        .index(2)
        .unwrap();

    funcalls
        .id("call_baz")
        .unwrap();
    funcalls
        .name("get_baz")
        .unwrap();
    funcalls
        .arguments_chunk(b"{baz_args}")
        .unwrap();

    // Assert
    funcalls.end().unwrap();
    assert_writers(
        &tracked_dagops,
        "call_foo",
        "get_foo",
        "{foo_args}",
    );
    assert_writers(
        &tracked_dagops,
        "call_bar",
        "get_bar",
        "{bar_args}",
    );
    assert_writers(
        &tracked_dagops,
        "call_baz",
        "get_baz",
        "{baz_args}",
    );
    assert_own_dagops(&tracked_dagops, 3);
}

//
// More detailed tests
//

#[test]
fn index_increment_validation() {
    let writer = RcWriter::new();
    let tracked_dagops = TrackedDagOps::default();
    let mut funcalls = FunCallsBuilder::new(tracked_dagops.clone());

    // First index must be 0
    assert!(funcalls
        .index(0)
        .is_ok());

    // Index can stay the same
    assert!(funcalls
        .index(0)
        .is_ok());

    // Index can increment by 1
    assert!(funcalls
        .index(1)
        .is_ok());

    // Index can stay the same
    assert!(funcalls
        .index(1)
        .is_ok());

    // Index can increment by 1
    assert!(funcalls
        .index(2)
        .is_ok());

    // Index cannot skip
    let result = funcalls.index(4);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("cannot skip values"));

    // Index cannot go backwards (never decreases)
    let result = funcalls.index(1);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("cannot decrease"));
}

#[test]
fn first_index_must_be_zero() {
    let writer = RcWriter::new();
    let tracked_dagops = TrackedDagOps::default();
    let mut funcalls = FunCallsBuilder::new(tracked_dagops.clone());

    // First index must be 0
    let result = funcalls.index(1);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("First tool call index must be 0"));
}

#[test]
fn arguments_span_multiple_deltas() {
    let writer = RcWriter::new();
    let tracked_dagops = TrackedDagOps::default();
    let mut funcalls = FunCallsBuilder::new(tracked_dagops.clone());

    // Enable streaming mode
    assert!(funcalls
        .index(0)
        .is_ok());

    // Set up id and name first so new_item gets called
    funcalls
        .id("call_123")
        .unwrap();
    funcalls
        .name("get_user")
        .unwrap();

    // Arguments can be set multiple times - this should work
    funcalls
        .arguments_chunk(b"{")
        .unwrap();
    funcalls
        .arguments_chunk(b"\"arg\": \"value\"")
        .unwrap();
    funcalls
        .arguments_chunk(b"}")
        .unwrap();

    // End the item (use end() for streaming mode)
    funcalls.end().unwrap();

    // No error should occur - arguments are allowed to span deltas
    assert_writers(
        &tracked_dagops,
        "call_123",
        "get_user",
        "{\"arg\": \"value\"}",
    );
    assert_own_dagops(&tracked_dagops, 1);
}

#[test]
fn test_id_already_given_error_streaming() {
    let writer = RcWriter::new();
    let tracked_dagops = TrackedDagOps::default();
    let mut funcalls = FunCallsBuilder::new(tracked_dagops.clone());

    // First ID should work
    funcalls
        .id("call_123")
        .unwrap();

    // Second ID should error
    let result = funcalls.id("call_456");
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("ID is already given"));
}

#[test]
fn test_name_already_given_error() {
    let writer = RcWriter::new();
    let tracked_dagops = TrackedDagOps::default();
    let mut funcalls = FunCallsBuilder::new(tracked_dagops.clone());

    // First name should work
    funcalls
        .name("get_user")
        .unwrap();

    // Second name should error
    let result = funcalls.name("get_data");
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Name is already given"));
}

#[test]
fn test_id_then_name_calls_new_item() {
    let writer = RcWriter::new();
    let tracked_dagops = TrackedDagOps::default();
    let mut funcalls = FunCallsBuilder::new(tracked_dagops.clone());

    // Set id first, then name
    funcalls
        .id("call_123")
        .unwrap();
    funcalls
        .name("get_user")
        .unwrap();

    // Should have started outputting (partial output expected since we haven't called end_item)
    let output = writer.get_output();
    assert!(output.contains("call_123"));
    assert!(output.contains("get_user"));
}

#[test]
fn test_name_then_id_calls_new_item() {
    let writer = RcWriter::new();
    let tracked_dagops = TrackedDagOps::default();
    let mut funcalls = FunCallsBuilder::new(tracked_dagops.clone());

    // Set name first, then id
    funcalls
        .name("get_user")
        .unwrap();
    funcalls
        .id("call_123")
        .unwrap();

    // Should have started outputting (partial output expected since we haven't called end_item)
    let output = writer.get_output();
    assert!(output.contains("call_123"));
    assert!(output.contains("get_user"));
}

#[test]
fn test_arguments_chunk_without_new_item_stores() {
    let writer = RcWriter::new();
    let tracked_dagops = TrackedDagOps::default();
    let mut funcalls = FunCallsBuilder::new(tracked_dagops.clone());

    // Add arguments without calling new_item first
    funcalls
        .arguments_chunk(b"{\"arg\": \"value\"}")
        .unwrap();

    // Should not have written anything yet
    // Now set id and name to trigger new_item
    funcalls
        .id("call_123")
        .unwrap();
    funcalls
        .name("get_user")
        .unwrap();

    // Now end the item
    funcalls
        .end_item_if_direct()
        .unwrap();

    assert_writers(
        &tracked_dagops,
        "call_123",
        "get_user",
        "{\"arg\": \"value\"}",
    );
}

#[test]
fn test_arguments_chunk_with_new_item_forwards() {
    let writer = RcWriter::new();
    let tracked_dagops = TrackedDagOps::default();
    let mut funcalls = FunCallsBuilder::new(tracked_dagops.clone());

    // Set id and name to trigger new_item
    funcalls
        .id("call_123")
        .unwrap();
    funcalls
        .name("get_user")
        .unwrap();

    // Now add arguments - should forward directly to writer
    funcalls
        .arguments_chunk(b"{\"arg\": \"value\"}")
        .unwrap();
    funcalls
        .end_item_if_direct()
        .unwrap();

    assert_writers(
        &tracked_dagops,
        "call_123",
        "get_user",
        "{\"arg\": \"value\"}",
    );
}

#[test]
fn test_end_item_if_direct_without_new_item_error() {
    let writer = RcWriter::new();
    let tracked_dagops = TrackedDagOps::default();
    let mut funcalls = FunCallsBuilder::new(tracked_dagops.clone());

    // Call end_item_if_direct without new_item should error
    let result = funcalls.end_item_if_direct();
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains(
        "At the end of a 'tool_calls' item, both 'id' and 'name' should be already given"
    ));
}

#[test]
fn test_end_item_if_direct_missing_name_error() {
    let writer = RcWriter::new();
    let tracked_dagops = TrackedDagOps::default();
    let mut funcalls = FunCallsBuilder::new(tracked_dagops.clone());

    // Set only id, but not name
    funcalls
        .id("call_123")
        .unwrap();

    // Call end_item_if_direct without name should error
    let result = funcalls.end_item_if_direct();
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("At the end of a 'tool_calls' item, 'name' should be already given"));
}

#[test]
fn test_end_item_if_direct_missing_id_error() {
    let writer = RcWriter::new();
    let tracked_dagops = TrackedDagOps::default();
    let mut funcalls = FunCallsBuilder::new(tracked_dagops.clone());

    // Set only name, but not id
    funcalls
        .name("test_function")
        .unwrap();

    // Call end_item_if_direct without id should error
    let result = funcalls.end_item_if_direct();
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("At the end of a 'tool_calls' item, 'id' should be already given"));
}

#[test]
fn test_index_increment_calls_end_item_if_not_called() {
    let writer = RcWriter::new();
    let tracked_dagops = TrackedDagOps::default();
    let mut funcalls = FunCallsBuilder::new(tracked_dagops.clone());

    // Start with index 0
    funcalls
        .index(0)
        .unwrap();
    funcalls
        .id("call_123")
        .unwrap();
    funcalls
        .name("get_user")
        .unwrap();
    funcalls
        .arguments_chunk(b"{}")
        .unwrap();

    // Move to index 1 without calling end_item_if_direct - should auto-call it
    funcalls
        .index(1)
        .unwrap();

    // The first item should be completed
    let output = writer.get_output();
    assert!(output.contains("call_123"));
    assert!(output.contains("get_user"));
    assert!(output.contains("{}"));
}

#[test]
fn test_end_calls_end_item_if_not_called() {
    let writer = RcWriter::new();
    let tracked_dagops = TrackedDagOps::default();
    let mut funcalls = FunCallsBuilder::new(tracked_dagops.clone());

    // Set up a function call
    funcalls
        .id("call_123")
        .unwrap();
    funcalls
        .name("get_user")
        .unwrap();
    funcalls
        .arguments_chunk(b"{}")
        .unwrap();

    // Call end without calling end_item_if_direct first
    funcalls.end().unwrap();

    // Should have auto-called end_item_if_direct
    assert_writers(&tracked_dagops, "call_123", "get_user", "{}");
    assert_own_dagops(&tracked_dagops, 1);
}

#[test]
fn test_multiple_arguments_chunks_accumulated() {
    let writer = RcWriter::new();
    let tracked_dagops = TrackedDagOps::default();
    let mut funcalls = FunCallsBuilder::new(tracked_dagops.clone());

    // Add multiple argument chunks before new_item
    funcalls
        .arguments_chunk(b"{")
        .unwrap();
    funcalls
        .arguments_chunk(b"\"key\":")
        .unwrap();
    funcalls
        .arguments_chunk(b"\"value\"")
        .unwrap();
    funcalls
        .arguments_chunk(b"}")
        .unwrap();

    // Set id and name to trigger new_item
    funcalls
        .id("call_123")
        .unwrap();
    funcalls
        .name("get_user")
        .unwrap();
    funcalls
        .end_item_if_direct()
        .unwrap();

    assert_writers(
        &tracked_dagops,
        "call_123",
        "get_user",
        "{\"key\":\"value\"}",
    );
}

#[test]
fn test_end_item_if_direct_ends_item_in_direct_mode() {
    let writer = RcWriter::new();
    let tracked_dagops = TrackedDagOps::default();
    let mut funcalls = FunCallsBuilder::new(tracked_dagops.clone());

    // Direct mode - no call to index()
    funcalls
        .id("call_123")
        .unwrap();
    funcalls
        .name("get_user")
        .unwrap();
    funcalls
        .arguments_chunk(b"{\"arg\": \"value\"}")
        .unwrap();

    // Call end_item_if_direct - should end the item in direct mode
    funcalls
        .end_item_if_direct()
        .unwrap();

    // The item should be completed immediately
    assert_writers(
        &tracked_dagops,
        "call_123",
        "get_user",
        "{\"arg\": \"value\"}",
    );
}

#[test]
fn test_end_item_if_direct_does_not_end_item_in_streaming_mode() {
    let writer = RcWriter::new();
    let tracked_dagops = TrackedDagOps::default();
    let mut funcalls = FunCallsBuilder::new(tracked_dagops.clone());

    // Streaming mode - call index() to enable streaming
    funcalls
        .index(0)
        .unwrap();

    funcalls
        .id("call_123")
        .unwrap();
    funcalls
        .name("get_user")
        .unwrap();
    funcalls
        .arguments_chunk(b"{\"arg\": \"value\"}")
        .unwrap();

    // Call end_item_if_direct - should NOT end the item in streaming mode
    funcalls
        .end_item_if_direct()
        .unwrap();

    // The item should NOT be completed yet - streaming mode doesn't end until index changes or end() is called
    let output = writer.get_output();
    // Should have partial output but not complete line
    assert!(output.contains("call_123"));
    assert!(output.contains("get_user"));
    // But should not have a complete line with newline
    assert!(!output.ends_with("\n"));
}

#[test]
fn test_enforce_end_item_works_in_direct_mode() {
    let writer = RcWriter::new();
    let tracked_dagops = TrackedDagOps::default();
    let mut funcalls = FunCallsBuilder::new(tracked_dagops.clone());

    // Direct mode - no call to index()
    funcalls
        .id("call_123")
        .unwrap();
    funcalls
        .name("get_user")
        .unwrap();
    funcalls
        .arguments_chunk(b"{\"arg\": \"value\"}")
        .unwrap();

    // Call enforce_end_item - should end the item in direct mode
    funcalls
        .enforce_end_item()
        .unwrap();

    // The item should be completed immediately
    assert_writers(
        &tracked_dagops,
        "call_123",
        "get_user",
        "{\"arg\": \"value\"}",
    );
}

#[test]
fn test_enforce_end_item_works_in_streaming_mode() {
    let writer = RcWriter::new();
    let tracked_dagops = TrackedDagOps::default();
    let mut funcalls = FunCallsBuilder::new(tracked_dagops.clone());

    // Streaming mode - call index() to enable streaming
    funcalls
        .index(0)
        .unwrap();

    funcalls
        .id("call_123")
        .unwrap();
    funcalls
        .name("get_user")
        .unwrap();
    funcalls
        .arguments_chunk(b"{\"arg\": \"value\"}")
        .unwrap();

    // Call enforce_end_item - should end the item even in streaming mode
    funcalls
        .enforce_end_item()
        .unwrap();

    // The item should be completed (unlike end_item_if_direct which does nothing in streaming mode)
    assert_writers(
        &tracked_dagops,
        "call_123",
        "get_user",
        "{\"arg\": \"value\"}",
    );
}

#[test]
fn test_enforce_end_item_without_new_item_error() {
    let writer = RcWriter::new();
    let tracked_dagops = TrackedDagOps::default();
    let mut funcalls = FunCallsBuilder::new(tracked_dagops.clone());

    // Call enforce_end_item without new_item should error
    let result = funcalls.enforce_end_item();
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("enforce_end_item called without new_item being called first"));
}
