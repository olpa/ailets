#[macro_use]
extern crate hamcrest;
use actor_runtime_mocked::add_file;
use actor_runtime_mocked::RcWriter;
use hamcrest::prelude::*;
use messages_to_query::env_opts::EnvOpts;
use messages_to_query::structure_builder::StructureBuilder;
use std::collections::HashMap;
use std::io::Write;

fn wrap_boilerplate(s: &str) -> String {
    let s1 = r#"{ "url": "https://api.openai.com/v1/chat/completions","#;
    let s2 = r#""method": "POST","#;
    let s3 = r#""headers": { "Content-type": "application/json", "Authorization": "Bearer {{secret}}" },"#;
    let s4 = r#""body": { "model": "gpt-4o-mini", "stream": true, "messages": ["#;
    let s_end = "]}}\n";
    let s = s.replace("_NL_", "\n");
    format!("{}\n{}\n{}\n{}{}{}", s1, s2, s3, s4, s, s_end)
}

fn create_empty_env_opts() -> EnvOpts {
    EnvOpts::from_map(HashMap::new())
}

fn begin_message(builder: &mut StructureBuilder<RcWriter>, role: &str) {
    builder.begin_content_item().unwrap();
    builder.add_item_attribute(String::from("type"), String::from("ctl")).unwrap();
    builder.add_item_attribute(String::from("role"), String::from(role)).unwrap();
    builder.handle_role().unwrap();
    builder.end_content_item().unwrap();
}

#[test]
fn happy_path_for_text() {
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());
    let mut builder = builder;

    begin_message(&mut builder, "user");
    builder.begin_content_item().unwrap();
    builder.add_item_attribute(String::from("type"), String::from("text")).unwrap();
    builder.begin_text().unwrap();
    write!(builder.get_writer(), "Hello!").unwrap();
    builder.end_text().unwrap();
    builder.end_content_item().unwrap();
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

    begin_message(&mut builder, "user");
    builder.begin_content_item().unwrap();
    builder.begin_text().unwrap();
    write!(builder.get_writer(), "Text item of the first message").unwrap();
    builder.end_text().unwrap();
    builder.end_content_item().unwrap();

    begin_message(&mut builder, "assistant");
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
fn skip_empty_content_items_but_create_content_wrapper() {
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());
    let mut builder = builder;

    begin_message(&mut builder, "user");
    builder.begin_content_item().unwrap();
    builder.end_content_item().unwrap();

    begin_message(&mut builder, "user");
    builder.begin_content_item().unwrap();
    builder.end_content_item().unwrap();
    builder.begin_content_item().unwrap();
    builder.end_content_item().unwrap();
    builder.end().unwrap();

    let empty_msg = "{\"role\":\"user\",\"content\":[\n\n]}".to_owned();
    let two_empty_msgs = wrap_boilerplate(format!("{},{}", empty_msg, empty_msg).as_str());
    assert_that!(writer.get_output(), equal_to(two_empty_msgs));
}

#[test]
fn several_contentless_roles_create_several_messages_anyway() {
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());
    let mut builder = builder;

    begin_message(&mut builder, "user");
    begin_message(&mut builder, "assistant");
    begin_message(&mut builder, "user");
    begin_message(&mut builder, "tool");
    builder.end().unwrap();

    let msg_user = r#"{"role":"user","content":[_NL__NL_]}"#;
    let msg_assistant = r#"{"role":"assistant","content":[_NL__NL_]}"#;
    let msg_tool = r#"{"role":"tool","content":[_NL__NL_]}"#;
    let msg_user2 = r#"{"role":"user","content":[_NL__NL_]}"#;
    let expected = wrap_boilerplate(&format!(
        "{},{},{},{}",
        msg_user, msg_assistant, msg_user2, msg_tool
    ));
    assert_that!(writer.get_output(), equal_to(expected));
}

#[test]
fn auto_generate_type_text() {
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());
    let mut builder = builder;
    begin_message(&mut builder, "user");

    builder.begin_content_item().unwrap();
    builder.begin_text().unwrap();
    write!(builder.get_writer(), "hello").unwrap();
    builder.end_text().unwrap();
    builder.end_content_item().unwrap();
    builder.end().unwrap();

    let expected =
        wrap_boilerplate(r#"{"role":"user","content":[_NL_{"type":"text","text":"hello"}_NL_]}"#);
    assert_that!(writer.get_output(), equal_to(expected));
}

#[test]
fn mix_type_text() {
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());
    let mut builder = builder;
    begin_message(&mut builder, "user");

    builder.begin_content_item().unwrap();
    builder.begin_content_item().unwrap();
    builder.add_item_type(String::from("text")).unwrap();
    builder.begin_text().unwrap();
    write!(builder.get_writer(), "hello").unwrap();
    builder.end_text().unwrap();
    builder.add_item_type(String::from("text")).unwrap();
    builder.end_content_item().unwrap();
    builder.end().unwrap();

    let expected =
        wrap_boilerplate(r#"{"role":"user","content":[_NL_{"type":"text","text":"hello"}_NL_]}"#);
    assert_that!(writer.get_output(), equal_to(expected));
}

#[test]
fn reject_conflicting_type() {
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());
    let mut builder = builder;
    begin_message(&mut builder, "user");

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
fn support_special_chars_and_unicode() {
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());
    let mut builder = builder;

    let special_chars = "Special chars: \"\\/\n\r\t\u{1F600}";

    begin_message(&mut builder, "user");
    builder.begin_content_item().unwrap();
    builder.add_item_type(String::from("text")).unwrap();
    builder.begin_text().unwrap();
    builder
        .get_writer()
        .write_all(special_chars.as_bytes())
        .unwrap();
    builder.end_text().unwrap();
    builder.end_content_item().unwrap();
    builder.end().unwrap();

    let expected = wrap_boilerplate(
        format!(
            r#"{{"role":"user","content":[_NL_{{"type":"text","text":"{}"}}_NL_]}}"#,
            special_chars
        )
        .as_str(),
    );
    assert_that!(writer.get_output(), equal_to(expected));
}

#[test]
fn add_image_by_url() {
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());
    let mut builder = builder;

    begin_message(&mut builder, "user");
    builder.begin_content_item().unwrap();

    builder.add_item_type(String::from("image")).unwrap();
    builder.begin_image_url().unwrap();
    builder
        .get_writer()
        .write_all(b"http://example.com/image.png")
        .unwrap();
    builder.end_image_url().unwrap();

    builder.end_content_item().unwrap();
    builder.end().unwrap();

    let expected_image_item =
        r#"{"type":"image_url","image_url":{"url":"http://example.com/image.png"}}"#;
    assert_that!(
        writer.get_output(),
        equal_to(wrap_boilerplate(&format!(
            r#"{{"role":"user","content":[_NL_{}_NL_]}}"#,
            expected_image_item
        )))
    );
}

#[test]
fn add_image_by_key() {
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());
    let mut builder = builder;

    add_file(String::from("media/image-as-key-1.png"), b"hello".to_vec());

    begin_message(&mut builder, "user");
    builder.begin_content_item().unwrap();

    builder.add_item_type(String::from("image")).unwrap();
    builder
        .set_content_item_attribute(String::from("content_type"), String::from("image/png"))
        .unwrap();
    builder.image_key("media/image-as-key-1.png").unwrap();

    builder.end_content_item().unwrap();
    builder.end().unwrap();

    let expected_image_item =
        r#"{"type":"image_url","image_url":{"url":"data:image/png;base64,aGVsbG8="}}"#;
    assert_that!(
        writer.get_output(),
        equal_to(wrap_boilerplate(&format!(
            r#"{{"role":"user","content":[_NL_{}_NL_]}}"#,
            expected_image_item
        )))
    );
}

#[test]
fn image_as_key_file_not_found() {
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());
    let mut builder = builder;

    begin_message(&mut builder, "user");
    builder.begin_content_item().unwrap();

    builder.add_item_type(String::from("image")).unwrap();
    builder
        .set_content_item_attribute(String::from("content_type"), String::from("image/png"))
        .unwrap();

    let result = builder.image_key("media/nonexistent.png");

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_that!(
        err.to_string().as_str(),
        matches_regex("media/nonexistent.png")
    );
}

#[test]
fn add_image_with_detail() {
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());
    let mut builder = builder;

    begin_message(&mut builder, "user");
    builder.begin_content_item().unwrap();

    builder.add_item_type(String::from("image")).unwrap();
    builder
        .set_content_item_attribute(String::from("detail"), String::from("high"))
        .unwrap();
    builder.begin_image_url().unwrap();
    builder
        .get_writer()
        .write_all(b"http://example.com/image.png")
        .unwrap();
    builder.end_image_url().unwrap();

    builder.end_content_item().unwrap();
    builder.end().unwrap();

    let expected_image_item = r#"{"type":"image_url","image_url":{"detail":"high","url":"http://example.com/image.png"}}"#;
    assert_that!(
        writer.get_output(),
        equal_to(wrap_boilerplate(&format!(
            r#"{{"role":"user","content":[_NL_{}_NL_]}}"#,
            expected_image_item
        )))
    );
}

#[test]
fn image_key_with_adversarial_content_type() {
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());
    let mut builder = builder;

    add_file(String::from("media/test.png"), b"hello".to_vec());

    begin_message(&mut builder, "user");
    builder.begin_content_item().unwrap();

    builder.add_item_type(String::from("image")).unwrap();
    builder
        .set_content_item_attribute(
            String::from("content_type"),
            String::from("\"\"image/png\0\\/\"';\u{202E}\u{2028}"),
        )
        .unwrap();
    builder.image_key("media/test.png").unwrap();

    builder.end_content_item().unwrap();
    builder.end().unwrap();

    // Only escape enough to have a valid json
    let expected_image_item = format!(
        r#"{{"type":"image_url","image_url":{{"url":"data:\"\"image/png\u0000\\/\"';{}{};base64,aGVsbG8="}}}}"#,
        '\u{202E}' as char, '\u{2028}' as char
    );
    assert_that!(
        writer.get_output(),
        equal_to(wrap_boilerplate(&format!(
            r#"{{"role":"user","content":[_NL_{}_NL_]}}"#,
            expected_image_item
        )))
    );
}

#[test]
fn image_settings_dont_transfer() {
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());
    let mut builder = builder;

    begin_message(&mut builder, "user");

    // First image with content_type and detail
    builder.begin_content_item().unwrap();
    builder.add_item_type(String::from("image")).unwrap();
    builder
        .set_content_item_attribute(String::from("content_type"), String::from("image/png"))
        .unwrap();
    builder
        .set_content_item_attribute(String::from("detail"), String::from("high"))
        .unwrap();
    builder.begin_image_url().unwrap();
    builder
        .get_writer()
        .write_all(b"http://example.com/image1.png")
        .unwrap();
    builder.end_image_url().unwrap();
    builder.end_content_item().unwrap();

    // Second image without content_type and detail
    builder.begin_content_item().unwrap();
    builder.add_item_type(String::from("image")).unwrap();
    builder.begin_image_url().unwrap();
    builder
        .get_writer()
        .write_all(b"http://example.com/image2.png")
        .unwrap();
    builder.end_image_url().unwrap();
    builder.end_content_item().unwrap();

    builder.end().unwrap();

    let expected_image1 = r#"{"type":"image_url","image_url":{"detail":"high","url":"http://example.com/image1.png"}}"#;
    let expected_image2 =
        r#"{"type":"image_url","image_url":{"url":"http://example.com/image2.png"}}"#;

    assert_that!(
        writer.get_output(),
        equal_to(wrap_boilerplate(&format!(
            r#"{{"role":"user","content":[_NL_{},_NL_{}_NL_]}}"#,
            expected_image1, expected_image2
        )))
    );
}

#[test]
fn mix_text_and_image_content() {
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());
    let mut builder = builder;
    begin_message(&mut builder, "user");

    // Text item
    builder.begin_content_item().unwrap();
    builder.add_item_type(String::from("text")).unwrap();
    builder.begin_text().unwrap();
    write!(builder.get_writer(), "Hello world").unwrap();
    builder.end_text().unwrap();
    builder.end_content_item().unwrap();

    // Image item
    builder.begin_content_item().unwrap();
    builder.add_item_type(String::from("image")).unwrap();
    builder.begin_image_url().unwrap();
    builder
        .get_writer()
        .write_all(b"http://example.com/image.png")
        .unwrap();
    builder.end_image_url().unwrap();
    builder.end_content_item().unwrap();

    // Another text item
    builder.begin_content_item().unwrap();
    builder.add_item_type(String::from("text")).unwrap();
    builder.begin_text().unwrap();
    write!(builder.get_writer(), "Another text").unwrap();
    builder.end_text().unwrap();
    builder.end_content_item().unwrap();

    builder.end().unwrap();

    let text_item1 = r#"{"type":"text","text":"Hello world"}"#;
    let image_item = r#"{"type":"image_url","image_url":{"url":"http://example.com/image.png"}}"#;
    let text_item2 = r#"{"type":"text","text":"Another text"}"#;
    let expected_content = format!(
        r#"_NL_{},_NL_{},_NL_{}_NL_"#,
        text_item1, image_item, text_item2
    );
    let expected_message = format!(r#"{{"role":"user","content":[{}]}}"#, expected_content);
    assert_that!(
        writer.get_output(),
        equal_to(wrap_boilerplate(&expected_message))
    );
}
