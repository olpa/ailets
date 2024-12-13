use jiter::{Jiter, NumberInt, Peek};


fn main() {
    let json_data = r#"
    {
        "name": "John Doe",
        "age": 43,
        "phones": [
            "+44 1234567",
            "+44 2345678"
        ]
    }"#;
    let mut jiter = Jiter::new(json_data.as_bytes());
    assert_eq!(jiter.next_object().unwrap(), Some("name"));
    assert_eq!(jiter.next_str().unwrap(), "John Doe");
    assert_eq!(jiter.next_key().unwrap(), Some("age"));
    assert_eq!(jiter.next_int().unwrap(), NumberInt::Int(43));
    assert_eq!(jiter.next_key().unwrap(), Some("phones"));
    assert_eq!(jiter.next_array().unwrap(), Some(Peek::String));
    // we know the next value is a string as we just asserted so
    assert_eq!(jiter.known_str().unwrap(), "+44 1234567");
    assert_eq!(jiter.array_step().unwrap(), Some(Peek::String));
    // same again
    assert_eq!(jiter.known_str().unwrap(), "+44 2345678");
    // next we'll get `None` from `array_step` as the array is finished
    assert_eq!(jiter.array_step().unwrap(), None);
    // and `None` from `next_key` as the object is finished
    assert_eq!(jiter.next_key().unwrap(), None);
    // and we check there's nothing else in the input
    jiter.finish().unwrap();
}

fn parse_message() {
    let json_data = r#"
    {
        "role":"assistant",
        "content":[
            {"type":"text", "text":"Hello! How can I assist you today?"}
        ]
    }"#;

    let mut jiter = Jiter::new(json_data.as_bytes());
    assert_eq!(jiter.next_object().unwrap(), Some("role"));
    assert_eq!(jiter.next_str().unwrap(), "assistant");
    assert_eq!(jiter.next_key().unwrap(), Some("content"));
    assert_eq!(jiter.next_array().unwrap(), Some(Peek::String));
}
