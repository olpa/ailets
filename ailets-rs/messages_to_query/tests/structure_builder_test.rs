#[macro_use]
extern crate hamcrest;
use actor_runtime::FfiActorRuntime;
use actor_runtime_mocked::add_file;
use actor_runtime_mocked::RcWriter;
use embedded_io::Write;
use hamcrest::prelude::*;
use messages_to_query::env_opts::EnvOpts;
use messages_to_query::structure_builder::StructureBuilder;
use serde_json::Value;
use std::collections::HashMap;

fn wrap_boilerplate(s: &str) -> String {
    let s1 = r#"{ "url": "https://api.openai.com/v1/chat/completions","#;
    let s2 = r#""method": "POST","#;
    let s3 = r#""headers": { "Content-type": "application/json", "Authorization": "Bearer {{secret}}" },"#;
    let s4 = r#""body": { "model": "gpt-4o-mini", "stream": true, "messages": ["#;
    let s_end = "]}}\n";
    let s = s.replace("_NL_", "\n");
    format!("{}\n{}\n{}\n{}{}{}", s1, s2, s3, s4, s, s_end)
}

fn inject_tools(payload: &str, tools: &str) -> String {
    payload.replace(
        r#""messages": ["#,
        &format!(r#""tools": {},"messages": ["#, tools),
    )
}

fn create_empty_env_opts() -> EnvOpts {
    EnvOpts::from_map(HashMap::new())
}

fn begin_message(builder: &mut StructureBuilder<RcWriter>, role: &str) {
    builder.begin_item().unwrap();
    builder
        .add_item_attribute(String::from("type"), String::from("ctl"))
        .unwrap();
    builder.handle_role(role).unwrap();
    builder.end_item().unwrap();
}

#[test]
fn happy_path_for_text() {
    let writer = RcWriter::new();
    let runtime = FfiActorRuntime::new();
    let builder = StructureBuilder::new(writer.clone(), &runtime, create_empty_env_opts());
    let mut builder = builder;

    begin_message(&mut builder, "user");
    builder.begin_item().unwrap();
    builder
        .add_item_attribute(String::from("type"), String::from("text"))
        .unwrap();
    builder.begin_text().unwrap();
    embedded_io::Write::write_all(builder.get_writer(), b"Hello!").unwrap();
    builder.end_text().unwrap();
    builder.end_item().unwrap();
    builder.end().unwrap();

    assert_that!(
        writer.get_output(),
        equal_to(wrap_boilerplate(
            r#"{"role":"user",_NL_"content":[_NL_{"type":"text","text":"Hello!"}_NL_]}"#
        ))
    );
}

#[test]
fn many_messages_and_items() {
    let writer = RcWriter::new();
    let runtime = FfiActorRuntime::new();
    let builder = StructureBuilder::new(writer.clone(), &runtime, create_empty_env_opts());
    let mut builder = builder;

    begin_message(&mut builder, "user");
    builder.begin_item().unwrap();
    builder.begin_text().unwrap();
    embedded_io::Write::write_all(builder.get_writer(), b"Text item of the first message").unwrap();
    builder.end_text().unwrap();
    builder.end_item().unwrap();

    begin_message(&mut builder, "assistant");
    builder.begin_item().unwrap();
    builder.begin_text().unwrap();
    embedded_io::Write::write_all(builder.get_writer(), b"First item of the second message")
        .unwrap();
    builder.end_text().unwrap();
    builder.end_item().unwrap();
    builder.begin_item().unwrap();
    builder.begin_text().unwrap();
    embedded_io::Write::write_all(builder.get_writer(), b"Second item of the second message")
        .unwrap();
    builder.end_text().unwrap();
    builder.end_item().unwrap();
    builder.end().unwrap();

    let text_item1 = r#"{"type":"text","text":"Text item of the first message"}"#;
    let text_item2a = r#"{"type":"text","text":"First item of the second message"}"#;
    let text_item2b = r#"{"type":"text","text":"Second item of the second message"}"#;
    let expected = String::from(
            r#"{"role":"user",_NL_"content":[_NL__TI1__NL_]},{"role":"assistant",_NL_"content":[_NL__TI2a_,_NL__TI2b__NL_]}"#
        ).replace("_TI1_", text_item1).replace("_TI2a_", text_item2a).replace("_TI2b_", text_item2b);
    let expected = wrap_boilerplate(expected.as_str());
    assert_that!(writer.get_output(), equal_to(expected));
}

#[test]
fn several_contentless_roles_create_several_messages_anyway() {
    let writer = RcWriter::new();
    let runtime = FfiActorRuntime::new();
    let builder = StructureBuilder::new(writer.clone(), &runtime, create_empty_env_opts());
    let mut builder = builder;

    begin_message(&mut builder, "user");
    begin_message(&mut builder, "assistant");
    begin_message(&mut builder, "user");

    // For tool role, we need to set up tool_call_id
    builder.begin_item().unwrap();
    builder
        .add_item_attribute(String::from("type"), String::from("ctl"))
        .unwrap();
    builder
        .add_item_attribute(String::from("tool_call_id"), String::from("call_dummy"))
        .unwrap();
    builder.handle_role("tool").unwrap();
    builder.end_item().unwrap();

    builder.end().unwrap();

    let msg_user = r#"{"role":"user"}"#;
    let msg_assistant = r#"{"role":"assistant"}"#;
    let msg_tool = r#"{"role":"tool","tool_call_id":"call_dummy"}"#;
    let msg_user2 = r#"{"role":"user"}"#;
    let expected = wrap_boilerplate(&format!(
        "{},{},{},{}",
        msg_user, msg_assistant, msg_user2, msg_tool
    ));
    assert_that!(writer.get_output(), equal_to(expected));
}

#[test]
fn reject_role_for_non_ctl_type() {
    let writer = RcWriter::new();
    let runtime = FfiActorRuntime::new();
    let builder = StructureBuilder::new(writer.clone(), &runtime, create_empty_env_opts());
    let mut builder = builder;

    builder.begin_item().unwrap();
    builder
        .add_item_attribute(String::from("type"), String::from("text"))
        .unwrap();

    let err = builder.handle_role("user").unwrap_err();
    assert_that!(
        err,
        equal_to("For 'role' attribute, expected item type 'ctl', got 'text'".to_string())
    );
}

#[test]
fn auto_generate_type_text() {
    let writer = RcWriter::new();
    let runtime = FfiActorRuntime::new();
    let builder = StructureBuilder::new(writer.clone(), &runtime, create_empty_env_opts());
    let mut builder = builder;
    begin_message(&mut builder, "user");

    builder.begin_item().unwrap();
    builder.begin_text().unwrap();
    embedded_io::Write::write_all(builder.get_writer(), b"hello").unwrap();
    builder.end_text().unwrap();
    builder.end_item().unwrap();
    builder.end().unwrap();

    let expected = wrap_boilerplate(
        r#"{"role":"user",_NL_"content":[_NL_{"type":"text","text":"hello"}_NL_]}"#,
    );
    assert_that!(writer.get_output(), equal_to(expected));
}

#[test]
fn reject_unknown_type() {
    let writer = RcWriter::new();
    let runtime = FfiActorRuntime::new();
    let builder = StructureBuilder::new(writer.clone(), &runtime, create_empty_env_opts());
    let mut builder = builder;
    begin_message(&mut builder, "user");

    builder.begin_item().unwrap();
    let err = builder
        .add_item_attribute(String::from("type"), String::from("unknown"))
        .unwrap_err();
    assert_that!(
        err,
        equal_to(
            "Invalid type value: 'unknown'. Allowed values are: text, image, function, ctl, toolspec"
                .to_string()
        )
    );
}

#[test]
fn reject_conflicting_type() {
    let writer = RcWriter::new();
    let runtime = FfiActorRuntime::new();
    let builder = StructureBuilder::new(writer.clone(), &runtime, create_empty_env_opts());
    let mut builder = builder;
    begin_message(&mut builder, "user");

    builder.begin_item().unwrap();
    builder
        .add_item_attribute(String::from("type"), String::from("text"))
        .unwrap();
    let err = builder
        .add_item_attribute(String::from("type"), String::from("image"))
        .unwrap_err();
    assert_that!(
        err,
        equal_to(
            "Wrong content item type: already typed as \"text\", new type is \"image\"".to_string()
        )
    );

    // Different content items have different types
    builder.begin_item().unwrap();
    builder
        .add_item_attribute(String::from("type"), String::from("image"))
        .unwrap();
}

#[test]
fn support_special_chars_and_unicode() {
    let writer = RcWriter::new();
    let runtime = FfiActorRuntime::new();
    let builder = StructureBuilder::new(writer.clone(), &runtime, create_empty_env_opts());
    let mut builder = builder;

    let special_chars = "Special chars: \"\\/\n\r\t\u{1F600}";

    begin_message(&mut builder, "user");
    builder.begin_item().unwrap();
    builder
        .add_item_attribute(String::from("type"), String::from("text"))
        .unwrap();
    builder.begin_text().unwrap();
    builder
        .get_writer()
        .write_all(special_chars.as_bytes())
        .unwrap();
    builder.end_text().unwrap();
    builder.end_item().unwrap();
    builder.end().unwrap();

    let expected = wrap_boilerplate(
        format!(
            r#"{{"role":"user",_NL_"content":[_NL_{{"type":"text","text":"{}"}}_NL_]}}"#,
            special_chars
        )
        .as_str(),
    );
    assert_that!(writer.get_output(), equal_to(expected));
}

#[test]
fn pass_preceding_attributes_to_text_output() {
    let writer = RcWriter::new();
    let runtime = FfiActorRuntime::new();
    let builder = StructureBuilder::new(writer.clone(), &runtime, create_empty_env_opts());
    let mut builder = builder;

    begin_message(&mut builder, "user");
    builder.begin_item().unwrap();

    builder
        .add_item_attribute(String::from("custom_attr_1"), String::from("value_1"))
        .unwrap();
    builder
        .add_item_attribute(String::from("type"), String::from("text"))
        .unwrap();
    builder
        .add_item_attribute(String::from("custom_attr_2"), String::from("value_2"))
        .unwrap();

    builder.begin_text().unwrap();
    embedded_io::Write::write_all(builder.get_writer(), b"Hello world").unwrap();
    builder.end_text().unwrap();
    builder.end_item().unwrap();
    builder.end().unwrap();

    let expected_text_item = r#"{"type":"text","custom_attr_1":"value_1","custom_attr_2":"value_2","text":"Hello world"}"#;
    assert_that!(
        writer.get_output(),
        equal_to(wrap_boilerplate(
            format!(
                r#"{{"role":"user",_NL_"content":[_NL_{}_NL_]}}"#,
                expected_text_item
            )
            .as_str()
        ))
    );
}

#[test]
fn pass_following_attributes_to_text_output() {
    let writer = RcWriter::new();
    let runtime = FfiActorRuntime::new();
    let builder = StructureBuilder::new(writer.clone(), &runtime, create_empty_env_opts());
    let mut builder = builder;

    begin_message(&mut builder, "user");
    builder.begin_item().unwrap();
    builder.begin_text().unwrap();
    embedded_io::Write::write_all(builder.get_writer(), b"Hello world").unwrap();
    builder.end_text().unwrap();

    builder
        .add_item_attribute(String::from("custom_attr_3"), String::from("value_3"))
        .unwrap();
    builder
        .add_item_attribute(String::from("custom_attr_4"), String::from("value_4"))
        .unwrap();

    builder.end_item().unwrap();
    builder.end().unwrap();

    let expected_text_item = r#"{"type":"text","text":"Hello world","custom_attr_3":"value_3","custom_attr_4":"value_4"}"#;
    assert_that!(
        writer.get_output(),
        equal_to(wrap_boilerplate(
            format!(
                r#"{{"role":"user",_NL_"content":[_NL_{}_NL_]}}"#,
                expected_text_item
            )
            .as_str()
        ))
    );
}

#[test]
fn add_image_by_url() {
    let writer = RcWriter::new();
    let runtime = FfiActorRuntime::new();
    let builder = StructureBuilder::new(writer.clone(), &runtime, create_empty_env_opts());
    let mut builder = builder;

    begin_message(&mut builder, "user");
    builder.begin_item().unwrap();

    builder
        .add_item_attribute(String::from("type"), String::from("image"))
        .unwrap();
    builder.begin_image_url().unwrap();
    builder
        .get_writer()
        .write_all(b"http://example.com/image.png")
        .unwrap();
    builder.end_image_url().unwrap();

    builder.end_item().unwrap();
    builder.end().unwrap();

    let expected_image_item =
        r#"{"type":"image_url","image_url":{"url":"http://example.com/image.png"}}"#;
    assert_that!(
        writer.get_output(),
        equal_to(wrap_boilerplate(
            format!(
                r#"{{"role":"user",_NL_"content":[_NL_{}_NL_]}}"#,
                expected_image_item
            )
            .as_str()
        ))
    );
}

#[test]
fn add_image_by_key() {
    let writer = RcWriter::new();
    let runtime = FfiActorRuntime::new();
    let builder = StructureBuilder::new(writer.clone(), &runtime, create_empty_env_opts());
    let mut builder = builder;

    add_file(String::from("media/image-as-key-1.png"), b"hello".to_vec());

    begin_message(&mut builder, "user");
    builder.begin_item().unwrap();

    builder
        .add_item_attribute(String::from("type"), String::from("image"))
        .unwrap();
    builder
        .add_item_attribute(String::from("content_type"), String::from("image/png"))
        .unwrap();
    builder.image_key("media/image-as-key-1.png").unwrap();

    builder.end_item().unwrap();
    builder.end().unwrap();

    let expected_image_item =
        r#"{"type":"image_url","image_url":{"url":"data:image/png;base64,aGVsbG8="}}"#;
    assert_that!(
        writer.get_output(),
        equal_to(wrap_boilerplate(
            format!(
                r#"{{"role":"user",_NL_"content":[_NL_{}_NL_]}}"#,
                expected_image_item
            )
            .as_str()
        ))
    );
}

#[test]
fn image_as_key_file_not_found() {
    let writer = RcWriter::new();
    let runtime = FfiActorRuntime::new();
    let builder = StructureBuilder::new(writer.clone(), &runtime, create_empty_env_opts());
    let mut builder = builder;

    begin_message(&mut builder, "user");
    builder.begin_item().unwrap();

    builder
        .add_item_attribute(String::from("type"), String::from("image"))
        .unwrap();
    builder
        .add_item_attribute(String::from("content_type"), String::from("image/png"))
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
    let runtime = FfiActorRuntime::new();
    let builder = StructureBuilder::new(writer.clone(), &runtime, create_empty_env_opts());
    let mut builder = builder;

    begin_message(&mut builder, "user");
    builder.begin_item().unwrap();

    builder
        .add_item_attribute(String::from("type"), String::from("image"))
        .unwrap();
    builder
        .add_item_attribute(String::from("detail"), String::from("high"))
        .unwrap();
    builder.begin_image_url().unwrap();
    builder
        .get_writer()
        .write_all(b"http://example.com/image.png")
        .unwrap();
    builder.end_image_url().unwrap();

    builder.end_item().unwrap();
    builder.end().unwrap();

    let expected_image_item = r#"{"type":"image_url","image_url":{"detail":"high","url":"http://example.com/image.png"}}"#;
    assert_that!(
        writer.get_output(),
        equal_to(wrap_boilerplate(
            format!(
                r#"{{"role":"user",_NL_"content":[_NL_{}_NL_]}}"#,
                expected_image_item
            )
            .as_str()
        ))
    );
}

#[test]
fn image_key_with_adversarial_content_type() {
    let writer = RcWriter::new();
    let runtime = FfiActorRuntime::new();
    let builder = StructureBuilder::new(writer.clone(), &runtime, create_empty_env_opts());
    let mut builder = builder;

    add_file(String::from("media/test.png"), b"hello".to_vec());

    begin_message(&mut builder, "user");
    builder.begin_item().unwrap();

    builder
        .add_item_attribute(String::from("type"), String::from("image"))
        .unwrap();
    builder
        .add_item_attribute(
            String::from("content_type"),
            String::from("\"\"image/png\0\\/\"';\u{202E}\u{2028}"),
        )
        .unwrap();
    builder.image_key("media/test.png").unwrap();

    builder.end_item().unwrap();
    builder.end().unwrap();

    // Only escape enough to have a valid json
    let expected_image_item = format!(
        r#"{{"type":"image_url","image_url":{{"url":"data:\"\"image/png\u0000\\/\"';{}{};base64,aGVsbG8="}}}}"#,
        '\u{202E}' as char, '\u{2028}' as char
    );
    assert_that!(
        writer.get_output(),
        equal_to(wrap_boilerplate(
            format!(
                r#"{{"role":"user",_NL_"content":[_NL_{}_NL_]}}"#,
                expected_image_item
            )
            .as_str()
        ))
    );
}

#[test]
fn image_settings_dont_transfer() {
    let writer = RcWriter::new();
    let runtime = FfiActorRuntime::new();
    let builder = StructureBuilder::new(writer.clone(), &runtime, create_empty_env_opts());
    let mut builder = builder;

    begin_message(&mut builder, "user");

    // First image with content_type and detail
    builder.begin_item().unwrap();
    builder
        .add_item_attribute(String::from("type"), String::from("image"))
        .unwrap();
    builder
        .add_item_attribute(String::from("content_type"), String::from("image/png"))
        .unwrap();
    builder
        .add_item_attribute(String::from("detail"), String::from("high"))
        .unwrap();
    builder.begin_image_url().unwrap();
    builder
        .get_writer()
        .write_all(b"http://example.com/image1.png")
        .unwrap();
    builder.end_image_url().unwrap();
    builder.end_item().unwrap();

    // Second image without content_type and detail
    builder.begin_item().unwrap();
    builder
        .add_item_attribute(String::from("type"), String::from("image"))
        .unwrap();
    builder.begin_image_url().unwrap();
    builder
        .get_writer()
        .write_all(b"http://example.com/image2.png")
        .unwrap();
    builder.end_image_url().unwrap();
    builder.end_item().unwrap();

    builder.end().unwrap();

    let expected_image1 = r#"{"type":"image_url","image_url":{"detail":"high","url":"http://example.com/image1.png"}}"#;
    let expected_image2 =
        r#"{"type":"image_url","image_url":{"url":"http://example.com/image2.png"}}"#;

    assert_that!(
        writer.get_output(),
        equal_to(wrap_boilerplate(
            format!(
                r#"{{"role":"user",_NL_"content":[_NL_{},_NL_{}_NL_]}}"#,
                expected_image1, expected_image2
            )
            .as_str()
        ))
    );
}

#[test]
fn mix_text_and_image_content() {
    let writer = RcWriter::new();
    let runtime = FfiActorRuntime::new();
    let builder = StructureBuilder::new(writer.clone(), &runtime, create_empty_env_opts());
    let mut builder = builder;
    begin_message(&mut builder, "user");

    // Text item
    builder.begin_item().unwrap();
    builder
        .add_item_attribute(String::from("type"), String::from("text"))
        .unwrap();
    builder.begin_text().unwrap();
    embedded_io::Write::write_all(builder.get_writer(), b"Hello world").unwrap();
    builder.end_text().unwrap();
    builder.end_item().unwrap();

    // Image item
    builder.begin_item().unwrap();
    builder
        .add_item_attribute(String::from("type"), String::from("image"))
        .unwrap();
    builder.begin_image_url().unwrap();
    builder
        .get_writer()
        .write_all(b"http://example.com/image.png")
        .unwrap();
    builder.end_image_url().unwrap();
    builder.end_item().unwrap();

    // Another text item
    builder.begin_item().unwrap();
    builder
        .add_item_attribute(String::from("type"), String::from("text"))
        .unwrap();
    builder.begin_text().unwrap();
    embedded_io::Write::write_all(builder.get_writer(), b"Another text").unwrap();
    builder.end_text().unwrap();
    builder.end_item().unwrap();

    builder.end().unwrap();

    let text_item1 = r#"{"type":"text","text":"Hello world"}"#;
    let image_item = r#"{"type":"image_url","image_url":{"url":"http://example.com/image.png"}}"#;
    let text_item2 = r#"{"type":"text","text":"Another text"}"#;
    let expected_content = format!(
        r#"_NL_{},_NL_{},_NL_{}_NL_"#,
        text_item1, image_item, text_item2
    );
    let expected_message = format!(r#"{{"role":"user",_NL_"content":[{}]}}"#, expected_content);
    assert_that!(
        writer.get_output(),
        equal_to(wrap_boilerplate(expected_message.as_str()))
    );
}

#[test]
fn function_call() {
    let writer = RcWriter::new();
    let runtime = FfiActorRuntime::new();
    let builder = StructureBuilder::new(writer.clone(), &runtime, create_empty_env_opts());
    let mut builder = builder;

    begin_message(&mut builder, "assistant");
    builder.begin_item().unwrap();
    builder
        .add_item_attribute(String::from("type"), String::from("function"))
        .unwrap();
    builder
        .add_item_attribute(String::from("id"), String::from("id123"))
        .unwrap();
    builder
        .add_item_attribute(String::from("name"), String::from("get_weather"))
        .unwrap();
    builder.begin_function_arguments().unwrap();
    embedded_io::Write::write_all(builder.get_writer(), b"foo,bar").unwrap();
    builder.end_function_arguments().unwrap();
    builder.end_item().unwrap();
    builder.end().unwrap();

    let expected_item = r#"{"role":"assistant",_NL_"tool_calls":[_NL_{"type":"function","id":"id123","function":{"name":"get_weather","arguments":"foo,bar"}}_NL_]}"#;
    assert_that!(
        writer.get_output(),
        equal_to(wrap_boilerplate(expected_item))
    );
}

#[test]
fn function_must_have_name() {
    let writer = RcWriter::new();
    let runtime = FfiActorRuntime::new();
    let builder = StructureBuilder::new(writer.clone(), &runtime, create_empty_env_opts());
    let mut builder = builder;

    begin_message(&mut builder, "assistant");
    builder.begin_item().unwrap();
    builder
        .add_item_attribute(String::from("type"), String::from("function"))
        .unwrap();
    builder
        .add_item_attribute(String::from("id"), String::from("id123"))
        .unwrap();
    let err = builder.begin_function_arguments().unwrap_err();
    assert_that!(
        err,
        equal_to("Missing required 'name' attribute for 'type=function'".to_string())
    );
}

#[test]
fn mix_content_and_tool_calls() {
    let writer = RcWriter::new();
    let runtime = FfiActorRuntime::new();
    let builder = StructureBuilder::new(writer.clone(), &runtime, create_empty_env_opts());
    let mut builder = builder;

    begin_message(&mut builder, "assistant");

    // Two text items (content section)
    builder.begin_item().unwrap();
    builder.begin_text().unwrap();
    builder.get_writer().write_all(b"First text").unwrap();
    builder.end_text().unwrap();
    builder.end_item().unwrap();

    builder.begin_item().unwrap();
    builder.begin_text().unwrap();
    builder.get_writer().write_all(b"Second text").unwrap();
    builder.end_text().unwrap();
    builder.end_item().unwrap();

    // Single function call (tool_calls section)
    builder.begin_item().unwrap();
    builder
        .add_item_attribute(String::from("type"), String::from("function"))
        .unwrap();
    builder
        .add_item_attribute(String::from("name"), String::from("get_weather"))
        .unwrap();
    builder.begin_function_arguments().unwrap();
    builder
        .get_writer()
        .write_all(br#"{\"location\":\"London\"}"#)
        .unwrap();
    builder.end_function_arguments().unwrap();
    builder.end_item().unwrap();

    begin_message(&mut builder, "user");

    // Single text item (content section)
    builder.begin_item().unwrap();
    builder.begin_text().unwrap();
    builder.get_writer().write_all(b"User response").unwrap();
    builder.end_text().unwrap();
    builder.end_item().unwrap();

    // Two function calls (tool_calls section)
    builder.begin_item().unwrap();
    builder
        .add_item_attribute(String::from("type"), String::from("function"))
        .unwrap();
    builder
        .add_item_attribute(String::from("name"), String::from("get_time"))
        .unwrap();
    builder.begin_function_arguments().unwrap();
    builder
        .get_writer()
        .write_all(br#"{\"timezone\":\"UTC\"}"#)
        .unwrap();
    builder.end_function_arguments().unwrap();
    builder.end_item().unwrap();

    builder.begin_item().unwrap();
    builder
        .add_item_attribute(String::from("type"), String::from("function"))
        .unwrap();
    builder
        .add_item_attribute(String::from("name"), String::from("get_date"))
        .unwrap();
    builder.begin_function_arguments().unwrap();
    builder
        .get_writer()
        .write_all(br#"{\"format\":\"ISO\"}"#)
        .unwrap();
    builder.end_function_arguments().unwrap();
    builder.end_item().unwrap();

    builder.end().unwrap();

    let msg1_text1 = r#"{"type":"text","text":"First text"}"#;
    let msg1_text2 = r#"{"type":"text","text":"Second text"}"#;
    let msg1_fn = r#"{"type":"function","function":{"name":"get_weather","arguments":"{\"location\":\"London\"}"}}"#;

    let msg2_text = r#"{"type":"text","text":"User response"}"#;
    let msg2_fn1 = r#"{"type":"function","function":{"name":"get_time","arguments":"{\"timezone\":\"UTC\"}"}}"#;
    let msg2_fn2 =
        r#"{"type":"function","function":{"name":"get_date","arguments":"{\"format\":\"ISO\"}"}}"#;

    let msg1 = format!(
        r#"{{"role":"assistant",_NL_"content":[_NL_{},_NL_{}],_NL_"tool_calls":[_NL_{}_NL_]}}"#,
        msg1_text1, msg1_text2, msg1_fn
    );
    let msg2 = format!(
        r#"{{"role":"user",_NL_"content":[_NL_{}],_NL_"tool_calls":[_NL_{},_NL_{}_NL_]}}"#,
        msg2_text, msg2_fn1, msg2_fn2
    );
    let expected = format!("{},{}", msg1, msg2);
    assert_that!(writer.get_output(), equal_to(wrap_boilerplate(&expected)));
}

#[test]
fn happy_path_toolspecs() {
    let writer = RcWriter::new();
    let runtime = FfiActorRuntime::new();
    let mut builder = StructureBuilder::new(writer.clone(), &runtime, create_empty_env_opts());

    let get_user_name_fn = r#"{
        "name": "get_user_name",
        "description": "Get the user's name. Call this whenever you need to know the name of the user.",
        "strict": true,
        "parameters": {
            "type": "object",
            "properties": {},
            "additionalProperties": false
        }
    }"#;

    let another_function_fn = r#"{
        "name": "another_function", "foo": "bar"
    }"#;

    let data1 = get_user_name_fn.as_bytes();
    let mut cursor1 = data1.as_ref();
    let mut buffer1 = [0u8; 1024];
    let rjiter1 = scan_json::RJiter::new(&mut cursor1, &mut buffer1);
    let mut rjiter1 = rjiter1;

    let data2 = another_function_fn.as_bytes();
    let mut cursor2 = data2.as_ref();
    let mut buffer2 = [0u8; 1024];
    let rjiter2 = scan_json::RJiter::new(&mut cursor2, &mut buffer2);
    let mut rjiter2 = rjiter2;

    //
    // Act
    //
    builder.begin_item().unwrap();
    builder
        .add_item_attribute(String::from("type"), String::from("toolspec"))
        .unwrap();
    builder.toolspec_rjiter(&mut rjiter1).unwrap();
    builder.end_item().unwrap();

    builder.begin_item().unwrap();
    builder
        .add_item_attribute(String::from("type"), String::from("toolspec"))
        .unwrap();
    builder.toolspec_rjiter(&mut rjiter2).unwrap();
    builder.end_item().unwrap();

    begin_message(&mut builder, "user");
    builder.begin_item().unwrap();
    builder
        .add_item_attribute(String::from("type"), String::from("text"))
        .unwrap();
    builder.begin_text().unwrap();
    embedded_io::Write::write_all(builder.get_writer(), b"Hello!").unwrap();
    builder.end_text().unwrap();
    builder.end_item().unwrap();

    builder.end().unwrap();

    //
    // Assert
    //
    let output_json: Value =
        serde_json::from_str(&writer.get_output()).expect("Failed to parse output as JSON");

    let expected_tools = format!(
        r#"[{{
        "type": "function",
        "function": {get_user_name_fn}
    }},
    {{
        "type": "function",
        "function": {another_function_fn}
    }}]"#
    );
    let expected_item =
        format!(r#"{{"role":"user",_NL_"content":[{{"type":"text","text":"Hello!"}}]}}"#);
    let expected = wrap_boilerplate(&expected_item);
    let expected_with_tools = inject_tools(&expected, &expected_tools);
    let expected_json = serde_json::from_str(&expected_with_tools)
        .expect("Failed to parse expected output as JSON");
    assert_that!(output_json, equal_to(expected_json));
}

#[test]
fn toolspec_by_key() {
    let writer = RcWriter::new();
    let runtime = FfiActorRuntime::new();
    let builder = StructureBuilder::new(writer.clone(), &runtime, create_empty_env_opts());
    let mut builder = builder;

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
    add_file(
        String::from("tools/get_user_name.json"),
        toolspec_content.as_bytes().to_vec(),
    );

    let data = toolspec_content.as_bytes();
    let mut cursor = data.as_ref();
    let mut buffer = [0u8; 1024];
    let mut rjiter = scan_json::RJiter::new(&mut cursor, &mut buffer);

    //
    // Act
    //
    builder.begin_item().unwrap();
    builder
        .add_item_attribute(String::from("type"), String::from("toolspec"))
        .unwrap();
    builder.toolspec_rjiter(&mut rjiter).unwrap();
    builder.end_item().unwrap();
    builder.end().unwrap();

    //
    // Assert
    //
    let compact_content = r#"{"name":"get_user_name","description":"Get the user's name. Call this whenever you need to know the name of the user.","strict":true,"parameters":{"type":"object","properties":{},"additionalProperties":false}}"#;
    let expected_toolspec_item = format!(r#"{{"type":"function","function":{}}}"#, compact_content);
    let expected_tools = format!(r#"[{}]"#, expected_toolspec_item);
    let expected = format!(
        r#"{{ "url": "https://api.openai.com/v1/chat/completions",
"method": "POST",
"headers": {{ "Content-type": "application/json", "Authorization": "Bearer {{{{secret}}}}" }},
"body": {{ "model": "gpt-4o-mini", "stream": true,
"tools": {} }}}}
"#,
        expected_tools
    );
    assert_that!(writer.get_output(), equal_to(expected));
}

#[test]
fn toolspec_key_file_not_found() {
    let writer = RcWriter::new();
    let runtime = FfiActorRuntime::new();
    let builder = StructureBuilder::new(writer.clone(), &runtime, create_empty_env_opts());
    let mut builder = builder;

    builder.begin_item().unwrap();
    builder
        .add_item_attribute(String::from("type"), String::from("toolspec"))
        .unwrap();

    let result = builder.toolspec_key("tools/nonexistent.json");

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_that!(
        err.to_string().as_str(),
        matches_regex("tools/nonexistent.json")
    );
}

#[test]
fn toolspec_key_with_bad_json() {
    let writer = RcWriter::new();
    let runtime = FfiActorRuntime::new();
    let builder = StructureBuilder::new(writer.clone(), &runtime, create_empty_env_opts());
    let mut builder = builder;

    // Add a file with invalid JSON content
    let bad_json_content = r#"{
        "name": "get_user_name",
        "description": bad json here.."#;
    add_file(
        String::from("tools/bad_json.json"),
        bad_json_content.as_bytes().to_vec(),
    );

    builder.begin_item().unwrap();
    builder
        .add_item_attribute(String::from("type"), String::from("toolspec"))
        .unwrap();

    let result = builder.toolspec_key("tools/bad_json.json");

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_that!(
        err.to_string().as_str(),
        matches_regex("tools/bad_json.json")
    );
}

#[test]
fn several_toolspecs_to_one_block() {
    let writer = RcWriter::new();
    let runtime = FfiActorRuntime::new();
    let mut builder = StructureBuilder::new(writer.clone(), &runtime, create_empty_env_opts());

    // Simple placeholder toolspecs (valid JSON)
    let toolspec1_content = r#"{"name":"tool1","description":"First tool"}"#;
    let toolspec2_content = r#"{"name":"tool2","description":"Second tool"}"#;
    let toolspec3_content = r#"{"name":"tool3","description":"Third tool"}"#;

    let data1 = toolspec1_content.as_bytes();
    let mut cursor1 = data1.as_ref();
    let mut buffer1 = [0u8; 1024];
    let rjiter1 = scan_json::RJiter::new(&mut cursor1, &mut buffer1);
    let mut rjiter1 = rjiter1;

    let data2 = toolspec2_content.as_bytes();
    let mut cursor2 = data2.as_ref();
    let mut buffer2 = [0u8; 1024];
    let rjiter2 = scan_json::RJiter::new(&mut cursor2, &mut buffer2);
    let mut rjiter2 = rjiter2;

    let data3 = toolspec3_content.as_bytes();
    let mut cursor3 = data3.as_ref();
    let mut buffer3 = [0u8; 1024];
    let rjiter3 = scan_json::RJiter::new(&mut cursor3, &mut buffer3);
    let mut rjiter3 = rjiter3;

    //
    // Act - Add several toolspecs to one block
    //
    builder.begin_item().unwrap();
    builder
        .add_item_attribute(String::from("type"), String::from("toolspec"))
        .unwrap();
    builder.toolspec_rjiter(&mut rjiter1).unwrap();
    builder.end_item().unwrap();

    builder.begin_item().unwrap();
    builder
        .add_item_attribute(String::from("type"), String::from("toolspec"))
        .unwrap();
    builder.toolspec_rjiter(&mut rjiter2).unwrap();
    builder.end_item().unwrap();

    builder.begin_item().unwrap();
    builder
        .add_item_attribute(String::from("type"), String::from("toolspec"))
        .unwrap();
    builder.toolspec_rjiter(&mut rjiter3).unwrap();
    builder.end_item().unwrap();

    builder.end().unwrap();

    //
    // Assert
    //
    let expected_tools = format!(
        r#"[{{"type":"function","function":{}}},
{{"type":"function","function":{}}},
{{"type":"function","function":{}}}]"#,
        toolspec1_content, toolspec2_content, toolspec3_content
    );

    let expected = format!(
        r#"{{ "url": "https://api.openai.com/v1/chat/completions",
"method": "POST",
"headers": {{ "Content-type": "application/json", "Authorization": "Bearer {{{{secret}}}}" }},
"body": {{ "model": "gpt-4o-mini", "stream": true,
"tools": {} }}}}
"#,
        expected_tools
    );
    assert_that!(writer.get_output(), equal_to(expected));
}

#[test]
fn mix_toolspec_and_other_content() {
    let writer = RcWriter::new();
    let runtime = FfiActorRuntime::new();
    let mut builder = StructureBuilder::new(writer.clone(), &runtime, create_empty_env_opts());

    let toolspec1_content = r#"{"name":"tool1","description":"First tool"}"#;
    let toolspec2_content = r#"{"name":"tool2","description":"Second tool"}"#;

    let data1 = toolspec1_content.as_bytes();
    let mut cursor1 = data1.as_ref();
    let mut buffer1 = [0u8; 1024];
    let rjiter1 = scan_json::RJiter::new(&mut cursor1, &mut buffer1);
    let mut rjiter1 = rjiter1;

    let data2 = toolspec2_content.as_bytes();
    let mut cursor2 = data2.as_ref();
    let mut buffer2 = [0u8; 1024];
    let rjiter2 = scan_json::RJiter::new(&mut cursor2, &mut buffer2);
    let mut rjiter2 = rjiter2;

    //
    // Act
    //

    // 1. ctl-item to start a section
    begin_message(&mut builder, "assistant");

    // 2. a toolspec -> it should stop just started section and create "tools"
    builder.begin_item().unwrap();
    builder
        .add_item_attribute(String::from("type"), String::from("toolspec"))
        .unwrap();
    builder.toolspec_rjiter(&mut rjiter1).unwrap();
    builder.end_item().unwrap();

    // 3. some content: it should start "content"
    builder.begin_item().unwrap();
    builder
        .add_item_attribute(String::from("type"), String::from("text"))
        .unwrap();
    builder.begin_text().unwrap();
    embedded_io::Write::write_all(builder.get_writer(), b"Some text content").unwrap();
    builder.end_text().unwrap();
    builder.end_item().unwrap();

    // 4. another toolspec, so that another "tools" will be created
    builder.begin_item().unwrap();
    builder
        .add_item_attribute(String::from("type"), String::from("toolspec"))
        .unwrap();
    builder.toolspec_rjiter(&mut rjiter2).unwrap();
    builder.end_item().unwrap();

    // 5. a tool call item: it should start "tool_calls"
    builder.begin_item().unwrap();
    builder
        .add_item_attribute(String::from("type"), String::from("function"))
        .unwrap();
    builder
        .add_item_attribute(String::from("name"), String::from("get_weather"))
        .unwrap();
    builder.begin_function_arguments().unwrap();
    builder
        .get_writer()
        .write_all(br#"{\"location\":\"London\"}"#)
        .unwrap();
    builder.end_function_arguments().unwrap();
    builder.end_item().unwrap();

    builder.end().unwrap();

    //
    // Assert
    //
    let actual_output = writer.get_output();

    // Get the prefix (everything before "messages") from wrap_boilerplate
    let mut prefix = wrap_boilerplate("").replace(r#""messages": []"#, "");
    if let Some(idx) = prefix.find("\"stream\": true,") {
        prefix = prefix[..idx + "\"stream\": true,".len()].to_string();
    }

    // Build expected output manually by concatenating parts
    let expected_message1 = r#"{"role":"assistant"}"#;
    let expected_tools1 = format!(
        r#"[{{"type":"function","function":{}}}]"#,
        toolspec1_content
    );
    let expected_message2 = format!(
        r#"{{"role":"user",
"content":[
{{"type":"text","text":"Some text content"}}
]}}"#
    );
    let expected_tools2 = format!(
        r#"[{{"type":"function","function":{}}}]"#,
        toolspec2_content
    );
    let expected_message3 = format!(
        r#"{{"role":"user",
"tool_calls":[
{{"type":"function","function":{{"name":"get_weather","arguments":"{{\"location\":\"London\"}}"}}}}
]}}"#
    );

    // Concatenate all parts to build the expected output
    let expected_final = format!(
        "{} \"messages\": [{}],\n\"tools\": {}, \"messages\": [{}],\n\"tools\": {}, \"messages\": [{}]}}}}\n",
        prefix,
        expected_message1,
        expected_tools1,
        expected_message2,
        expected_tools2,
        expected_message3
    );

    assert_that!(actual_output, equal_to(expected_final));
}

#[test]
fn tool_role_requires_tool_call_id() {
    let writer = RcWriter::new();
    let runtime = FfiActorRuntime::new();
    let mut builder = StructureBuilder::new(writer.clone(), &runtime, create_empty_env_opts());

    // Set up item_attr with tool_call_id before begin_message
    builder.begin_item().unwrap();
    builder
        .add_item_attribute(String::from("type"), String::from("ctl"))
        .unwrap();
    builder
        .add_item_attribute(String::from("tool_call_id"), String::from("call_123"))
        .unwrap();
    builder.handle_role("tool").unwrap();
    builder.end_item().unwrap();

    builder.begin_item().unwrap();
    builder
        .add_item_attribute(String::from("type"), String::from("text"))
        .unwrap();
    builder.begin_text().unwrap();
    embedded_io::Write::write_all(builder.get_writer(), b"Tool response content").unwrap();
    builder.end_text().unwrap();
    builder.end_item().unwrap();
    builder.end().unwrap();

    // Verify that the tool_call_id appears in the output
    let expected = wrap_boilerplate(
        r#"{"role":"tool","tool_call_id":"call_123",_NL_"content":[_NL_{"type":"text","text":"Tool response content"}_NL_]}"#,
    );
    assert_that!(writer.get_output(), equal_to(expected));
}

#[test]
fn tool_role_missing_tool_call_id() {
    let writer = RcWriter::new();
    let runtime = FfiActorRuntime::new();
    let mut builder = StructureBuilder::new(writer.clone(), &runtime, create_empty_env_opts());

    // Set up item_attr without tool_call_id for tool role
    builder.begin_item().unwrap();
    builder
        .add_item_attribute(String::from("type"), String::from("ctl"))
        .unwrap();
    // Don't add tool_call_id attribute

    let err = builder.handle_role("tool").unwrap_err();
    assert_that!(
        err,
        equal_to("Missing required 'tool_call_id' attribute for role 'tool'".to_string())
    );
}
