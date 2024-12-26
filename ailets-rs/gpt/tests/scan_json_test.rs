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
    let mut buffer = vec![0u8; 160]; // FIXME: return back 16
    let mut rjiter = RJiter::new(&mut reader, &mut buffer);

    let triggers: Vec<Trigger<()>> = vec![];
    scan_json(&triggers, &mut rjiter, ());
}
