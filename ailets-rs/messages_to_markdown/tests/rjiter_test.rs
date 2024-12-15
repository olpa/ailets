use std::io::Cursor;
use std::sync::Arc;

use jiter::JsonValue;
use jiter::LazyIndexMap;
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
fn test_rjiter_with_leading_spaces() {
    // Create input with 33 spaces followed by an empty JSON object
    let input = "                                 {}".as_bytes();
    let mut reader = Cursor::new(input);

    // Create a 16-byte buffer
    let mut buffer = [0u8; 16];

    // Create RJiter instance
    let mut rjiter = RJiter::new(&mut reader, &mut buffer);

    // Try to parse the input
    let result = rjiter.next_object_bytes();

    // Verify that we get an error because the buffer is too small
    // for the leading spaces
    assert!(result.is_err());
}
