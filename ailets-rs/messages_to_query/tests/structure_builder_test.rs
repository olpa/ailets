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
    builder.end().unwrap();

    assert_that!(
        writer.get_output(),
        equal_to(
            String::from(
                r#"{"role":"user","content":[_NL_{"type":"text","text":"Hello!"}_NL_]}_NL_"#
            )
            .replace("_NL_", "\n")
        )
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
    builder.end().unwrap();
    let text_item1 = r#"{"type":"text","text":"Text item of the first message"}"#;
    let text_item2a = r#"{"type":"text","text":"First item of the second message"}"#;
    let text_item2b = r#"{"type":"text","text":"Second item of the second message"}"#;

    let expected = String::from(
            r#"{"role":"user","content":[_NL__TI1__NL_]},{"role":"assistant","content":[_NL__TI2a_,_NL__TI2b__NL_]}_NL_"#
        ).replace("_NL_", "\n").replace("_TI1_", text_item1).replace("_TI2a_", text_item2a).replace("_TI2b_", text_item2b);

    assert_that!(writer.get_output(), equal_to(expected));
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
    builder.end().unwrap();
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
    builder.end().unwrap();

    let empty_msg = "{\"content\":[\n\n]}".to_owned();
    let two_empty_msgs = format!("{},{}\n", empty_msg, empty_msg);
    assert_that!(writer.get_output(), equal_to(two_empty_msgs));
}
