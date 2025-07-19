use std::io::Write;

use actor_runtime_mocked::RcWriter;
use gpt::structure_builder::StructureBuilder;
use gpt::funcalls::ContentItemFunction;

#[test]
fn basic_pass() {
    // Arrange
    let mut writer = RcWriter::new();
    let mut builder = StructureBuilder::new(writer.clone());

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
    let mut builder = StructureBuilder::new(writer.clone());

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
    let mut builder = StructureBuilder::new(writer.clone());

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
fn output_tool_call() {
    // Arrange
    let writer = RcWriter::new();
    let mut builder = StructureBuilder::new(writer.clone());
    let tool_call = ContentItemFunction::new(
        "call_123",
        "get_user_name", 
        "{}"
    );

    // Act
    builder.begin_message();
    builder.output_tool_call(&tool_call).unwrap();
    builder.end_message().unwrap();

    // Assert
    let expected = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"tool_call"},{"id":"call_123","function_name":"get_user_name","function_arguments":"{}"}]
"#.to_owned();
    assert_eq!(writer.get_output(), expected);
}

#[test]
fn inject_tool_calls() {
    // Arrange
    let writer = RcWriter::new();
    let mut builder = StructureBuilder::new(writer.clone());
    
    // Add tool calls to the builder's funcalls
    let funcalls = builder.get_funcalls_mut();
    funcalls.delta_id("call_123");
    funcalls.delta_function_name("get_user_name");
    funcalls.delta_function_arguments("{}");
    funcalls.end_current();

    // Act
    builder.begin_message();
    builder.inject_tool_calls().unwrap();
    builder.end_message().unwrap();

    // Assert
    let expected = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"tool_call"},{"id":"call_123","function_name":"get_user_name","function_arguments":"{}"}]
"#.to_owned();
    assert_eq!(writer.get_output(), expected);
}

#[test]
fn streaming_tool_call_completion() {
    // Arrange
    let writer = RcWriter::new();
    let mut builder = StructureBuilder::new(writer.clone());
    
    // Act - simulate streaming tool call input exactly like the real handlers would
    builder.begin_message();
    
    // First delta: index 0, id, type, name, empty arguments (like handlers do)
    builder.on_tool_call_index(0).unwrap(); // This enables streaming and calls delta_index
    builder.get_funcalls_mut().delta_id("call_123");
    builder.get_funcalls_mut().delta_function_name("get_user_name");
    builder.get_funcalls_mut().delta_function_arguments(""); // Empty at first
    builder.on_tool_call_field_update().unwrap();
    
    // Second delta: complete the arguments  
    builder.get_funcalls_mut().delta_function_arguments("{}");
    builder.on_tool_call_field_update().unwrap();
    
    // End current to finalize the streaming tool call
    builder.get_funcalls_mut().end_current();
    builder.on_tool_call_field_update().unwrap();
    
    builder.end_message().unwrap();

    // Assert - tool call should be output when complete
    let expected = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"tool_call"},{"id":"call_123","function_name":"get_user_name","function_arguments":"{}"}]
"#.to_owned();
    assert_eq!(writer.get_output(), expected);
}

#[test]
fn streaming_mode_not_enabled_without_index() {
    // Arrange
    let writer = RcWriter::new();
    let mut builder = StructureBuilder::new(writer.clone());
    
    // Act - add tool call data without using streaming index
    builder.begin_message();
    builder.get_funcalls_mut().delta_id("call_123");
    builder.get_funcalls_mut().delta_function_name("get_user_name");
    builder.get_funcalls_mut().delta_function_arguments("{}");
    builder.get_funcalls_mut().end_current();
    builder.on_tool_call_field_update().unwrap(); // Should not stream
    builder.inject_tool_calls().unwrap(); // Batch mode
    builder.end_message().unwrap();

    // Assert - should work in batch mode
    let expected = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"tool_call"},{"id":"call_123","function_name":"get_user_name","function_arguments":"{}"}]
"#.to_owned();
    assert_eq!(writer.get_output(), expected);
}