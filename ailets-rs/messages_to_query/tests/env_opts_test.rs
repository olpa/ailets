#[macro_use]
extern crate hamcrest;
use actor_runtime_mocked::RcWriter;
use hamcrest::prelude::*;
use messages_to_query::env_opts::EnvOpts;
use messages_to_query::structure_builder::StructureBuilder;
use serde_json;
use std::collections::HashMap;
use std::io::Cursor;
use std::io::Write;

#[test]
fn test_env_opts_happy_path() {
    let input = r#"{"foo": "bar"}"#;
    let reader = Cursor::new(input.as_bytes());

    let env_opts = EnvOpts::envopts_from_reader(reader).unwrap();

    let foo_value = env_opts.get("foo").unwrap();
    assert_eq!(foo_value.as_str().unwrap(), "bar");
}

#[test]
fn test_env_opts_not_map() {
    let input = r#"["not", "a", "map"]"#;
    let reader = Cursor::new(input.as_bytes());

    let result = EnvOpts::envopts_from_reader(reader);
    assert!(result.is_err());
}
#[test]
fn test_env_opts_invalid_json() {
    let input = r#"{"foo": "bar""#; // Missing closing brace
    let reader = Cursor::new(input.as_bytes());

    let result = EnvOpts::envopts_from_reader(reader);
    assert!(result.is_err());
}

fn _build_with_env_opts(env_opts: EnvOpts) -> String {
    let writer = RcWriter::new();
    let mut builder = StructureBuilder::new(writer.clone(), env_opts);
    builder.begin_message().unwrap();
    builder.add_role("user").unwrap();
    builder.begin_content().unwrap();
    builder.begin_content_item().unwrap();
    builder.begin_text().unwrap();
    write!(builder.get_writer(), "Hello!").unwrap();
    builder.end_text().unwrap();
    builder.end_content_item().unwrap();
    builder.end_content().unwrap();
    builder.end_message().unwrap();
    builder.end().unwrap();
    writer.get_output()
}

#[test]
fn override_endpoint_model_stream() {
    let mut opts = HashMap::new();
    opts.insert(
        "http.url".to_string(),
        serde_json::Value::String(
            "https://my-custom-fairy-api.example.com/v1/chat/completions".to_string(),
        ),
    );
    opts.insert(
        "llm.model".to_string(),
        serde_json::Value::String("my-custom-fairy-model".to_string()),
    );
    opts.insert("llm.stream".to_string(), serde_json::Value::Bool(false));
    let env_opts = EnvOpts::from_map(opts);

    let output = _build_with_env_opts(env_opts);

    assert_that!(
        output.as_str(),
        matches_regex("my-custom-fairy-api.example.com")
    );
    assert_that!(output.as_str(), matches_regex("my-custom-fairy-model"));
    assert_that!(output.as_str(), matches_regex("\"stream\": false"));
}

#[test]
fn override_stream_option() {
    // set to "false"
    let mut opts = HashMap::new();
    opts.insert("llm.stream".to_string(), serde_json::Value::Bool(false));
    let env_opts = EnvOpts::from_map(opts);
    // act and assert
    let output = _build_with_env_opts(env_opts);
    assert_that!(output.as_str(), matches_regex("\"stream\": false"));

    // set to "true"
    let mut opts = HashMap::new();
    opts.insert("llm.stream".to_string(), serde_json::Value::Bool(true));
    let env_opts = EnvOpts::from_map(opts);
    // act and assert
    let output = _build_with_env_opts(env_opts);
    assert_that!(output.as_str(), matches_regex("\"stream\": true"));

    // set to an invalid value
    let mut opts = HashMap::new();
    opts.insert(
        "llm.stream".to_string(),
        serde_json::Value::String("invalid".to_string()),
    );
    let env_opts = EnvOpts::from_map(opts);
    // act and assert
    let output = _build_with_env_opts(env_opts);
    assert_that!(output.as_str(), matches_regex("\"stream\": true"));
}

#[test]
fn add_llm_options_of_different_types() {
    // Test a string value
    let mut opts = HashMap::new();
    opts.insert(
        "llm.foo".to_string(),
        serde_json::Value::String("bar".to_string()),
    );
    let env_opts = EnvOpts::from_map(opts);
    let output = _build_with_env_opts(env_opts);
    assert_that!(output.as_str(), matches_regex("\"foo\": \"bar\""));

    // Test a number value
    let mut opts = HashMap::new();
    opts.insert(
        "llm.max_tokens".to_string(),
        serde_json::Value::Number(serde_json::Number::from(100)),
    );
    let env_opts = EnvOpts::from_map(opts);
    let output = _build_with_env_opts(env_opts);
    assert_that!(output.as_str(), matches_regex("\"max_tokens\": 100"));

    // Test an array value
    let mut opts = HashMap::new();
    let arr = vec![
        serde_json::Value::String("system".to_string()),
        serde_json::Value::String("user".to_string()),
    ];
    opts.insert(
        "llm.allowed_roles".to_string(),
        serde_json::Value::Array(arr),
    );
    let env_opts = EnvOpts::from_map(opts);
    let output = _build_with_env_opts(env_opts);
    assert_that!(
        output.as_str(),
        matches_regex(r#""allowed_roles":\s*\["system","user"\]"#)
    );

    // Test an object value
    let mut opts = HashMap::new();
    let mut obj = serde_json::Map::new();
    obj.insert(
        "top_p".to_string(),
        serde_json::Value::Number(serde_json::Number::from_f64(0.9).unwrap()),
    );
    obj.insert(
        "top_k".to_string(),
        serde_json::Value::Number(serde_json::Number::from(50)),
    );
    opts.insert(
        "llm.sampling_params".to_string(),
        serde_json::Value::Object(obj),
    );
    let env_opts = EnvOpts::from_map(opts);
    let output = _build_with_env_opts(env_opts);
    assert_that!(
        output.as_str(),
        matches_regex(r#""sampling_params":\s*\{"top_k":50,"top_p":0.9\}"#)
    );
}

#[test]
fn no_duplicate_model_and_stream() {
    let mut opts = HashMap::new();
    opts.insert(
        "llm.model".to_string(),
        serde_json::Value::String("my-model".to_string()),
    );
    opts.insert("llm.stream".to_string(), serde_json::Value::Bool(false));
    let env_opts = EnvOpts::from_map(opts);
    let output = _build_with_env_opts(env_opts);

    // Model and stream should appear exactly once
    let model_count = output.matches("\"model\"").count();
    let stream_count = output.matches("\"stream\"").count();
    assert_that!(model_count, equal_to(1));
    assert_that!(stream_count, equal_to(1));

    // Verify the values are correct
    assert_that!(output.as_str(), matches_regex("\"model\":\\s*\"my-model\""));
    assert_that!(output.as_str(), matches_regex("\"stream\":\\s*false"));
}

#[test]
fn override_content_type_and_authorization_headers() {
    let mut opts = HashMap::new();
    opts.insert(
        "http.header.Content-type".to_string(),
        serde_json::Value::String("custom/type".to_string()),
    );
    opts.insert(
        "http.header.Authorization".to_string(),
        serde_json::Value::String("Bearer custom-token".to_string()),
    );
    let env_opts = EnvOpts::from_map(opts);
    let output = _build_with_env_opts(env_opts);

    assert_that!(
        output.as_str(),
        matches_regex(r#""Content-type":\s*"custom/type""#)
    );
    assert_that!(
        output.as_str(),
        matches_regex(r#""Authorization":\s*"Bearer custom-token""#)
    );
}
