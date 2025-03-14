#[macro_use]
extern crate hamcrest;
use actor_runtime_mocked::RcWriter;
use hamcrest::prelude::*;
use messages_to_query::structure_builder::StructureBuilder;

#[test]
fn happy_path_for_text() {
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone());
    let mut builder = builder;

    builder.begin_message().unwrap();
    builder.add_role("user").unwrap();
    builder.begin_content().unwrap();
    builder.begin_content_item().unwrap();
    builder.add_text("Hello!").unwrap();
    builder.end_content_item().unwrap();
    builder.end_content().unwrap();
    builder.end_message().unwrap();

    assert_that!(
        writer.get_output(),
        equal_to(String::from(
            r#"{"role":"user","content":[{"type":"text","text":"Hello!"}]}"#
        ))
    );
}

#[test]
fn many_messages_and_items() {
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone());
    let mut builder = builder;

    builder.begin_message().unwrap();
    builder.add_role("user").unwrap();
    builder.begin_content().unwrap();
    builder.begin_content_item().unwrap();
    builder.add_text("Text item of the first message").unwrap();
    builder.end_content_item().unwrap();
    builder.end_content().unwrap();
    builder.end_message().unwrap();

    builder.begin_message().unwrap();
    builder.add_role("assistant").unwrap();
    builder.begin_content().unwrap();
    builder.begin_content_item().unwrap();
    builder
        .add_text("First item of the second message")
        .unwrap();
    builder.end_content_item().unwrap();
    builder.begin_content_item().unwrap();
    builder
        .add_text("Second item of the second message")
        .unwrap();
    builder.end_content_item().unwrap();
    builder.end_content().unwrap();
    builder.end_message().unwrap();

    let text_item1 = r#"{"type":"text","text":"Text item of the first message"}"#;
    let text_item2a = r#"{"type":"text","text":"First item of the second message"}"#;
    let text_item2b = r#"{"type":"text","text":"Second item of the second message"}"#;

    assert_that!(
        writer.get_output(),
        equal_to(format!(
            "{}{}{}{}{}{}{}{}",
            r#"{"role":"user","content":["#,
            text_item1,
            "]},",
            r#"{"role":"assistant","content":["#,
            text_item2a,
            ",",
            text_item2b,
            "]}"
        ))
    );
}

#[test]
fn skip_contentless_messages() {
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone());
    let mut builder = builder;

    builder.begin_message().unwrap();
    builder.end_message().unwrap();
    builder.begin_message().unwrap();
    builder.end_message().unwrap();
    builder.begin_message().unwrap();
    builder.end_message().unwrap();

    assert_that!(writer.get_output(), equal_to(String::new()));
}

#[test]
fn skip_empty_content_items() {
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone());
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

    let empty_msg = r#"{"role":"user","content":[]}"#.to_owned() + "\n";
    assert_that!(writer.get_output(), equal_to(empty_msg.repeat(2)));
}
