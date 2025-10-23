use actor_runtime_mocked::RcWriter;
use gpt::fcw_chat::FunCallsToChat;
use gpt::fcw_trait::FunCallsWrite;

pub mod dagops_mock;
use dagops_mock::DummyDagOps;

//
// Tests for FunCallsToChat implementation
//

#[test]
fn single_funcall() {
    // Arrange
    let writer = RcWriter::new();
    let mut chat_writer = FunCallsToChat::new(writer.clone());
    let mut dagops = DummyDagOps;

    // Act
    chat_writer
        .new_item(
            "call_9cFpsOXfVWMUoDz1yyyP1QXD",
            "get_user_name",
            &mut dagops,
        )
        .unwrap();
    chat_writer.arguments_chunk(b"{}").unwrap();
    chat_writer.end_item().unwrap();

    // Assert
    let output = writer.get_output();
    let expected = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"function","id":"call_9cFpsOXfVWMUoDz1yyyP1QXD","name":"get_user_name"},{"arguments":"{}"}]
"#;
    assert_eq!(output, expected);
}

#[test]
fn several_funcalls() {
    // Arrange
    let writer = RcWriter::new();
    let mut chat_writer = FunCallsToChat::new(writer.clone());
    let mut dagops = DummyDagOps;

    // First tool call
    chat_writer
        .new_item("call_foo", "get_foo", &mut dagops)
        .unwrap();
    chat_writer.arguments_chunk(b"{foo_args}").unwrap();
    chat_writer.end_item().unwrap();

    // Second tool call
    chat_writer
        .new_item("call_bar", "get_bar", &mut dagops)
        .unwrap();
    chat_writer.arguments_chunk(b"{bar_args}").unwrap();
    chat_writer.end_item().unwrap();

    // Third tool call
    chat_writer
        .new_item("call_baz", "get_baz", &mut dagops)
        .unwrap();
    chat_writer.arguments_chunk(b"{baz_args}").unwrap();
    chat_writer.end_item().unwrap();

    // Assert
    let output = writer.get_output();
    let expected = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"function","id":"call_foo","name":"get_foo"},{"arguments":"{foo_args}"}]
[{"type":"function","id":"call_bar","name":"get_bar"},{"arguments":"{bar_args}"}]
[{"type":"function","id":"call_baz","name":"get_baz"},{"arguments":"{baz_args}"}]
"#;
    assert_eq!(output, expected);
}

#[test]
fn long_arguments() {
    // Arrange
    let writer = RcWriter::new();
    let mut chat_writer = FunCallsToChat::new(writer.clone());
    let mut dagops = DummyDagOps;

    // Act - arguments come in multiple chunks
    chat_writer
        .new_item("call_123", "test_func", &mut dagops)
        .unwrap();
    chat_writer.arguments_chunk(b"{\\\"arg1\\\":").unwrap();
    chat_writer.arguments_chunk(b"\\\"value1\\\",").unwrap();
    chat_writer
        .arguments_chunk(b"\\\"arg2\\\":\\\"value2\\\"}")
        .unwrap();
    chat_writer.end_item().unwrap();

    // Assert
    let output = writer.get_output();
    let expected = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"function","id":"call_123","name":"test_func"},{"arguments":"{\"arg1\":\"value1\",\"arg2\":\"value2\"}"}]
"#;
    assert_eq!(output, expected);
}

#[test]
fn multiple_arguments_chunks() {
    // Arrange
    let writer = RcWriter::new();
    let mut chat_writer = FunCallsToChat::new(writer.clone());
    let mut dagops = DummyDagOps;

    // Act - multiple calls to arguments_chunk join values to one arguments attribute
    chat_writer
        .new_item("call_multi", "foo", &mut dagops)
        .unwrap();
    chat_writer.arguments_chunk(b"{\\\"first\\\":").unwrap();
    chat_writer.arguments_chunk(b"\\\"chunk1\\\",").unwrap();
    chat_writer.arguments_chunk(b"\\\"second\\\":").unwrap();
    chat_writer.arguments_chunk(b"\\\"chunk2\\\",").unwrap();
    chat_writer
        .arguments_chunk(b"\\\"third\\\":\\\"chunk3\\\"}")
        .unwrap();
    chat_writer.end_item().unwrap();

    // Assert
    let output = writer.get_output();
    let expected = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"function","id":"call_multi","name":"foo"},{"arguments":"{\"first\":\"chunk1\",\"second\":\"chunk2\",\"third\":\"chunk3\"}"}]
"#;
    assert_eq!(output, expected);
}

#[test]
fn empty_arguments() {
    // Arrange
    let writer = RcWriter::new();
    let mut chat_writer = FunCallsToChat::new(writer.clone());
    let mut dagops = DummyDagOps;

    // Act - function call with empty arguments
    chat_writer
        .new_item("call_empty", "no_args_func", &mut dagops)
        .unwrap();
    chat_writer.end_item().unwrap();

    // Assert
    let output = writer.get_output();
    let expected = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"function","id":"call_empty","name":"no_args_func"},{"arguments":""}]
"#;
    assert_eq!(output, expected);
}

#[test]
fn json_escaping_in_id_and_name() {
    // Arrange
    let writer = RcWriter::new();
    let mut chat_writer = FunCallsToChat::new(writer.clone());
    let mut dagops = DummyDagOps;

    // Act - id and name contain JSON special characters that need escaping
    chat_writer
        .new_item("call_\"quote\"", "test_\"name\"", &mut dagops)
        .unwrap();
    chat_writer
        .arguments_chunk(b"{\\\"key\\\":\\\"value\\\"}")
        .unwrap();
    chat_writer.end_item().unwrap();

    // Assert - id and name JSON special characters should be properly escaped
    let output = writer.get_output();
    let expected = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"function","id":"call_\"quote\"","name":"test_\"name\""},{"arguments":"{\"key\":\"value\"}"}]
"#;
    assert_eq!(output, expected);
}

#[test]
fn json_escaping_backslashes_in_id_and_name() {
    // Arrange
    let writer = RcWriter::new();
    let mut chat_writer = FunCallsToChat::new(writer.clone());
    let mut dagops = DummyDagOps;

    // Act - test backslash escaping in id and name
    chat_writer
        .new_item("call\\id", "test\\name", &mut dagops)
        .unwrap();
    chat_writer
        .arguments_chunk(b"{\\\"path\\\":\\\"C:\\\\\\\\Program Files\\\\\\\\\\\"}")
        .unwrap();
    chat_writer.end_item().unwrap();

    // Assert - backslashes in id and name should be properly escaped
    let output = writer.get_output();
    let expected = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"function","id":"call\\id","name":"test\\name"},{"arguments":"{\"path\":\"C:\\\\Program Files\\\\\"}"}]
"#;
    assert_eq!(output, expected);
}

#[test]
fn header_written_only_once() {
    // Arrange
    let writer = RcWriter::new();
    let mut chat_writer = FunCallsToChat::new(writer.clone());
    let mut dagops = DummyDagOps;

    // Act - multiple function calls should only write header once
    chat_writer
        .new_item("call_1", "func_1", &mut dagops)
        .unwrap();
    chat_writer.arguments_chunk(b"{}").unwrap();
    chat_writer.end_item().unwrap();

    chat_writer
        .new_item("call_2", "func_2", &mut dagops)
        .unwrap();
    chat_writer.arguments_chunk(b"{}").unwrap();
    chat_writer.end_item().unwrap();

    // Assert - header should appear only once at the beginning
    let output = writer.get_output();
    let expected = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"function","id":"call_1","name":"func_1"},{"arguments":"{}"}]
[{"type":"function","id":"call_2","name":"func_2"},{"arguments":"{}"}]
"#;
    assert_eq!(output, expected);

    // Verify there's only one occurrence of the header
    let header_count = output
        .matches(r#"[{"type":"ctl"},{"role":"assistant"}]"#)
        .count();
    assert_eq!(header_count, 1, "Header should appear exactly once");
}

#[test]
fn no_output_when_no_function_calls() {
    // Arrange
    let writer = RcWriter::new();
    let _chat_writer = FunCallsToChat::new(writer.clone());

    // Act - create writer but don't call any methods
    // (no new_item, arguments_chunk, or end_item calls)

    // Assert - no output should be written (including no header)
    let output = writer.get_output();
    assert_eq!(
        output, "",
        "No output should be written when no function calls are made"
    );
}
