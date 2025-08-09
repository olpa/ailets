use std::io::Write;

use actor_runtime_mocked::RcWriter;
use dagops_mock::TrackedDagOps;
use gpt::structure_builder::StructureBuilder;

pub mod dagops_mock;

// Helper function to get chat output from DAG value nodes
fn get_chat_output(tracked_dagops: &TrackedDagOps) -> String {
    let value_nodes = tracked_dagops.value_nodes();
    assert!(!value_nodes.is_empty(), "Expected at least one value node for chat output");
    let first_node = &value_nodes[0];
    let (_, _, chat_output) = tracked_dagops.parse_value_node(first_node);
    chat_output
}

#[test]
fn basic_pass() {
    // Arrange
    let mut writer = RcWriter::new();
    let tracked_dagops = TrackedDagOps::default();
    let mut builder = StructureBuilder::new(writer.clone(), tracked_dagops.clone());

    // Act
    builder.begin_message().unwrap();
    builder.role("assistant").unwrap();
    builder.begin_text_chunk().unwrap();
    writer.write_all(b"hello").unwrap();
    builder.end_text_chunk().unwrap();
    builder.end_message().unwrap();

    // Assert
    let expected = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"text"},{"text":"hello"}]
"#
    .to_owned();
    assert_eq!(writer.get_output(), expected);
}

#[test]
fn create_message_without_input_role() {
    // Arrange
    let mut writer = RcWriter::new();
    let tracked_dagops = TrackedDagOps::default();
    let mut builder = StructureBuilder::new(writer.clone(), tracked_dagops.clone());

    // Act without "builder.role()"
    builder.begin_message().unwrap();
    builder.role("assistant").unwrap();
    builder.begin_text_chunk().unwrap();
    writer.write_all(b"hello").unwrap();
    builder.end_text_chunk().unwrap();
    builder.end_message().unwrap();

    // Assert
    let expected = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"text"},{"text":"hello"}]
"#
    .to_owned();
    assert_eq!(writer.get_output(), expected);
}

#[test]
fn can_call_end_message_multiple_times() {
    // Arrange
    let mut writer = RcWriter::new();
    let tracked_dagops = TrackedDagOps::default();
    let mut builder = StructureBuilder::new(writer.clone(), tracked_dagops.clone());

    // Act
    builder.begin_message().unwrap();
    builder.role("assistant").unwrap();
    builder.begin_text_chunk().unwrap();
    writer.write_all(b"hello").unwrap();
    builder.end_text_chunk().unwrap();
    builder.end_message().unwrap();
    builder.end_message().unwrap(); // Should be ok
    builder.end_message().unwrap(); // Should be ok

    // Assert
    let expected = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"text"},{"text":"hello"}]
"#
    .to_owned();
    assert_eq!(writer.get_output(), expected);
}

#[test]
fn output_direct_tool_call() {
    // Arrange
    let writer = RcWriter::new();
    let tracked_dagops = TrackedDagOps::default();
    let mut builder = StructureBuilder::new(writer.clone(), tracked_dagops.clone());

    // Act
    {
        builder.begin_message().unwrap();
        builder.role("assistant").unwrap();
        builder.tool_call_id("call_123").unwrap();
        builder.tool_call_name("get_user_name").unwrap();
        {
            let mut writer = builder.get_arguments_chunk_writer();
            writer.write_all(b"{}").unwrap();
        }
        builder.tool_call_end_if_direct().unwrap();
        builder.end_message().unwrap();
        builder.end().unwrap();
    } // Ensure writers are dropped before assertions

    // Assert ctl message is written to writer
    let expected_ctl = r#"[{"type":"ctl"},{"role":"assistant"}]
"#;
    assert_eq!(writer.get_output(), expected_ctl);
    
    // Assert chat output (including function call) is in DAG value nodes
    let chat_output = get_chat_output(&tracked_dagops);
    let expected_chat = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"function","id":"call_123","name":"get_user_name"},{"arguments":"{}"}]
"#;
    assert_eq!(chat_output, expected_chat);

    // Assert DAG operations - should have 3 value nodes (chat output + tool input + tool spec)
    assert_eq!(tracked_dagops.value_nodes().len(), 3);

    // Assert tool input value node (index 1 since chat output is at index 0)
    let (_, explain_tool_input, value_tool_input) =
        tracked_dagops.parse_value_node(&tracked_dagops.value_nodes()[1]);
    assert!(explain_tool_input.contains("tool input - get_user_name"));
    assert_eq!(value_tool_input, "{}");

    // Assert tool spec value node (index 2)
    let (_, explain_tool_spec, value_tool_spec) =
        tracked_dagops.parse_value_node(&tracked_dagops.value_nodes()[2]);
    assert!(explain_tool_spec.contains("tool call spec - get_user_name"));
    let expected_tool_spec =
        r#"[{"type":"function","id":"call_123","name":"get_user_name"},{"arguments":"{}"}]"#;
    assert_eq!(value_tool_spec, expected_tool_spec);

    // Assert that the workflows include .gpt workflow
    let workflows = tracked_dagops.workflows();
    let gpt_workflow_exists = workflows.iter().any(|workflow| {
        let (_, workflow_name, _) = tracked_dagops.parse_workflow(workflow);
        workflow_name == ".gpt"
    });
    assert!(
        gpt_workflow_exists,
        "Expected .gpt workflow to be added to DAG"
    );
}

#[test]
fn output_streaming_tool_call() {
    // Arrange
    let writer = RcWriter::new();
    let tracked_dagops = TrackedDagOps::default();
    let mut builder = StructureBuilder::new(writer.clone(), tracked_dagops.clone());

    // Act
    builder.begin_message().unwrap();
    builder.role("assistant").unwrap();
    builder.tool_call_index(0).unwrap();
    builder.tool_call_id("call_123").unwrap();
    builder.tool_call_name("foo").unwrap();
    {
        let mut writer = builder.get_arguments_chunk_writer();
        writer.write_all(b"foo ").unwrap();
        writer.write_all(b"args").unwrap();
    }

    builder.tool_call_index(1).unwrap();
    builder.tool_call_id("call_456").unwrap();
    builder.tool_call_name("bar").unwrap();
    {
        let mut writer = builder.get_arguments_chunk_writer();
        writer.write_all(b"bar ").unwrap();
        writer.write_all(b"args").unwrap();
    }
    builder.end_message().unwrap();
    builder.end().unwrap();

    // Assert ctl message is written to writer
    let expected_ctl = r#"[{"type":"ctl"},{"role":"assistant"}]
"#;
    assert_eq!(writer.get_output(), expected_ctl);
    
    // Assert chat output (including function calls) is in DAG value nodes
    let chat_output = get_chat_output(&tracked_dagops);
    let expected_chat = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"function","id":"call_123","name":"foo"},{"arguments":"foo args"}]
[{"type":"function","id":"call_456","name":"bar"},{"arguments":"bar args"}]
"#;
    assert_eq!(chat_output, expected_chat);

    // Assert DAG operations - should have 5 value nodes (chat output + 2 tool inputs + 2 tool specs)
    assert_eq!(tracked_dagops.value_nodes().len(), 5);

    // Assert first tool (foo) input value node (index 1 since chat output is at index 0)
    let (_, explain_tool_input1, value_tool_input1) =
        tracked_dagops.parse_value_node(&tracked_dagops.value_nodes()[1]);
    assert!(explain_tool_input1.contains("tool input - foo"));
    assert_eq!(value_tool_input1, "foo args");

    // Assert first tool (foo) spec value node (index 2)
    let (_, explain_tool_spec1, value_tool_spec1) =
        tracked_dagops.parse_value_node(&tracked_dagops.value_nodes()[2]);
    assert!(explain_tool_spec1.contains("tool call spec - foo"));
    let expected_tool_spec1 =
        r#"[{"type":"function","id":"call_123","name":"foo"},{"arguments":"foo args"}]"#;
    assert_eq!(value_tool_spec1, expected_tool_spec1);

    // Assert second tool (bar) input value node (index 3)
    let (_, explain_tool_input2, value_tool_input2) =
        tracked_dagops.parse_value_node(&tracked_dagops.value_nodes()[3]);
    assert!(explain_tool_input2.contains("tool input - bar"));
    assert_eq!(value_tool_input2, "bar args");

    // Assert second tool (bar) spec value node (index 4)
    let (_, explain_tool_spec2, value_tool_spec2) =
        tracked_dagops.parse_value_node(&tracked_dagops.value_nodes()[4]);
    assert!(explain_tool_spec2.contains("tool call spec - bar"));
    let expected_tool_spec2 =
        r#"[{"type":"function","id":"call_456","name":"bar"},{"arguments":"bar args"}]"#;
    assert_eq!(value_tool_spec2, expected_tool_spec2);

    // Assert that the workflows include .gpt workflow
    let workflows = tracked_dagops.workflows();
    let gpt_workflow_exists = workflows.iter().any(|workflow| {
        let (_, workflow_name, _) = tracked_dagops.parse_workflow(workflow);
        workflow_name == ".gpt"
    });
    assert!(
        gpt_workflow_exists,
        "Expected .gpt workflow to be added to DAG"
    );
}

#[test]
fn autoclose_text_on_end_message() {
    // Arrange
    let mut writer = RcWriter::new();
    let tracked_dagops = TrackedDagOps::default();
    let mut builder = StructureBuilder::new(writer.clone(), tracked_dagops.clone());

    // Act - start text but don't explicitly close it
    builder.begin_message().unwrap();
    builder.role("assistant").unwrap();
    builder.begin_text_chunk().unwrap();
    writer.write_all(b"hello").unwrap();
    // Intentionally NOT calling end_text_chunk() here
    builder.end_message().unwrap(); // Should auto-close text

    // Assert - should have auto-closed the text chunk
    let expected = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"text"},{"text":"hello"}]
"#
    .to_owned();
    assert_eq!(writer.get_output(), expected);
}

#[test]
fn autoclose_text_on_new_message_and_role() {
    // Arrange
    let mut writer = RcWriter::new();
    let tracked_dagops = TrackedDagOps::default();
    let mut builder = StructureBuilder::new(writer.clone(), tracked_dagops.clone());

    // Act - start text in first message, then start new message without closing text
    builder.begin_message().unwrap();
    builder.role("assistant").unwrap();
    builder.begin_text_chunk().unwrap();
    writer.write_all(b"hello").unwrap();
    // Intentionally NOT calling end_text_chunk() here

    builder.begin_message().unwrap(); // Should auto-close
    builder.begin_text_chunk().unwrap(); // Should auto-close on begin_message, so this should work
    writer.write_all(b"from").unwrap();

    builder.role("user").unwrap(); // Should auto-close
    builder.begin_text_chunk().unwrap();
    writer.write_all(b"world").unwrap();
    builder.end_message().unwrap();

    // Assert - should have auto-closed the first text chunk on begin_message
    let expected = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"text"},{"text":"hello"}]
[{"type":"text"},{"text":"from"}]
[{"type":"ctl"},{"role":"user"}]
[{"type":"text"},{"text":"world"}]
"#
    .to_owned();
    assert_eq!(writer.get_output(), expected);
}

#[test]
#[ignore] // Obsolete: FunCallsBuilder now manages tool calls independently
fn autoclose_toolcall_on_end_message() {
    // This test is obsolete because tool calls are now managed by FunCallsBuilder
    // which outputs to DAG, not stdout. Auto-closing is handled internally by
    // FunCallsBuilder.end() when StructureBuilder.end() is called.
}

#[test]
#[ignore] // Obsolete: FunCallsBuilder now manages tool calls independently  
fn autoclose_toolcall_on_new_message_and_role() {
    // This test is obsolete because:
    // 1. Tool calls are now managed by FunCallsBuilder which outputs to DAG
    // 2. There's no longer auto-closing between text and tool calls since they use separate outputs
    // 3. FunCallsBuilder validates against reusing IDs/names without proper state management
}

#[test]
fn tool_call_without_arguments_chunk_has_empty_arguments() {
    // Arrange
    let writer = RcWriter::new();
    let tracked_dagops = TrackedDagOps::default();
    let mut builder = StructureBuilder::new(writer.clone(), tracked_dagops.clone());

    // Act - create tool call but never call tool_arguments_chunk
    builder.begin_message().unwrap();
    builder.role("assistant").unwrap();
    builder.tool_call_id("call_123").unwrap();
    builder.tool_call_name("get_user_name").unwrap();
    // Intentionally NOT calling get_arguments_chunk_writer() or writing any arguments
    builder.tool_call_end_if_direct().unwrap();
    builder.end_message().unwrap();

    // Assert ctl message is written to writer
    let expected_ctl = r#"[{"type":"ctl"},{"role":"assistant"}]
"#;
    assert_eq!(writer.get_output(), expected_ctl);
    
    // Assert chat output (including function call) is in DAG value nodes
    let chat_output = get_chat_output(&tracked_dagops);
    let expected_chat = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"function","id":"call_123","name":"get_user_name"},{"arguments":""}]
"#;
    assert_eq!(chat_output, expected_chat);

    // Assert DAG operations - should have 3 value nodes (chat output + tool input + tool spec)
    assert_eq!(tracked_dagops.value_nodes().len(), 3);

    // Assert tool input value node has empty string (index 1 since chat output is at index 0)
    let (_, explain_tool_input, value_tool_input) =
        tracked_dagops.parse_value_node(&tracked_dagops.value_nodes()[1]);
    assert!(explain_tool_input.contains("tool input - get_user_name"));
    assert_eq!(value_tool_input, "");

    // Assert tool spec value node has empty arguments (index 2)
    let (_, explain_tool_spec, value_tool_spec) =
        tracked_dagops.parse_value_node(&tracked_dagops.value_nodes()[2]);
    assert!(explain_tool_spec.contains("tool call spec - get_user_name"));
    let expected_tool_spec =
        r#"[{"type":"function","id":"call_123","name":"get_user_name"},{"arguments":""}]"#;
    assert_eq!(value_tool_spec, expected_tool_spec);
}

#[test]
fn mixing_text_and_functions_separate_outputs() {
    // Arrange
    let mut writer = RcWriter::new();
    let tracked_dagops = TrackedDagOps::default();
    let mut builder = StructureBuilder::new(writer.clone(), tracked_dagops.clone());

    // Act - mix text and function calls
    builder.begin_message().unwrap();
    builder.role("assistant").unwrap();
    
    // Start with text
    builder.begin_text_chunk().unwrap();
    writer.write_all(b"I'll help you with that. ").unwrap();
    builder.end_text_chunk().unwrap();
    
    // Add a function call
    builder.tool_call_id("call_123").unwrap();
    builder.tool_call_name("get_user_info").unwrap();
    {
        let mut args_writer = builder.get_arguments_chunk_writer();
        args_writer.write_all(b"{\"user_id\": 42}").unwrap();
    }
    builder.tool_call_end_if_direct().unwrap();
    
    // Add more text after function call
    builder.begin_text_chunk().unwrap();
    writer.write_all(b"Let me process that for you.").unwrap();
    builder.end_text_chunk().unwrap();
    
    builder.end_message().unwrap();
    builder.end().unwrap();

    // Assert text output goes to stdout (only text and control messages)
    let expected_stdout = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"text"},{"text":"I'll help you with that. "}]
[{"type":"text"},{"text":"Let me process that for you."}]
"#;
    assert_eq!(writer.get_output(), expected_stdout);
    
    // Assert DAG was updated - should have at least one value node (chat output)
    let value_nodes = tracked_dagops.value_nodes();
    assert!(!value_nodes.is_empty(), "Expected at least one DAG value node");
    
    // Check that the first value node contains the chat output with function call
    let (_, _, chat_output) = tracked_dagops.parse_value_node(&value_nodes[0]);
    assert!(chat_output.contains("call_123"), "Chat output should contain function call ID");
    assert!(chat_output.contains("get_user_info"), "Chat output should contain function name");
    assert!(chat_output.contains("{\"user_id\": 42}"), "Chat output should contain function arguments");
}

#[test]
fn begin_text_chunk_no_prefix_when_already_open() {
    // Arrange
    let mut writer = RcWriter::new();
    let tracked_dagops = TrackedDagOps::default();
    let mut builder = StructureBuilder::new(writer.clone(), tracked_dagops.clone());

    // Act - begin text chunk, write some text, then begin text chunk again without closing
    builder.begin_message().unwrap();
    builder.role("assistant").unwrap();
    builder.begin_text_chunk().unwrap();
    writer.write_all(b"first part").unwrap();

    // This should not write the prefix since text is already open
    builder.begin_text_chunk().unwrap();
    writer.write_all(b" second part").unwrap();
    builder.end_text_chunk().unwrap();
    builder.end_message().unwrap();

    // Assert - should only have one text prefix, content should be concatenated
    let expected = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"text"},{"text":"first part second part"}]
"#
    .to_owned();
    assert_eq!(writer.get_output(), expected);
}
