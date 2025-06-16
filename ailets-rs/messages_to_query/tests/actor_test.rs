#[macro_use]
extern crate hamcrest;
use actor_runtime_mocked::{add_file, RcWriter};
use hamcrest::prelude::*;
use messages_to_query::_process_query;
use messages_to_query::env_opts::EnvOpts;
use serde_json::Value;
use std::collections::HashMap;
use std::io::Cursor;

fn create_empty_env_opts() -> EnvOpts {
    EnvOpts::from_map(HashMap::new())
}

fn wrap_boilerplate(s: &str, tools: Option<&str>) -> String {
    let s1 = r#"{ "url": "https://api.openai.com/v1/chat/completions","#;
    let s2 = r#""method": "POST","#;
    let s3 = r#""headers": { "Content-type": "application/json", "Authorization": "Bearer {{secret}}" },"#;
    let s4 = r#""body": { "model": "gpt-4o-mini", "stream": true, _TOOLS_ "messages": "#;
    let tools = match tools {
        Some(tools) => format!(r#""tools": {tools},"#),
        None => "".to_string(),
    };
    let s4 = s4.replace("_TOOLS_", &tools);
    let s_end = "}}\n";
    let s = s.replace("_NL_", "\n");
    format!("{}\n{}\n{}\n{}{}{}{}", s1, s2, s3, s4, tools, s, s_end)
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

    let expected_item1 = r#"{"role":"system","content":[{"type":"text","text":"You are a helpful assistant who answers in Spanish"}]}"#;
    let expected_item2 = r#"{"role":"user","content":[{"type":"text","text":"Hello!"}]}"#;
    let expected_output = wrap_boilerplate(
        format!(r#"[_NL_{},_NL_{}_NL_]"#, expected_item1, expected_item2).as_str(),
        None,
    );
    let expected_json =
        serde_json::from_str(&expected_output).expect("Failed to parse expected output as JSON");
    assert_that!(output_json, equal_to(expected_json));
}

#[test]
fn special_symbols_in_text() {
    let input = r#"[{"type": "ctl"}, {"role": "user"}]
                   [{"type": "text"}, {"text": "Here's a \"quoted\" string\nwith newline and unicode: \u1F60 🌟"}]
                   [{"type": "text"}, {"text": "Tab\there & escaped quotes: \"hello\""}]
                   [{"type": "text"}, {"text": "Backslashes \\ and more \\\\ and control chars \u0007"}]"#;
    let reader = Cursor::new(input);
    let writer = RcWriter::new();

    _process_query(reader, writer.clone(), create_empty_env_opts()).unwrap();
    let output_json: Value = serde_json::from_str(&writer.get_output().as_str())
        .expect("Failed to parse output as JSON");

    let expected_item = r#"[{"role":"user","content":[
        {"type":"text","text":"Here's a \"quoted\" string\nwith newline and unicode: \u1F60 🌟"},
        {"type":"text","text":"Tab\there & escaped quotes: \"hello\""},
        {"type":"text","text":"Backslashes \\ and more \\\\ and control chars \u0007"}
    ]}]"#;
    let expected_json = serde_json::from_str(wrap_boilerplate(expected_item, None).as_str())
        .expect("Failed to parse expected output as JSON");
    assert_that!(output_json, equal_to(expected_json));
}

#[test]
fn image_url_as_is() {
    let input = r#"[{"type": "ctl"}, {"role": "user"}]
                   [{"type": "image"}, {"image_url": "https://example.com/image.jpg"}]"#;
    let reader = Cursor::new(input);
    let writer = RcWriter::new();

    _process_query(reader, writer.clone(), create_empty_env_opts()).unwrap();
    let output_json: Value = serde_json::from_str(&writer.get_output().as_str())
        .expect("Failed to parse output as JSON");

    let expected_item = r#"[{"role": "user", "content": [{"type": "image_url", "image_url": {"url": "https://example.com/image.jpg"}}]}]"#;
    let expected_json = serde_json::from_str(wrap_boilerplate(expected_item, None).as_str())
        .expect("Failed to parse expected output as JSON");
    assert_that!(output_json, equal_to(expected_json));
}

#[test]
fn image_as_key() {
    let input = r#"[{"type": "ctl"}, {"role": "user"}]
                   [{"type": "image", "detail": "auto", "content_type": "image/png"},
                       {"image_key": "media/image-as-key-2.png"}]"#;
    let reader = Cursor::new(input);
    let writer = RcWriter::new();
    add_file(String::from("media/image-as-key-2.png"), b"hello".to_vec());

    _process_query(reader, writer.clone(), create_empty_env_opts()).unwrap();
    let output_json: Value = serde_json::from_str(&writer.get_output().as_str())
        .expect("Failed to parse output as JSON");

    let expected_item = r#"[{"role": "user", "content": [{"type": "image_url", "image_url": {"url": "data:image/png;base64,aGVsbG8=", "detail": "auto"}}]}]"#;
    let expected_json = serde_json::from_str(wrap_boilerplate(expected_item, None).as_str())
        .expect("Failed to parse expected output as JSON");
    assert_that!(output_json, equal_to(expected_json));
}

#[test]
fn mix_text_and_image() {
    let input = r#"[{"type": "ctl"}, {"role": "user"}]
                   [{"type": "text"}, {"text": "Here's an image:"}]
                   [{"type": "image"}, {"image_url": "https://example.com/image.jpg"}]
                   [{"type": "text"}, {"text": "What do you think about it?"}]"#;
    let reader = Cursor::new(input);
    let writer = RcWriter::new();

    _process_query(reader, writer.clone(), create_empty_env_opts()).unwrap();
    let output_json: Value = serde_json::from_str(&writer.get_output().as_str())
        .expect("Failed to parse output as JSON");

    let text_item1 = r#"{"type":"text","text":"Here's an image:"}"#;
    let image_item = r#"{"type":"image_url","image_url":{"url":"https://example.com/image.jpg"}}"#;
    let text_item2 = r#"{"type":"text","text":"What do you think about it?"}"#;
    let expected_item = format!(
        r#"[{{"role":"user","content":[_NL_{},_NL_{},_NL_{}_NL_]}}]"#,
        text_item1, image_item, text_item2
    );
    let expected_json = serde_json::from_str(wrap_boilerplate(&expected_item, None).as_str())
        .expect("Failed to parse expected output as JSON");
    assert_that!(output_json, equal_to(expected_json));
}

#[test]
fn regression_one_item_not_two() {
    let input = r#"[{"type": "ctl"}, {"role": "user"}]
                   [{"type": "text"}, {"text": "Hello!"}]"#;
    let reader = Cursor::new(input);
    let writer = RcWriter::new();

    _process_query(reader, writer.clone(), create_empty_env_opts()).unwrap();
    let output_json: Value = serde_json::from_str(&writer.get_output().as_str())
        .expect("Failed to parse output as JSON");

    let expected_item = r#"[{"content":[{"type":"text","text":"Hello!"}],"role":"user"}]"#;
    let expected_json = serde_json::from_str(wrap_boilerplate(&expected_item, None).as_str())
        .expect("Failed to parse expected output as JSON");
    assert_that!(output_json, equal_to(expected_json));
}

#[test]
fn function_call() {
    let input = r#"[{"type": "ctl"}, {"role": "assistant"}]
                   [{"type": "function", "id": "id123", "name": "get_weather"},
                       {"arguments": "{\"location\": \"London\", \"unit\": \"celsius\"}"}]"#;
    let reader = Cursor::new(input);
    let writer = RcWriter::new();

    _process_query(reader, writer.clone(), create_empty_env_opts()).unwrap();
    let output_json: Value = serde_json::from_str(&writer.get_output().as_str())
        .expect("Failed to parse output as JSON");

    let expected_item = r#"[{"role":"assistant","tool_calls": [
        {"id":"id123","type":"function","function":{"name":"get_weather","arguments":"{\"location\": \"London\", \"unit\": \"celsius\"}"}}
    ]}]"#;
    let expected_json = serde_json::from_str(wrap_boilerplate(&expected_item, None).as_str())
        .expect("Failed to parse expected output as JSON");
    assert_that!(output_json, equal_to(expected_json));
}

#[test]
fn special_symbols_in_function_arguments() {
    let input = r#"[{"type": "ctl"}, {"role": "assistant"}]
                   [{"type": "function", "id": "id123", "name": "process_text"},
                       {"arguments": "{\"text\": \"Hello\\n\\\"World\\\" 🌟 🎉\u1F60\\\""}]"#;
    let reader = Cursor::new(input);
    let writer = RcWriter::new();

    _process_query(reader, writer.clone(), create_empty_env_opts()).unwrap();
    let output_json: Value = serde_json::from_str(&writer.get_output().as_str())
        .expect("Failed to parse output as JSON");

    let expected_item = r#"[{"role":"assistant","tool_calls": [
        {"id":"id123","type":"function","function":{"name":"process_text",
          "arguments":"{\"text\": \"Hello\\n\\\"World\\\" 🌟 🎉\u1F60\\\""}}
    ]}]"#;
    let expected_json = serde_json::from_str(wrap_boilerplate(&expected_item, None).as_str())
        .expect("Failed to parse expected output as JSON");
    assert_that!(output_json, equal_to(expected_json));
}

#[test]
fn tool_specification() {
    let tools = r#"{
        "name": "get_user_name",
        "description": "Get the user's name. Call this whenever you need to know the name of the user.",
        "strict": true,
        "parameters": {
            "type": "object",
            "properties": {},
            "additionalProperties": false
        }
    }
    {
        "name": "another_function", "foo": "bar"
    }"#;
    let input = r#"[{"type": "ctl"}, {"role": "user"}]
                   [{"type": "tools"}, _TOOLS_]
                   [{"type": "text"}, {"text": "Hello!"}]"#;
    let input = input.replace("_TOOLS_", &tools);

    let reader = Cursor::new(input);
    let writer = RcWriter::new();

    _process_query(reader, writer.clone(), create_empty_env_opts()).unwrap();
    let output_json: Value = serde_json::from_str(&writer.get_output().as_str())
        .expect("Failed to parse output as JSON");

    let expected_item = r#"[{"role":"user","content":[{"type":"text","text":"Hello!"}]}]"#;
    let expected_tools = r#"[{
        "type": "function",
        "function": {
            "name": "get_user_name",
            "description": "Get the user's name. Call this whenever you need to know the name of the user.",
            "strict": true,
            "parameters": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        }
    },
    {
        "type": "function",
        "function": {
            "name": "another_function", "foo": "bar"
        }
    }]"#;
    let expected = wrap_boilerplate(&expected_item, Some(&expected_tools));
    let expected_json =
        serde_json::from_str(&expected).expect("Failed to parse expected output as JSON");
    assert_that!(output_json, equal_to(expected_json));
}
