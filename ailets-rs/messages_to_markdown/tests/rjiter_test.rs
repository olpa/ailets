use std::io::Cursor;
use std::sync::Arc;

use jiter::JsonValue;
use jiter::LazyIndexMap;
use jiter::Peek;
use messages_to_markdown::rjiter::RJiter;

#[test]
fn sanity_check() {
    let input = r#"{}}"#;
    let mut buffer = [0u8; 16];
    let mut reader = Cursor::new(input.as_bytes());

    let mut rjiter = RJiter::new(&mut reader, &mut buffer);

    let result = rjiter.next_value();
    assert!(result.is_ok());

    let empty_object = JsonValue::Object(Arc::new(LazyIndexMap::new()));
    assert_eq!(result.unwrap(), empty_object);
}

#[test]
fn skip_spaces() {
    // Create input with 33 spaces followed by an empty JSON object
    // Use a 16-byte buffer
    let input = "                                 {}".as_bytes();
    let mut buffer = [0u8; 16];
    let mut reader = Cursor::new(input);

    let mut rjiter = RJiter::new(&mut reader, &mut buffer);

    let result = rjiter.next_value();
    println!("result: {:?}", result); // FIXME
    assert!(result.is_ok());

    let empty_object = JsonValue::Object(Arc::new(LazyIndexMap::new()));
    assert_eq!(result.unwrap(), empty_object);
}

#[test]
fn pass_through_long_string() {
    let input = r#"{ "text": "very very very long string" }"#;
    let mut buffer = [0u8; 8]; // Small buffer to force multiple reads
    let mut reader = Cursor::new(input.as_bytes());
    let mut writer = Vec::new();

    let mut rjiter = RJiter::new(&mut reader, &mut buffer);

    // Consume object start
    assert_eq!(rjiter.next_object().unwrap(), Some("text"));
    assert_eq!(rjiter.peek().unwrap(), Peek::String);

    // Consume the string value
    rjiter.write_bytes(&mut writer).unwrap();

    assert_eq!(writer, "very very very long string".as_bytes());
}
