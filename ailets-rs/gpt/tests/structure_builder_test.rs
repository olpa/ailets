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
    
    // Act - test batch mode completion detection 
    builder.begin_message();
    
    // Add complete tool call data and use batch processing
    builder.get_funcalls_mut().delta_id("call_123");
    builder.get_funcalls_mut().delta_function_name("get_user_name");
    builder.get_funcalls_mut().delta_function_arguments("{}");
    builder.get_funcalls_mut().end_current();
    
    // Use batch mode to inject all tool calls at once
    builder.inject_tool_calls().unwrap();
    builder.end_message().unwrap();

    // Assert - tool call should be output
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

#[test] 
fn streaming_arguments_chunking() {
    use scan_json::RJiter;
    use std::io::Cursor;
    
    // Arrange
    let writer = RcWriter::new();
    let mut builder = StructureBuilder::new(writer.clone());
    
    // Act - simulate incremental arguments streaming using write_long_bytes
    builder.begin_message();
    builder.enable_streaming_mode();
    
    // Start with a tool call having id and name
    let tool_call = gpt::funcalls::ContentItemFunction::new("call_123", "get_user_name", "");
    builder.begin_streaming_tool_call(&tool_call).unwrap();
    
    // Simulate a JSON string value being streamed - write_long_bytes expects a JSON string
    let json_string = r#""{\"param\": \"value\"}""#;  // JSON-encoded string
    let mut json_reader = Cursor::new(json_string);
    let mut buffer = [0u8; 64];
    let mut rjiter = RJiter::new(&mut json_reader, &mut buffer);
    builder.stream_tool_call_arguments_chunk(&mut rjiter).unwrap();
    
    builder.close_streaming_tool_call().unwrap();
    builder.end_message().unwrap();

    // Assert - arguments should be streamed correctly 
    let expected = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"tool_call"},{"id":"call_123","function_name":"get_user_name","function_arguments":"{\"param\": \"value\"}"}]
"#.to_owned();
    assert_eq!(writer.get_output(), expected);
}

#[test]
fn streaming_arguments_multiple_chunks() {
    use scan_json::RJiter;
    use std::io::Cursor;
    
    // Arrange
    let writer = RcWriter::new();
    let mut builder = StructureBuilder::new(writer.clone());
    
    // Act - simulate multiple argument chunks being streamed
    builder.begin_message();
    builder.enable_streaming_mode();
    
    // Start tool call
    let tool_call = gpt::funcalls::ContentItemFunction::new("call_456", "execute_query", "");
    builder.begin_streaming_tool_call(&tool_call).unwrap();
    
    // Stream first chunk of arguments
    let chunk1 = r#""{\"query\": \"SEL""#;
    let mut reader1 = Cursor::new(chunk1);
    let mut buffer1 = [0u8; 32];
    let mut rjiter1 = RJiter::new(&mut reader1, &mut buffer1);
    builder.stream_tool_call_arguments_chunk(&mut rjiter1).unwrap();
    
    // Stream second chunk 
    let chunk2 = r#"ECT * FROM users\", \"limit\": 10}""#;
    let mut reader2 = Cursor::new(chunk2);
    let mut buffer2 = [0u8; 64];
    let mut rjiter2 = RJiter::new(&mut reader2, &mut buffer2);
    builder.stream_tool_call_arguments_chunk(&mut rjiter2).unwrap();
    
    builder.close_streaming_tool_call().unwrap();
    builder.end_message().unwrap();

    // Assert - multiple chunks should be combined correctly
    let expected = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"tool_call"},{"id":"call_456","function_name":"execute_query","function_arguments":"{\"query\": \"SELCT * FROM users\", \"limit\": 10}"}]
"#.to_owned();
    assert_eq!(writer.get_output(), expected);
}