use std::io::Write;

use actor_runtime_mocked::RcWriter;
use gpt::funcalls_builder::FunCallsBuilder;
use gpt::structure_builder::StructureBuilder;

#[test]
fn basic_pass() {
    // Arrange
    let mut writer = RcWriter::new();
    let mut dag_writer = Vec::new();
    let mut builder = StructureBuilder::new(writer.clone(), dag_writer);

    // Act
    builder.begin_message();
    builder.role("assistant").unwrap();
    builder.begin_text_chunk().unwrap();
    writer.write_all(b"hello").unwrap();
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
    let mut dag_writer = Vec::new();
    let mut builder = StructureBuilder::new(writer.clone(), dag_writer);

    // Act without "builder.role()"
    builder.begin_message();
    builder.begin_text_chunk().unwrap();
    writer.write_all(b"hello").unwrap();
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
    let mut dag_writer = Vec::new();
    let mut builder = StructureBuilder::new(writer.clone(), dag_writer);

    // Act
    builder.begin_message();
    builder.begin_text_chunk().unwrap();
    writer.write_all(b"hello").unwrap();
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
    let mut dag_writer = Vec::new();
    let mut builder = StructureBuilder::new(writer.clone(), dag_writer);

    // Act
    builder.begin_message();
    builder.tool_call_id("call_123").unwrap();
    builder.tool_call_name("get_user_name").unwrap();
    builder.tool_call_arguments_chunk("{}").unwrap();
    builder.tool_call_end_direct().unwrap();
    builder.end_message().unwrap();

    // Assert
    let expected = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"function","id":"call_123","name":"get_user_name"},{"arguments":"{}"}]
"#
    .to_owned();
    assert_eq!(writer.get_output(), expected);
}

#[test]
fn output_streaming_tool_call() {
    // Arrange
    let writer = RcWriter::new();
    let mut dag_writer = Vec::new();
    let mut builder = StructureBuilder::new(writer.clone(), dag_writer);

    // Act
    builder.begin_message();
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

    // Assert
    let expected = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"function","id":"call_123","name":"foo"},{"arguments":"foo args"}]
[{"type":"function","id":"call_456","name":"bar"},{"arguments":"bar args"}]
"#
    .to_owned();
    assert_eq!(writer.get_output(), expected);
}
