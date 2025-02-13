use areader::mocked_actor_runtime::{clear_mocks, get_output};
use messages_to_markdown::_messages_to_markdown;
use std::io::Cursor;

#[test]
fn test_basic_conversion() {
    clear_mocks();
    let json_data = r#"
    {
        "role":"assistant",
        "content":[
            {"type":"text", "text":"Hello!"}
        ]
    }"#;
    let reader = Cursor::new(json_data);

    _messages_to_markdown(reader);

    assert_eq!(get_output(), "Hello!\n");
}

#[test]
fn test_multiple_content_items() {
    clear_mocks();
    let json_data = r#"
    {
        "role":"assistant",
        "content":[
            {"type":"text", "text":"First item"},
            {"type":"text", "text":"Second item"},
            {"type":"text", "text":"Third item"}
        ]
    }"#;
    let reader = Cursor::new(json_data);

    _messages_to_markdown(reader);

    assert_eq!(get_output(), "First item\n\nSecond item\n\nThird item\n");
}

#[test]
fn test_two_messages() {
    clear_mocks();
    let json_data = r#"
    {
        "role":"assistant", 
        "content":[
            {"type":"text", "text":"First message"}
        ]
    }
    {
        "role":"assistant",
        "content":[
            {"type":"text", "text":"Second message"},
            {"type":"text", "text":"Extra text"}
        ]
    }"#;
    let reader = Cursor::new(json_data);

    _messages_to_markdown(reader);

    assert_eq!(
        get_output(),
        "First message\n\nSecond message\n\nExtra text\n"
    );
}

#[test]
fn test_empty_input() {
    clear_mocks();
    let json_data = "";
    let reader = Cursor::new(json_data);

    _messages_to_markdown(reader);

    assert_eq!(get_output(), "");
}

#[test]
fn test_long_text() {
    clear_mocks();
    // Create a 4KB text string
    let long_text = "x".repeat(4096);
    let json_data = format!(
        r#"
    {{
        "role":"assistant",
        "content":[
            {{"type":"text", "text":"{}"}}
        ]
    }}"#,
        long_text
    );
    let reader = Cursor::new(json_data);

    _messages_to_markdown(reader);

    assert_eq!(get_output(), format!("{}\n", long_text));
}

#[test]
fn test_skip_unknown_key_object() {
    clear_mocks();
    let json_data = r#"
    {
        "role":"assistant", 
        "content":[
            {"type":"text", "text":"First message"},
            {"unknown_key": {"some": "object"}},
            {"type":"text", "text":"Second message"}
        ]
    }"#;
    let reader = Cursor::new(json_data);

    _messages_to_markdown(reader);

    assert_eq!(get_output(), "First message\n\nSecond message\n");
}

#[test]
fn test_json_escapes() {
    clear_mocks();
    let json_data = r#"
    {
        "role":"assistant",
        "content":[
            {"type":"text", "text":"a\n\"\u0401\""}
        ]
    }"#;
    let reader = Cursor::new(json_data);

    _messages_to_markdown(reader);

    assert_eq!(get_output(), "a\n\"\u{0401}\"\n");
}
