#[macro_use]
extern crate hamcrest;
use actor_runtime_mocked::{add_file, RcWriter};
use hamcrest::prelude::*;
use messages_to_query::_process_query;
use messages_to_query::env_opts::EnvOpts;
use serde_json::Value;
use std::collections::HashMap;
use std::io::Cursor;

fn fix_json(json: &str) -> String {
    if json.contains("}\n{") {
        let json = json.replace("}\n{", "},\n{");
        let json = format!("[{}]", json);
        return json;
    }
    json.to_string()
}

fn create_empty_env_opts() -> EnvOpts {
    EnvOpts::from_map(HashMap::new())
}

fn wrap_boilerplate(s: &str) -> String {
    let s1 = r#"{ "url": "https://api.openai.com/v1/chat/completions","#;
    let s2 = r#""method": "POST","#;
    let s3 = r#""headers": { "Content-type": "application/json", "Authorization": "Bearer {{secret}}" },"#;
    let s4 = r#""body": { "model": "gpt-4o-mini", "stream": true, "messages": "#;
    let s_end = "}}\n";
    let s = s.replace("_NL_", "\n");
    format!("{}\n{}\n{}\n{}{}{}", s1, s2, s3, s4, s, s_end)
}

#[test]
fn test_text_items() {
    let fixture_content = std::fs::read_to_string("tests/fixture/text_items.txt")
        .expect("Failed to read fixture file 'text_items.txt'");
    let reader = Cursor::new(fixture_content.clone());
    let writer = RcWriter::new();

    _process_query(reader, writer.clone(), create_empty_env_opts()).unwrap();
    let output_json: Value = serde_json::from_str(&writer.get_output().as_str())
        .expect("Failed to parse output as JSON");

    let expected_output = wrap_boilerplate(fix_json(&fixture_content).as_str());
    let expected_json =
        serde_json::from_str(&expected_output).expect("Failed to parse expected output as JSON");
    assert_that!(output_json, equal_to(expected_json));
}

#[test]
fn input_as_array() {
    let fixture_content = std::fs::read_to_string("tests/fixture/text_items_arr.txt")
        .expect("Failed to read fixture file 'text_items_arr.txt'");
    let reader = Cursor::new(fixture_content.clone());
    let writer = RcWriter::new();

    _process_query(reader, writer.clone(), create_empty_env_opts()).unwrap();
    let output_json: Value = serde_json::from_str(&writer.get_output().as_str())
        .expect("Failed to parse output as JSON");

    let expected_json = serde_json::from_str(wrap_boilerplate(&fixture_content).as_str())
        .expect("Failed to parse expected output as JSON");
    assert_that!(output_json, equal_to(expected_json));
}

#[test]
fn image_url_as_is() {
    let input = r#"{"role": "user", "content": [{"type": "image", "image_url": "https://example.com/image.jpg"}]}"#;
    let reader = Cursor::new(input);
    let writer = RcWriter::new();

    _process_query(reader, writer.clone(), create_empty_env_opts()).unwrap();
    let output_json: Value = serde_json::from_str(&writer.get_output().as_str())
        .expect("Failed to parse output as JSON");

    let expected_item = r#"[{"role": "user", "content": [{"type": "image_url", "image_url": {"url": "https://example.com/image.jpg"}}]}]"#;
    let expected_json = serde_json::from_str(wrap_boilerplate(expected_item).as_str())
        .expect("Failed to parse expected output as JSON");
    assert_that!(output_json, equal_to(expected_json));
}

#[test]
fn image_as_key() {
    let input = r#"{"role": "user", "content": [{"type": "image", "image_key": "media/image-as-key.png"}]}"#;
    let reader = Cursor::new(input);
    let writer = RcWriter::new();
    add_file(String::from("media/image-as-key.png"), b"hello".to_vec());

    _process_query(reader, writer.clone(), create_empty_env_opts()).unwrap();
    let output_json: Value = serde_json::from_str(&writer.get_output().as_str())
        .expect("Failed to parse output as JSON");

    let expected_item = r#"[{"role": "user", "content": [{"type": "image_url", "image_url": {"url": "data:base64,aGVsbG8="}}]}]"#;
    let expected_json = serde_json::from_str(wrap_boilerplate(expected_item).as_str())
        .expect("Failed to parse expected output as JSON");
    assert_that!(output_json, equal_to(expected_json));
}
