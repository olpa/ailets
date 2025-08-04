use std::io::Write;

use actor_runtime_mocked::RcWriter;
use dagops_mock::TrackedDagOps;
use gpt::fcw_dag::FunCallsToDag;
use gpt::structure_builder::StructureBuilder;

pub mod dagops_mock;

#[test]
fn basic_pass() {
    // Arrange
    let mut writer = RcWriter::new();
    let mut tracked_dagops = TrackedDagOps::default();
    let dag_writer = FunCallsToDag::new(&mut tracked_dagops);
    let mut builder = StructureBuilder::new(writer.clone(), dag_writer);

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
    let mut tracked_dagops = TrackedDagOps::default();
    let dag_writer = FunCallsToDag::new(&mut tracked_dagops);
    let mut builder = StructureBuilder::new(writer.clone(), dag_writer);

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
    let mut tracked_dagops = TrackedDagOps::default();
    let dag_writer = FunCallsToDag::new(&mut tracked_dagops);
    let mut builder = StructureBuilder::new(writer.clone(), dag_writer);

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
    let mut tracked_dagops = TrackedDagOps::default();
    let dag_writer = FunCallsToDag::new(&mut tracked_dagops);
    let mut builder = StructureBuilder::new(writer.clone(), dag_writer);

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
    } // Ensure writers are dropped before assertions

    // Assert chat output
    let expected = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"function","id":"call_123","name":"get_user_name"},{"arguments":"{}"}]
"#
    .to_owned();
    assert_eq!(writer.get_output(), expected);

    // Assert DAG operations - should have 2 value nodes (tool input and tool spec)
    assert_eq!(tracked_dagops.value_nodes().len(), 2);

    // Assert tool input value node
    let (_, explain_tool_input, value_tool_input) =
        tracked_dagops.parse_value_node(&tracked_dagops.value_nodes()[0]);
    assert!(explain_tool_input.contains("tool input - get_user_name"));
    assert_eq!(value_tool_input, "{}");

    // Assert tool spec value node
    let (_, explain_tool_spec, value_tool_spec) =
        tracked_dagops.parse_value_node(&tracked_dagops.value_nodes()[1]);
    assert!(explain_tool_spec.contains("tool call spec - get_user_name"));
    let expected_tool_spec =
        r#"[{"type":"function","id":"call_123","name":"get_user_name"},{"arguments":"{}"}]"#;
    assert_eq!(value_tool_spec, expected_tool_spec);
}

#[test]
fn output_streaming_tool_call() {
    // Arrange
    let writer = RcWriter::new();
    let mut tracked_dagops = TrackedDagOps::default();
    let dag_writer = FunCallsToDag::new(&mut tracked_dagops);
    let mut builder = StructureBuilder::new(writer.clone(), dag_writer);

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

    // Assert chat output
    let expected = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"function","id":"call_123","name":"foo"},{"arguments":"foo args"}]
[{"type":"function","id":"call_456","name":"bar"},{"arguments":"bar args"}]
"#
    .to_owned();
    assert_eq!(writer.get_output(), expected);

    // Assert DAG operations - should have 4 value nodes (tool input and tool spec for each of 2 tools)
    assert_eq!(tracked_dagops.value_nodes().len(), 4);

    // Assert first tool (foo) input value node
    let (_, explain_tool_input1, value_tool_input1) =
        tracked_dagops.parse_value_node(&tracked_dagops.value_nodes()[0]);
    assert!(explain_tool_input1.contains("tool input - foo"));
    assert_eq!(value_tool_input1, "foo args");

    // Assert first tool (foo) spec value node
    let (_, explain_tool_spec1, value_tool_spec1) =
        tracked_dagops.parse_value_node(&tracked_dagops.value_nodes()[1]);
    assert!(explain_tool_spec1.contains("tool call spec - foo"));
    let expected_tool_spec1 =
        r#"[{"type":"function","id":"call_123","name":"foo"},{"arguments":"foo args"}]"#;
    assert_eq!(value_tool_spec1, expected_tool_spec1);

    // Assert second tool (bar) input value node
    let (_, explain_tool_input2, value_tool_input2) =
        tracked_dagops.parse_value_node(&tracked_dagops.value_nodes()[2]);
    assert!(explain_tool_input2.contains("tool input - bar"));
    assert_eq!(value_tool_input2, "bar args");

    // Assert second tool (bar) spec value node
    let (_, explain_tool_spec2, value_tool_spec2) =
        tracked_dagops.parse_value_node(&tracked_dagops.value_nodes()[3]);
    assert!(explain_tool_spec2.contains("tool call spec - bar"));
    let expected_tool_spec2 =
        r#"[{"type":"function","id":"call_456","name":"bar"},{"arguments":"bar args"}]"#;
    assert_eq!(value_tool_spec2, expected_tool_spec2);
}

#[test]
fn autoclose_text_on_end_message() {
    // Arrange
    let mut writer = RcWriter::new();
    let mut tracked_dagops = TrackedDagOps::default();
    let dag_writer = FunCallsToDag::new(&mut tracked_dagops);
    let mut builder = StructureBuilder::new(writer.clone(), dag_writer);

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
    let mut tracked_dagops = TrackedDagOps::default();
    let dag_writer = FunCallsToDag::new(&mut tracked_dagops);
    let mut builder = StructureBuilder::new(writer.clone(), dag_writer);

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
fn autoclose_toolcall_on_end_message() {
    // Arrange
    let writer = RcWriter::new();
    let mut tracked_dagops = TrackedDagOps::default();
    let dag_writer = FunCallsToDag::new(&mut tracked_dagops);
    let mut builder = StructureBuilder::new(writer.clone(), dag_writer);

    // Act - start tool call but don't explicitly end it
    builder.begin_message().unwrap();
    builder.role("assistant").unwrap();
    builder.tool_call_id("call_123").unwrap();
    builder.tool_call_name("get_user").unwrap();
    {
        let mut args_writer = builder.get_arguments_chunk_writer();
        args_writer.write_all(b"{}").unwrap();
    }
    // Intentionally NOT calling tool_call_end_if_direct() here
    builder.end_message().unwrap(); // Should auto-close tool call

    // Assert - should have auto-closed the tool call
    let expected = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"function","id":"call_123","name":"get_user"},{"arguments":"{}"}]
"#
    .to_owned();
    assert_eq!(writer.get_output(), expected);
}

#[test]
fn autoclose_toolcall_on_new_message_and_role() {
    // Arrange
    let writer = RcWriter::new();
    let mut tracked_dagops = TrackedDagOps::default();
    let dag_writer = FunCallsToDag::new(&mut tracked_dagops);
    let mut builder = StructureBuilder::new(writer.clone(), dag_writer);

    // Act - start tool call in first message, then start new message without closing tool call
    builder.begin_message().unwrap();
    builder.role("assistant").unwrap();
    builder.tool_call_id("call_id_foo").unwrap();
    builder.tool_call_name("get_foo").unwrap();
    {
        let mut args_writer = builder.get_arguments_chunk_writer();
        args_writer.write_all(b"fooargs").unwrap();
    }
    // Intentionally NOT calling tool_call_end_if_direct() here

    builder.begin_message().unwrap(); // Should auto-close tool call
    builder.tool_call_id("call_id_bar").unwrap();
    builder.tool_call_name("get_bar").unwrap();
    {
        let mut args_writer = builder.get_arguments_chunk_writer();
        args_writer.write_all(b"barargs").unwrap();
    }

    builder.role("user").unwrap(); // Should auto-close tool call
    builder.end_message().unwrap();

    // Assert - should have auto-closed tool calls at begin_message and role
    let expected = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"function","id":"call_id_foo","name":"get_foo"},{"arguments":"fooargs"}]
[{"type":"function","id":"call_id_bar","name":"get_bar"},{"arguments":"barargs"}]
[{"type":"ctl"},{"role":"user"}]
"#
    .to_owned();
    assert_eq!(writer.get_output(), expected);
}

#[test]
fn autoclose_mix_text_and_toolcall() {
    // Arrange
    let mut writer = RcWriter::new();
    let mut tracked_dagops = TrackedDagOps::default();
    let dag_writer = FunCallsToDag::new(&mut tracked_dagops);
    let mut builder = StructureBuilder::new(writer.clone(), dag_writer);

    // Arrange - autoclose text because of tool_id
    builder.begin_message().unwrap();
    builder.begin_text_chunk().unwrap();
    writer.write_all(b"I'll help you").unwrap();

    //
    // Act - autoclose text because of tool_id
    //
    builder.tool_call_id("call_123").unwrap(); // Should auto-close text

    // Assert: text is auto-closed
    let expected = r#"[{"type":"text"},{"text":"I'll help you"}]
"#;
    assert_eq!(writer.get_output(), expected);

    // Complete the tool to avoid errors
    builder.tool_call_name("get_user").unwrap();

    // Act: autoclose tool because of text
    builder.begin_text_chunk().unwrap();

    // Assert: autoclose tool because of text
    let expected = r#"[{"type":"text"},{"text":"I'll help you"}]
[{"type":"function","id":"call_123","name":"get_user"},{"arguments":""}]
[{"type":"text"},{"text":""#;
    assert_eq!(writer.get_output(), expected);

    //
    // Act: autoclose text because of tool_index
    //
    writer.write_all(b"Before tool index").unwrap();
    builder.tool_call_index(0).unwrap(); // Should auto-close text

    // Assert: text is auto-closed
    let expected = r#"[{"type":"text"},{"text":"I'll help you"}]
[{"type":"function","id":"call_123","name":"get_user"},{"arguments":""}]
[{"type":"text"},{"text":"Before tool index"}]
"#;
    assert_eq!(writer.get_output(), expected);

    // Complete the tool to avoid errors
    builder.tool_call_id("call_456").unwrap();
    builder.tool_call_name("get_data").unwrap();

    // Act: autoclose tool because of text
    builder.begin_text_chunk().unwrap();

    // Assert: autoclose tool because of text
    let expected = r#"[{"type":"text"},{"text":"I'll help you"}]
[{"type":"function","id":"call_123","name":"get_user"},{"arguments":""}]
[{"type":"text"},{"text":"Before tool index"}]
[{"type":"function","id":"call_456","name":"get_data"},{"arguments":""}]
[{"type":"text"},{"text":""#;
    assert_eq!(writer.get_output(), expected);

    //
    // Act: autoclose text because of tool_name
    //
    writer.write_all(b"Before tool name").unwrap();
    builder.tool_call_name("another_tool").unwrap(); // Should auto-close text

    // Assert: text is auto-closed
    let expected = r#"[{"type":"text"},{"text":"I'll help you"}]
[{"type":"function","id":"call_123","name":"get_user"},{"arguments":""}]
[{"type":"text"},{"text":"Before tool index"}]
[{"type":"function","id":"call_456","name":"get_data"},{"arguments":""}]
[{"type":"text"},{"text":"Before tool name"}]
"#;
    assert_eq!(writer.get_output(), expected);

    // Complete the tool to avoid errors
    builder.tool_call_id("call_789").unwrap();

    // Act: autoclose tool because of text
    builder.begin_text_chunk().unwrap();

    // Assert: autoclose tool because of text
    let expected = r#"[{"type":"text"},{"text":"I'll help you"}]
[{"type":"function","id":"call_123","name":"get_user"},{"arguments":""}]
[{"type":"text"},{"text":"Before tool index"}]
[{"type":"function","id":"call_456","name":"get_data"},{"arguments":""}]
[{"type":"text"},{"text":"Before tool name"}]
[{"type":"function","id":"call_789","name":"another_tool"},{"arguments":""}]
[{"type":"text"},{"text":""#;
    assert_eq!(writer.get_output(), expected);

    // Act: autoclose text because of tool_arguments_chunk
    writer.write_all(b"Before args").unwrap();
    {
        let mut args_writer = builder.get_arguments_chunk_writer(); // Should auto-close text
        args_writer.write_all(b"some_args").unwrap();
    }

    // Assert: text is auto-closed
    let expected = r#"[{"type":"text"},{"text":"I'll help you"}]
[{"type":"function","id":"call_123","name":"get_user"},{"arguments":""}]
[{"type":"text"},{"text":"Before tool index"}]
[{"type":"function","id":"call_456","name":"get_data"},{"arguments":""}]
[{"type":"text"},{"text":"Before tool name"}]
[{"type":"function","id":"call_789","name":"another_tool"},{"arguments":""}]
[{"type":"text"},{"text":"Before args"}]
"#;
    assert_eq!(writer.get_output(), expected);

    // Complete the tool to avoid errors
    builder.tool_call_name("final_tool").unwrap();
    builder.tool_call_id("call_final").unwrap();

    // Act: autoclose tool because of text
    builder.begin_text_chunk().unwrap();
    writer.write_all(b"After args").unwrap();

    // Assert: autoclose tool because of text
    builder.end_message().unwrap();
    let expected = r#"[{"type":"text"},{"text":"I'll help you"}]
[{"type":"function","id":"call_123","name":"get_user"},{"arguments":""}]
[{"type":"text"},{"text":"Before tool index"}]
[{"type":"function","id":"call_456","name":"get_data"},{"arguments":""}]
[{"type":"text"},{"text":"Before tool name"}]
[{"type":"function","id":"call_789","name":"another_tool"},{"arguments":""}]
[{"type":"text"},{"text":"Before args"}]
[{"type":"function","id":"call_final","name":"final_tool"},{"arguments":"some_args"}]
[{"type":"text"},{"text":"After args"}]
"#;
    assert_eq!(writer.get_output(), expected);
}

#[test]
fn tool_call_without_arguments_chunk_has_empty_arguments() {
    // Arrange
    let writer = RcWriter::new();
    let mut tracked_dagops = TrackedDagOps::default();
    let dag_writer = FunCallsToDag::new(&mut tracked_dagops);
    let mut builder = StructureBuilder::new(writer.clone(), dag_writer);

    // Act - create tool call but never call tool_arguments_chunk
    builder.begin_message().unwrap();
    builder.role("assistant").unwrap();
    builder.tool_call_id("call_123").unwrap();
    builder.tool_call_name("get_user_name").unwrap();
    // Intentionally NOT calling get_arguments_chunk_writer() or writing any arguments
    builder.tool_call_end_if_direct().unwrap();
    builder.end_message().unwrap();

    // Assert - output should contain "arguments" field with empty string value
    let expected = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"function","id":"call_123","name":"get_user_name"},{"arguments":""}]
"#
    .to_owned();
    assert_eq!(writer.get_output(), expected);

    // Assert DAG operations - should have 2 value nodes (tool input and tool spec)
    assert_eq!(tracked_dagops.value_nodes().len(), 2);

    // Assert tool input value node has empty string
    let (_, explain_tool_input, value_tool_input) =
        tracked_dagops.parse_value_node(&tracked_dagops.value_nodes()[0]);
    assert!(explain_tool_input.contains("tool input - get_user_name"));
    assert_eq!(value_tool_input, "");

    // Assert tool spec value node has empty arguments
    let (_, explain_tool_spec, value_tool_spec) =
        tracked_dagops.parse_value_node(&tracked_dagops.value_nodes()[1]);
    assert!(explain_tool_spec.contains("tool call spec - get_user_name"));
    let expected_tool_spec =
        r#"[{"type":"function","id":"call_123","name":"get_user_name"},{"arguments":""}]"#;
    assert_eq!(value_tool_spec, expected_tool_spec);
}

#[test]
fn begin_text_chunk_no_prefix_when_already_open() {
    // Arrange
    let mut writer = RcWriter::new();
    let mut tracked_dagops = TrackedDagOps::default();
    let dag_writer = FunCallsToDag::new(&mut tracked_dagops);
    let mut builder = StructureBuilder::new(writer.clone(), dag_writer);

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
