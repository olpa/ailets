use std::cell::RefCell;

use gpt::rjiter::RJiter;
use gpt::scan_json::{scan_json, Matcher, Trigger};

#[test]
fn test_scan_json_empty_input() {
    let mut reader = std::io::empty();
    let mut buffer = vec![0u8; 16];
    let mut rjiter = RJiter::new(&mut reader, &mut buffer);

    let triggers: Vec<Trigger<()>> = vec![];
    scan_json(&triggers, &mut rjiter, ());
}

#[test]
fn test_scan_json_top_level_types() {
    let json = r#"null true false 42 3.14 "hello" [] {}"#;
    let mut reader = json.as_bytes();
    let mut buffer = vec![0u8; 16];
    let mut rjiter = RJiter::new(&mut reader, &mut buffer);

    let triggers: Vec<Trigger<()>> = vec![];
    scan_json(&triggers, &mut rjiter, ());
}

#[test]
fn test_scan_json_simple_object() {
    let json = r#"{"null": null, "bool": true, "num": 42, "float": 3.14, "str": "hello"}"#;
    let mut reader = json.as_bytes();
    let mut buffer = vec![0u8; 16];
    let mut rjiter = RJiter::new(&mut reader, &mut buffer);

    let triggers: Vec<Trigger<()>> = vec![];
    scan_json(&triggers, &mut rjiter, ());
}

#[test]
fn test_scan_json_simple_array() {
    let json = r#"[null, true, false, 42, 3.14, "hello"]"#;
    let mut reader = json.as_bytes();
    let mut buffer = vec![0u8; 16];
    let mut rjiter = RJiter::new(&mut reader, &mut buffer);

    let triggers: Vec<Trigger<()>> = vec![];
    scan_json(&triggers, &mut rjiter, ());
}

#[test]
fn test_scan_json_nested_complex() {
    let json = r#"{
        "array_of_objects": [
            {"name": "obj1", "values": [1, 2, 3]},
            {"name": "obj2", "nested": {"x": 10, "y": 20}}
        ],
        "object_with_arrays": {
            "nums": [1, 2, [3, 4, [5, 6]], 7],
            "mixed": [
                {"a": 1},
                [true, false],
                {"b": ["hello", "world"]},
                42
            ]
        },
        "deep_nesting": {
            "level1": {
                "level2": [
                    {"level3": {"value": [1, {"final": "deepest"}]}}
                ]
            }
        }
    }"#;
    let mut reader = json.as_bytes();
    let mut buffer = vec![0u8; 64];
    let mut rjiter = RJiter::new(&mut reader, &mut buffer);

    let triggers: Vec<Trigger<()>> = vec![];
    scan_json(&triggers, &mut rjiter, ());
}

#[test]
fn test_call_begin() {
    let json = r#"{"foo": "bar"}"#;
    let mut reader = json.as_bytes();
    let mut buffer = vec![0u8; 16];
    let mut rjiter = RJiter::new(&mut reader, &mut buffer);

    let mut called: RefCell<bool> = RefCell::new(false);
    let triggers = vec![
        Trigger {
            matcher: Matcher::new("foo".to_string(), None, None, None),
            action: Box::new(|_, _| *called.borrow_mut() = true),
        }
    ];

    scan_json(&triggers, &mut rjiter, ());
    assert!(*called.borrow(), "Trigger should have been called for 'foo'");
}
