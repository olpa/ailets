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
        builder.tool_call_arguments_chunk("{}").unwrap();
        builder.tool_call_end_direct().unwrap();
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
    builder.tool_call_arguments_chunk("foo ").unwrap();
    builder.tool_call_arguments_chunk("args").unwrap();

    builder.tool_call_index(1).unwrap();
    builder.tool_call_id("call_456").unwrap();
    builder.tool_call_name("bar").unwrap();
    builder.tool_call_arguments_chunk("bar ").unwrap();
    builder.tool_call_arguments_chunk("args").unwrap();
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
