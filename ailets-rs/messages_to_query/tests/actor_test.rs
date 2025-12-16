#[macro_use]
extern crate hamcrest;
use actor_runtime_mocked::{RcWriter, VfsActorRuntime};
use hamcrest::prelude::*;
use messages_to_query::_process_messages;
use messages_to_query::env_opts::EnvOpts;
use serde_json::Value;
use std::collections::HashMap;

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

fn inject_tools(payload: &str, tools: &str) -> String {
    payload.replace(
        r#""messages": ["#,
        &format!(r#""tools": {},"messages": ["#, tools),
    )
}

#[test]
fn test_text_items() {
    let fixture_content = std::fs::read_to_string("tests/fixture/text_items.txt")
        .expect("Failed to read fixture file 'text_items.txt'");
    let reader = fixture_content.as_bytes();
    let writer = RcWriter::new();

    let runtime = VfsActorRuntime::new();
    _process_messages(reader, writer.clone(), &runtime, create_empty_env_opts()).unwrap();
    let output_json: Value = serde_json::from_str(&writer.get_output().as_str())
        .expect("Failed to parse output as JSON");

    let expected_item1 = r#"{"role":"system","content":[{"type":"text","text":"You are a helpful assistant who answers in Spanish"}]}"#;
    let expected_item2 = r#"{"role":"user","content":[{"type":"text","text":"Hello!"}]}"#;
    let expected_output = wrap_boilerplate(&format!(
        r#"[_NL_{},_NL_{}_NL_]"#,
        expected_item1, expected_item2
    ));
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
    let reader = input.as_bytes();
    let writer = RcWriter::new();

    let runtime = VfsActorRuntime::new();
    _process_messages(reader, writer.clone(), &runtime, create_empty_env_opts()).unwrap();
    let output_json: Value = serde_json::from_str(&writer.get_output().as_str())
        .expect("Failed to parse output as JSON");

    let expected_item = r#"[{"role":"user","content":[
        {"type":"text","text":"Here's a \"quoted\" string\nwith newline and unicode: \u1F60 🌟"},
        {"type":"text","text":"Tab\there & escaped quotes: \"hello\""},
        {"type":"text","text":"Backslashes \\ and more \\\\ and control chars \u0007"}
    ]}]"#;
    let expected_json = serde_json::from_str(&wrap_boilerplate(&expected_item))
        .expect("Failed to parse expected output as JSON");
    assert_that!(output_json, equal_to(expected_json));
}

#[test]
fn image_url_as_is() {
    let input = r#"[{"type": "ctl"}, {"role": "user"}]
                   [{"type": "image"}, {"image_url": "https://example.com/image.jpg"}]"#;
    let reader = input.as_bytes();
    let writer = RcWriter::new();

    let runtime = VfsActorRuntime::new();
    _process_messages(reader, writer.clone(), &runtime, create_empty_env_opts()).unwrap();
    let output_json: Value =
        serde_json::from_str(&writer.get_output().as_str()).unwrap_or_else(|e| {
            eprintln!("Output: {}", writer.get_output());
            panic!("Failed to parse output as JSON: {}", e);
        });

    let expected_item = r#"[{"role": "user", "content": [{"type": "image_url", "image_url": {"url": "https://example.com/image.jpg"}}]}]"#;
    let expected_json = serde_json::from_str(&wrap_boilerplate(&expected_item))
        .expect("Failed to parse expected output as JSON");
    assert_that!(output_json, equal_to(expected_json));
}

#[test]
fn image_as_key() {
    let input = r#"[{"type": "ctl"}, {"role": "user"}]
                   [{"type": "image", "detail": "auto", "content_type": "image/png"},
                       {"image_key": "media/image-as-key-2.png"}]"#;
    let reader = input.as_bytes();
    let writer = RcWriter::new();
    let runtime = VfsActorRuntime::new();
    runtime.add_file(String::from("media/image-as-key-2.png"), b"hello".to_vec());
    _process_messages(reader, writer.clone(), &runtime, create_empty_env_opts()).unwrap();
    let output_json: Value = serde_json::from_str(&writer.get_output().as_str())
        .expect("Failed to parse output as JSON");

    let expected_item = r#"[{"role": "user", "content": [{"type": "image_url", "image_url": {"url": "data:image/png;base64,aGVsbG8=", "detail": "auto"}}]}]"#;
    let expected_json = serde_json::from_str(&wrap_boilerplate(&expected_item))
        .expect("Failed to parse expected output as JSON");
    assert_that!(output_json, equal_to(expected_json));
}

#[test]
fn mix_text_and_image() {
    let input = r#"[{"type": "ctl"}, {"role": "user"}]
                   [{"type": "text"}, {"text": "Here's an image:"}]
                   [{"type": "image"}, {"image_url": "https://example.com/image.jpg"}]
                   [{"type": "text"}, {"text": "What do you think about it?"}]"#;
    let reader = input.as_bytes();
    let writer = RcWriter::new();

    let runtime = VfsActorRuntime::new();
    _process_messages(reader, writer.clone(), &runtime, create_empty_env_opts()).unwrap();
    let output_json: Value = serde_json::from_str(&writer.get_output().as_str())
        .expect("Failed to parse output as JSON");

    let text_item1 = r#"{"type":"text","text":"Here's an image:"}"#;
    let image_item = r#"{"type":"image_url","image_url":{"url":"https://example.com/image.jpg"}}"#;
    let text_item2 = r#"{"type":"text","text":"What do you think about it?"}"#;
    let expected_item = format!(
        r#"[{{"role":"user","content":[_NL_{},_NL_{},_NL_{}_NL_]}}]"#,
        text_item1, image_item, text_item2
    );
    let expected_json = serde_json::from_str(&wrap_boilerplate(&expected_item))
        .expect("Failed to parse expected output as JSON");
    assert_that!(output_json, equal_to(expected_json));
}

#[test]
fn regression_one_item_not_two() {
    let input = r#"[{"type": "ctl"}, {"role": "user"}]
                   [{"type": "text"}, {"text": "Hello!"}]"#;
    let reader = input.as_bytes();
    let writer = RcWriter::new();

    let runtime = VfsActorRuntime::new();
    _process_messages(reader, writer.clone(), &runtime, create_empty_env_opts()).unwrap();
    let output_json: Value = serde_json::from_str(&writer.get_output().as_str())
        .expect("Failed to parse output as JSON");

    let expected_item = r#"[{"content":[{"type":"text","text":"Hello!"}],"role":"user"}]"#;
    let expected_json = serde_json::from_str(&wrap_boilerplate(&expected_item))
        .expect("Failed to parse expected output as JSON");
    assert_that!(output_json, equal_to(expected_json));
}

#[test]
fn function_call() {
    let input = r#"[{"type": "ctl"}, {"role": "assistant"}]
                   [{"type": "function", "id": "id123", "name": "get_weather"},
                       {"arguments": "{\"location\": \"London\", \"unit\": \"celsius\"}"}]"#;
    let reader = input.as_bytes();
    let writer = RcWriter::new();

    let runtime = VfsActorRuntime::new();
    _process_messages(reader, writer.clone(), &runtime, create_empty_env_opts()).unwrap();
    let output_json: Value = serde_json::from_str(&writer.get_output().as_str())
        .expect("Failed to parse output as JSON");

    let expected_item = r#"[{"role":"assistant","tool_calls": [
        {"id":"id123","type":"function","function":{"name":"get_weather","arguments":"{\"location\": \"London\", \"unit\": \"celsius\"}"}}
    ]}]"#;
    let expected_json = serde_json::from_str(&wrap_boilerplate(&expected_item))
        .expect("Failed to parse expected output as JSON");
    assert_that!(output_json, equal_to(expected_json));
}

#[test]
fn special_symbols_in_function_arguments() {
    let input = r#"[{"type": "ctl"}, {"role": "assistant"}]
                   [{"type": "function", "id": "id123", "name": "process_text"},
                       {"arguments": "{\"text\": \"Hello\\n\\\"World\\\" 🌟 🎉\u1F60\\\""}]"#;
    let reader = input.as_bytes();
    let writer = RcWriter::new();

    let runtime = VfsActorRuntime::new();
    _process_messages(reader, writer.clone(), &runtime, create_empty_env_opts()).unwrap();
    let output_json: Value = serde_json::from_str(&writer.get_output().as_str())
        .expect("Failed to parse output as JSON");

    let expected_item = r#"[{"role":"assistant","tool_calls": [
        {"id":"id123","type":"function","function":{"name":"process_text",
          "arguments":"{\"text\": \"Hello\\n\\\"World\\\" 🌟 🎉\u1F60\\\""}}
    ]}]"#;
    let expected_json = serde_json::from_str(&wrap_boilerplate(&expected_item))
        .expect("Failed to parse expected output as JSON");
    assert_that!(output_json, equal_to(expected_json));
}

#[test]
fn tool_specification() {
    let get_user_name_function = r#"{
        "name": "get_user_name",
        "description": "Get the user's name. Call this whenever you need to know the name of the user.",
        "strict": true,
        "parameters": {
            "type": "object",
            "properties": {},
            "additionalProperties": false
        }
    }"#;

    let another_function = r#"{
        "name": "another_function", "foo": "bar"
    }"#;

    let input = format!(
        r#"[{{"type": "toolspec"}}, {{"toolspec": {get_user_name_function}}}]
           [{{"type": "toolspec"}}, {{"toolspec": {another_function}}}]
           [{{"type": "ctl"}}, {{"role": "user"}}]
           [{{"type": "text"}}, {{"text": "Hello!"}}]"#
    );

    let reader = input.as_bytes();
    let writer = RcWriter::new();

    let runtime = VfsActorRuntime::new();
    _process_messages(reader, writer.clone(), &runtime, create_empty_env_opts()).unwrap();
    let output_json: Value = serde_json::from_str(&writer.get_output().as_str())
        .expect("Failed to parse output as JSON");

    let expected_tools = format!(
        r#"[{{"type":"function","function":{}}},{{"type":"function","function":{}}}]"#,
        get_user_name_function, another_function
    );
    let expected_item =
        format!(r#"[{{"role":"user","content":[{{"type":"text","text":"Hello!"}}]}}]"#);
    let expected = wrap_boilerplate(&expected_item);
    let expected_with_tools = inject_tools(&expected, &expected_tools);
    let expected_json = serde_json::from_str(&expected_with_tools)
        .expect("Failed to parse expected output as JSON");
    assert_that!(output_json, equal_to(expected_json));
}

#[test]
fn toolspec_by_key() {
    let toolspec_content = r#"{
        "name": "get_user_name",
        "description": "Get the user's name. Call this whenever you need to know the name of the user.",
        "strict": true,
        "parameters": {
            "type": "object",
            "properties": {},
            "additionalProperties": false
        }
    }"#;
    let input = r#"[{"type": "toolspec"}, {"toolspec_key": "tools/get_user_name.json"}]
           [{"type": "ctl"}, {"role": "user"}]
           "#;

    let reader = input.as_bytes();
    let writer = RcWriter::new();

    // Act
    let runtime = VfsActorRuntime::new();
    runtime.add_file(
        String::from("tools/get_user_name.json"),
        toolspec_content.as_bytes().to_vec(),
    );
    _process_messages(reader, writer.clone(), &runtime, create_empty_env_opts()).unwrap();

    // Assert
    let output_json: Value = serde_json::from_str(&writer.get_output().as_str())
        .expect("Failed to parse output as JSON");

    let expected_tools = format!(r#"[{{"type":"function","function":{}}}]"#, toolspec_content);
    let expected_item = format!(r#"[{{"role":"user"}}]"#);
    let expected = wrap_boilerplate(&expected_item);
    let expected_with_tools = inject_tools(&expected, &expected_tools);
    let expected_json = serde_json::from_str(&expected_with_tools)
        .expect("Failed to parse expected output as JSON");
    assert_that!(output_json, equal_to(expected_json));
}

#[test]
fn tool_role_with_tool_call_id() {
    let input = r#"[{"type": "ctl", "tool_call_id": "call_hEUJSGdhP42m1HYos3OTEeCS"}, {"role": "tool"}]
                   [{"type": "text"}, {"text": "{}"}]
                   [{"type": "text"}, {"text": "{\"get_user_name\": \"olpa\"}"}]"#;
    let reader = input.as_bytes();
    let writer = RcWriter::new();

    let runtime = VfsActorRuntime::new();
    _process_messages(reader, writer.clone(), &runtime, create_empty_env_opts()).unwrap();
    let output_json: Value = serde_json::from_str(&writer.get_output().as_str())
        .expect("Failed to parse output as JSON");

    let expected_item = r#"[{"role":"tool","tool_call_id":"call_hEUJSGdhP42m1HYos3OTEeCS","content":[
        {"type":"text","text":"{}"},
        {"type":"text","text":"{\"get_user_name\": \"olpa\"}"}
    ]}]"#;
    let expected_json = serde_json::from_str(&wrap_boilerplate(&expected_item))
        .expect("Failed to parse expected output as JSON");
    assert_that!(output_json, equal_to(expected_json));
}
