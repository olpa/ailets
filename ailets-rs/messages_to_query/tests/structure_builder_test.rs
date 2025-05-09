#[macro_use]
extern crate hamcrest;
use actor_runtime_mocked::RcWriter;
use hamcrest::prelude::*;
use messages_to_query::env_opts::EnvOpts;
use messages_to_query::structure_builder::StructureBuilder;
use serde_json;
use std::collections::HashMap;
use std::io::Write;

fn wrap_boilerplate(s: &str) -> String {
    let s1 = r#"{ "url": "https://api.openai.com/v1/chat/completions","#;
    let s2 = r#""method": "POST","#;
    let s3 = r#""headers": { "Content-type": "application/json", "Authorization": "Bearer {{secret('openai','gpt4o')}}" },"#;
    let s4 = r#""body": { "model": "gpt-4o-mini", "stream": true, "messages": ["#;
    let s_end = "]}}\n";
    let s = s.replace("_NL_", "\n");
    format!("{}\n{}\n{}\n{}{}{}", s1, s2, s3, s4, s, s_end)
}

fn create_empty_env_opts() -> EnvOpts {
    EnvOpts::from_map(HashMap::new())
}

#[test]
fn happy_path_for_text() {
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());
    let mut builder = builder;

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

    assert_that!(
        writer.get_output(),
        equal_to(wrap_boilerplate(
            r#"{"role":"user","content":[_NL_{"type":"text","text":"Hello!"}_NL_]}"#
        ))
    );
}

#[test]
fn many_messages_and_items() {
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());
    let mut builder = builder;

    builder.begin_message().unwrap();
    builder.add_role("user").unwrap();
    builder.begin_content().unwrap();
    builder.begin_content_item().unwrap();
    builder.begin_text().unwrap();
    write!(builder.get_writer(), "Text item of the first message").unwrap();
    builder.end_text().unwrap();
    builder.end_content_item().unwrap();
    builder.end_content().unwrap();
    builder.end_message().unwrap();

    builder.begin_message().unwrap();
    builder.add_role("assistant").unwrap();
    builder.begin_content().unwrap();
    builder.begin_content_item().unwrap();
    builder.begin_text().unwrap();
    write!(builder.get_writer(), "First item of the second message").unwrap();
    builder.end_text().unwrap();
    builder.end_content_item().unwrap();
    builder.begin_content_item().unwrap();
    builder.begin_text().unwrap();
    write!(builder.get_writer(), "Second item of the second message").unwrap();
    builder.end_text().unwrap();
    builder.end_content_item().unwrap();
    builder.end_content().unwrap();
    builder.end_message().unwrap();
    builder.end().unwrap();
    let text_item1 = r#"{"type":"text","text":"Text item of the first message"}"#;
    let text_item2a = r#"{"type":"text","text":"First item of the second message"}"#;
    let text_item2b = r#"{"type":"text","text":"Second item of the second message"}"#;

    let expected = String::from(
            r#"{"role":"user","content":[_NL__TI1__NL_]},{"role":"assistant","content":[_NL__TI2a_,_NL__TI2b__NL_]}"#
        ).replace("_TI1_", text_item1).replace("_TI2a_", text_item2a).replace("_TI2b_", text_item2b);
    let expected = wrap_boilerplate(expected.as_str());
    assert_that!(writer.get_output(), equal_to(expected));
}

#[test]
fn skip_contentless_messages() {
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());
    let mut builder = builder;

    builder.begin_message().unwrap();
    builder.end_message().unwrap();
    builder.begin_message().unwrap();
    builder.end_message().unwrap();
    builder.begin_message().unwrap();
    builder.end_message().unwrap();
    builder.end().unwrap();
    assert_that!(writer.get_output(), equal_to(String::new()));
}

#[test]
fn skip_empty_content_items() {
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());
    let mut builder = builder;

    builder.begin_message().unwrap();
    builder.begin_content().unwrap();
    builder.begin_content_item().unwrap();
    builder.end_content_item().unwrap();
    builder.end_content().unwrap();
    builder.end_message().unwrap();

    builder.begin_message().unwrap();
    builder.begin_content().unwrap();
    builder.begin_content_item().unwrap();
    builder.end_content_item().unwrap();
    builder.begin_content_item().unwrap();
    builder.end_content_item().unwrap();
    builder.end_content().unwrap();
    builder.end_message().unwrap();
    builder.end().unwrap();

    let empty_msg = "{\"content\":[\n\n]}".to_owned();
    let two_empty_msgs = wrap_boilerplate(format!("{},{}", empty_msg, empty_msg).as_str());
    assert_that!(writer.get_output(), equal_to(two_empty_msgs));
}

#[test]
fn auto_generate_type_text() {
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());
    let mut builder = builder;
    builder.begin_message().unwrap();
    builder.begin_content().unwrap();

    builder.begin_content_item().unwrap();
    builder.begin_text().unwrap();
    write!(builder.get_writer(), "hello").unwrap();
    builder.end_text().unwrap();
    builder.end_content_item().unwrap();
    builder.get_writer().write_all(b"]}]}}\n").unwrap(); // boilerplate

    let expected = wrap_boilerplate(r#"{"content":[_NL_{"type":"text","text":"hello"}]}"#);
    assert_that!(writer.get_output(), equal_to(expected));
}

#[test]
fn mix_type_text() {
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());
    let mut builder = builder;
    builder.begin_message().unwrap();
    builder.begin_content().unwrap();

    builder.begin_content_item().unwrap();
    builder.add_item_type(String::from("text")).unwrap();
    builder.begin_text().unwrap();
    write!(builder.get_writer(), "hello").unwrap();
    builder.end_text().unwrap();
    builder.add_item_type(String::from("text")).unwrap();
    builder.end_content_item().unwrap();
    builder.get_writer().write_all(b"]}]}}\n").unwrap(); // boilerplate

    let expected = wrap_boilerplate(r#"{"content":[_NL_{"type":"text","text":"hello"}]}"#);
    assert_that!(writer.get_output(), equal_to(expected));
}

#[test]
fn reject_conflicting_type() {
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());
    let mut builder = builder;
    builder.begin_message().unwrap();
    builder.begin_content().unwrap();

    builder.begin_content_item().unwrap();
    builder.add_item_type(String::from("text")).unwrap();
    let err = builder.add_item_type(String::from("image")).unwrap_err();
    assert_that!(
        err,
        equal_to(
            "Wrong content item type: already typed as \"text\", new type is \"image\"".to_string()
        )
    );

    // Different content items have different types
    builder.begin_content_item().unwrap();
    builder.add_item_type(String::from("image")).unwrap();
}

#[test]
fn having_role_enforces_content() {
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());
    let mut builder = builder;

    builder.begin_message().unwrap();
    builder.add_role("user").unwrap();
    builder.end_message().unwrap();
    builder.end().unwrap();

    let expected = wrap_boilerplate(r#"{"role":"user","content":[_NL__NL_]}"#);
    assert_that!(writer.get_output(), equal_to(expected));
}

#[test]
fn support_special_chars_and_unicode() {
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());
    let mut builder = builder;

    let special_chars = "Special chars: \"\\/\n\r\t\u{1F600}";

    builder.begin_message().unwrap();
    builder.begin_content().unwrap();

    builder.begin_content_item().unwrap();
    builder.add_item_type(String::from("text")).unwrap();
    builder.begin_text().unwrap();
    builder
        .get_writer()
        .write_all(special_chars.as_bytes())
        .unwrap();
    builder.end_text().unwrap();
    builder.end_content_item().unwrap();

    builder.end_content().unwrap();
    builder.end_message().unwrap();
    builder.end().unwrap();

    let expected = wrap_boilerplate(
        format!(
            r#"{{"content":[_NL_{{"type":"text","text":"{}"}}_NL_]}}"#,
            special_chars
        )
        .as_str(),
    );
    assert_that!(writer.get_output(), equal_to(expected));
}
