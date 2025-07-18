use actor_runtime_mocked::RcWriter;
use messages_to_markdown::_messages_to_markdown;
use std::io::Cursor;

#[test]
fn test_basic_conversion() {
    let json_data = r#"
        [{"type":"ctl"}, {"role":"assistant"}]
        [{"type":"text"}, {"text":"Hello!"}]
    "#;
    let reader = Cursor::new(json_data);
    let writer = RcWriter::new();

    _messages_to_markdown(reader, writer.clone()).unwrap();

    assert_eq!(writer.get_output(), "Hello!\n");
}

#[test]
fn test_multiple_content_items() {
    let json_data = r#"
        [{"type":"ctl"}, {"role":"assistant"}]
        [{"type":"text"}, {"text":"First item"}]
        [{"type":"text"}, {"text":"Second item"}]
        [{"type":"text"}, {"text":"Third item"}]
    "#;
    let reader = Cursor::new(json_data);
    let writer = RcWriter::new();

    _messages_to_markdown(reader, writer.clone()).unwrap();

    assert_eq!(
        writer.get_output(),
        "First item\n\nSecond item\n\nThird item\n"
    );
}

#[test]
fn test_two_messages() {
    let json_data = r#"
    [{"type":"ctl"}, {"role":"assistant"}]
      [{"type":"text"}, {"text":"First message"}]
    [{"type":"ctl"}, {"role":"assistant"}]
      [{"type":"text"}, {"text":"Second message"}]
      [{"type":"text"}, {"text":"Extra text"}]
    "#;
    let reader = Cursor::new(json_data);
    let writer = RcWriter::new();

    _messages_to_markdown(reader, writer.clone()).unwrap();

    assert_eq!(
        writer.get_output(),
        "First message\n\nSecond message\n\nExtra text\n"
    );
}

#[test]
fn test_empty_input() {
    let json_data = "";
    let reader = Cursor::new(json_data);
    let writer = RcWriter::new();

    _messages_to_markdown(reader, writer.clone()).unwrap();

    assert_eq!(writer.get_output(), "");
}

#[test]
fn test_long_text() {
    // Create a 4KB text string
    let long_text = "x".repeat(4096);
    let json_data = format!(
        r#"
        [{{"type":"ctl"}}, {{"role":"assistant"}}]
        [{{"type":"text"}}, {{"text":"{}"}}]
    "#,
        long_text
    );
    let reader = Cursor::new(json_data);
    let writer = RcWriter::new();

    _messages_to_markdown(reader, writer.clone()).unwrap();

    assert_eq!(writer.get_output(), format!("{}\n", long_text));
}

#[test]
fn test_skip_unknown_key_object() {
    let json_data = r#"
        [{"type":"ctl"}, {"role":"assistant"}]
        [{"type":"text"}, {"text":"First message", "unknown_key": {"some": "object"}}]
        [{"unknown_key": {"some": "object"}}]
        [{"type":"text"}, {"text":"Second message"}]
    "#;
    let reader = Cursor::new(json_data);
    let writer = RcWriter::new();

    _messages_to_markdown(reader, writer.clone()).unwrap();

    assert_eq!(writer.get_output(), "First message\n\nSecond message\n");
}

#[test]
fn test_json_escapes() {
    let json_data = r#"
        [{"type":"ctl"}, {"role":"assistant"}]
        [{"type":"text"}, {"text":"a\n\"\u0401\""}]
    "#;
    let reader = Cursor::new(json_data);
    let writer = RcWriter::new();

    _messages_to_markdown(reader, writer.clone()).unwrap();

    assert_eq!(writer.get_output(), "a\n\"\u{0401}\"\n");
}
