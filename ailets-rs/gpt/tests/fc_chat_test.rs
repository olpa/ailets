use gpt::funcalls_write::{FunCallsToChat, FunCallsWrite};

//
// Tests for FunCallsToChat implementation
//

#[test]
fn single_funcall() {
    // Arrange
    let mut writer = Vec::new();
    let mut chat_writer = FunCallsToChat::new(&mut writer);

    // Act
    chat_writer
        .new_item("call_9cFpsOXfVWMUoDz1yyyP1QXD", "get_user_name")
        .unwrap();
    chat_writer.arguments_chunk("{}").unwrap();
    chat_writer.end_item().unwrap();

    // Assert
    let output = String::from_utf8(writer).unwrap();
    let expected = r#"[{"type":"function","id":"call_9cFpsOXfVWMUoDz1yyyP1QXD","name":"get_user_name"},{"arguments":"{}"}]
"#;
    assert_eq!(output, expected);
}

#[test]
fn several_funcalls() {
    // Arrange
    let mut writer = Vec::new();
    let mut chat_writer = FunCallsToChat::new(&mut writer);

    // First tool call
    chat_writer.new_item("call_foo", "get_foo").unwrap();
    chat_writer.arguments_chunk("{foo_args}").unwrap();
    chat_writer.end_item().unwrap();

    // Second tool call
    chat_writer.new_item("call_bar", "get_bar").unwrap();
    chat_writer.arguments_chunk("{bar_args}").unwrap();
    chat_writer.end_item().unwrap();

    // Third tool call
    chat_writer.new_item("call_baz", "get_baz").unwrap();
    chat_writer.arguments_chunk("{baz_args}").unwrap();
    chat_writer.end_item().unwrap();

    // Assert
    let output = String::from_utf8(writer).unwrap();
    let expected = r#"[{"type":"function","id":"call_foo","name":"get_foo"},{"arguments":"{foo_args}"}]
[{"type":"function","id":"call_bar","name":"get_bar"},{"arguments":"{bar_args}"}]
[{"type":"function","id":"call_baz","name":"get_baz"},{"arguments":"{baz_args}"}]
"#;
    assert_eq!(output, expected);
}

#[test]
fn long_arguments() {
    // Arrange
    let mut writer = Vec::new();
    let mut chat_writer = FunCallsToChat::new(&mut writer);

    // Act - arguments come in multiple chunks
    chat_writer.new_item("call_123", "test_func").unwrap();
    chat_writer.arguments_chunk("{\"arg1\":").unwrap();
    chat_writer.arguments_chunk("\"value1\",").unwrap();
    chat_writer.arguments_chunk("\"arg2\":\"value2\"}").unwrap();
    chat_writer.end_item().unwrap();

    // Assert
    let output = String::from_utf8(writer).unwrap();
    let expected = "[{\"type\":\"function\",\"id\":\"call_123\",\"name\":\"test_func\"},{\"arguments\":\"{\"arg1\":\"value1\",\"arg2\":\"value2\"}\"}]\n";
    assert_eq!(output, expected);
}

#[test]
fn empty_arguments() {
    // Arrange
    let mut writer = Vec::new();
    let mut chat_writer = FunCallsToChat::new(&mut writer);

    // Act - function call with empty arguments
    chat_writer.new_item("call_empty", "no_args_func").unwrap();
    chat_writer.end_item().unwrap();

    // Assert
    let output = String::from_utf8(writer).unwrap();
    let expected = r#"[{"type":"function","id":"call_empty","name":"no_args_func"},{"arguments":""}]
"#;
    assert_eq!(output, expected);
}
