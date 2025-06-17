#[macro_use]
extern crate hamcrest;
use actor_runtime_mocked::add_file;
use actor_runtime_mocked::RcWriter;
use hamcrest::prelude::*;
use messages_to_query::env_opts::EnvOpts;
use messages_to_query::structure_builder::StructureBuilder;
use std::collections::HashMap;
use std::io::Write;

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
    let builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());
    let mut builder = builder;

    begin_message(&mut builder, "user");
    builder.begin_item().unwrap();
    builder
        .add_item_attribute(String::from("type"), String::from("text"))
        .unwrap();
    builder.begin_text().unwrap();
    write!(builder.get_writer(), "Hello!").unwrap();
    builder.end_text().unwrap();
    builder.end_item().unwrap();
    builder.end().unwrap();

    assert_that!(
        writer.get_output(),
        equal_to(wrap_boilerplate(
            r#"{"role":"user","content":[_NL_{"type":"text","text":"Hello!"}_NL_]}"#,
            None
        ))
    );
}

#[test]
fn many_messages_and_items() {
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());
    let mut builder = builder;

    begin_message(&mut builder, "user");
    builder.begin_item().unwrap();
    builder.begin_text().unwrap();
    write!(builder.get_writer(), "Text item of the first message").unwrap();
    builder.end_text().unwrap();
    builder.end_item().unwrap();

    begin_message(&mut builder, "assistant");
    builder.begin_item().unwrap();
    builder.begin_text().unwrap();
    write!(builder.get_writer(), "First item of the second message").unwrap();
    builder.end_text().unwrap();
    builder.end_item().unwrap();
    builder.begin_item().unwrap();
    builder.begin_text().unwrap();
    write!(builder.get_writer(), "Second item of the second message").unwrap();
    builder.end_text().unwrap();
    builder.end_item().unwrap();
    builder.end().unwrap();

    let text_item1 = r#"{"type":"text","text":"Text item of the first message"}"#;
    let text_item2a = r#"{"type":"text","text":"First item of the second message"}"#;
    let text_item2b = r#"{"type":"text","text":"Second item of the second message"}"#;
    let expected = String::from(
            r#"{"role":"user","content":[_NL__TI1__NL_]},{"role":"assistant","content":[_NL__TI2a_,_NL__TI2b__NL_]}"#
        ).replace("_TI1_", text_item1).replace("_TI2a_", text_item2a).replace("_TI2b_", text_item2b);
    let expected = wrap_boilerplate(expected.as_str(), None);
    assert_that!(writer.get_output(), equal_to(expected));
}

#[test]
fn skip_empty_items_but_create_content_wrapper() {
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());
    let mut builder = builder;

    begin_message(&mut builder, "user");
    builder.begin_item().unwrap();
    builder.end_item().unwrap();

    begin_message(&mut builder, "user");
    builder.begin_item().unwrap();
    builder.end_item().unwrap();
    builder.begin_item().unwrap();
    builder.end_item().unwrap();
    builder.end().unwrap();

    let empty_msg = "{\"role\":\"user\",\"content\":[]}".to_owned();
    let two_empty_msgs = wrap_boilerplate(format!("{},{}", empty_msg, empty_msg).as_str(), None);
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

    let msg_user = r#"{"role":"user","content":[]}"#;
    let msg_assistant = r#"{"role":"assistant","content":[]}"#;
    let msg_tool = r#"{"role":"tool","content":[]}"#;
    let msg_user2 = r#"{"role":"user","content":[]}"#;
    let expected = wrap_boilerplate(
        &format!("{},{},{},{}", msg_user, msg_assistant, msg_user2, msg_tool),
        None,
    );
    assert_that!(writer.get_output(), equal_to(expected));
}

#[test]
fn reject_role_for_non_ctl_type() {
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());
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
    let builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());
    let mut builder = builder;
    begin_message(&mut builder, "user");

    builder.begin_item().unwrap();
    builder.begin_text().unwrap();
    write!(builder.get_writer(), "hello").unwrap();
    builder.end_text().unwrap();
    builder.end_item().unwrap();
    builder.end().unwrap();

    let expected = wrap_boilerplate(
        r#"{"role":"user","content":[_NL_{"type":"text","text":"hello"}_NL_]}"#,
        None,
    );
    assert_that!(writer.get_output(), equal_to(expected));
}

#[test]
fn reject_unknown_type() {
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());
    let mut builder = builder;
    begin_message(&mut builder, "user");

    builder.begin_item().unwrap();
    let err = builder
        .add_item_attribute(String::from("type"), String::from("unknown"))
        .unwrap_err();
    assert_that!(
        err,
        equal_to(
            "Invalid type value: 'unknown'. Allowed values are: text, image, function, ctl"
                .to_string()
        )
    );
}

#[test]
fn reject_conflicting_type() {
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());
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
    let builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());
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
            r#"{{"role":"user","content":[_NL_{{"type":"text","text":"{}"}}_NL_]}}"#,
            special_chars
        )
        .as_str(),
        None,
    );
    assert_that!(writer.get_output(), equal_to(expected));
}

#[test]
fn pass_preceding_attributes_to_text_output() {
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());
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
    write!(builder.get_writer(), "Hello world").unwrap();
    builder.end_text().unwrap();
    builder.end_item().unwrap();
    builder.end().unwrap();

    let expected_text_item = r#"{"type":"text","custom_attr_1":"value_1","custom_attr_2":"value_2","text":"Hello world"}"#;
    assert_that!(
        writer.get_output(),
        equal_to(wrap_boilerplate(
            &format!(
                r#"{{"role":"user","content":[_NL_{}_NL_]}}"#,
                expected_text_item
            ),
            None
        ))
    );
}

#[test]
fn pass_following_attributes_to_text_output() {
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());
    let mut builder = builder;

    begin_message(&mut builder, "user");
    builder.begin_item().unwrap();
    builder.begin_text().unwrap();
    write!(builder.get_writer(), "Hello world").unwrap();
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
            &format!(
                r#"{{"role":"user","content":[_NL_{}_NL_]}}"#,
                expected_text_item
            ),
            None
        ))
    );
}

#[test]
fn add_image_by_url() {
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());
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
            &format!(
                r#"{{"role":"user","content":[_NL_{}_NL_]}}"#,
                expected_image_item
            ),
            None
        ))
    );
}

#[test]
fn add_image_by_key() {
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());
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
            &format!(
                r#"{{"role":"user","content":[_NL_{}_NL_]}}"#,
                expected_image_item
            ),
            None
        ))
    );
}

#[test]
fn image_as_key_file_not_found() {
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());
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
    let builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());
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
            &format!(
                r#"{{"role":"user","content":[_NL_{}_NL_]}}"#,
                expected_image_item
            ),
            None
        ))
    );
}

#[test]
fn image_key_with_adversarial_content_type() {
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());
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
            &format!(
                r#"{{"role":"user","content":[_NL_{}_NL_]}}"#,
                expected_image_item
            ),
            None
        ))
    );
}

#[test]
fn image_settings_dont_transfer() {
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());
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
            &format!(
                r#"{{"role":"user","content":[_NL_{},_NL_{}_NL_]}}"#,
                expected_image1, expected_image2
            ),
            None
        ))
    );
}

#[test]
fn mix_text_and_image_content() {
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());
    let mut builder = builder;
    begin_message(&mut builder, "user");

    // Text item
    builder.begin_item().unwrap();
    builder
        .add_item_attribute(String::from("type"), String::from("text"))
        .unwrap();
    builder.begin_text().unwrap();
    write!(builder.get_writer(), "Hello world").unwrap();
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
    write!(builder.get_writer(), "Another text").unwrap();
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
    let expected_message = format!(r#"{{"role":"user","content":[{}]}}"#, expected_content);
    assert_that!(
        writer.get_output(),
        equal_to(wrap_boilerplate(&expected_message, None))
    );
}

#[test]
fn function_call() {
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());
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
    write!(builder.get_writer(), "foo,bar").unwrap();
    builder.end_function_arguments().unwrap();
    builder.end_item().unwrap();
    builder.end().unwrap();

    let expected_item = r#"{"role":"assistant","tool_calls":[_NL_{"type":"function","id":"id123","function":{"name":"get_weather","arguments":"foo,bar"}}_NL_]}"#;
    assert_that!(
        writer.get_output(),
        equal_to(wrap_boilerplate(expected_item, None))
    );
}

#[test]
fn function_must_have_name() {
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());
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
    let builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());
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
        r#"{{"role":"assistant","content":[_NL_{},_NL_{}],"tool_calls":[_NL_{}_NL_]}}"#,
        msg1_text1, msg1_text2, msg1_fn
    );
    let msg2 = format!(
        r#"{{"role":"user","content":[_NL_{}],"tool_calls":[_NL_{},_NL_{}_NL_]}}"#,
        msg2_text, msg2_fn1, msg2_fn2
    );
    let expected = format!("{},{}", msg1, msg2);
    assert_that!(
        writer.get_output(),
        equal_to(wrap_boilerplate(&expected, None))
    );
}

#[test]
fn happy_path_toolspecs() {
    let writer = RcWriter::new();
    let mut builder = StructureBuilder::new(writer.clone(), create_empty_env_opts());

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

    begin_message(&mut builder, "user");

    builder.begin_item().unwrap();
    builder
        .add_item_attribute(String::from("type"), String::from("toolspecs"))
        .unwrap();
    builder.begin_text().unwrap();
    builder
        .get_writer()
        .write_all(format!("{}\n{}", get_user_name_fn, another_function_fn).as_bytes())
        .unwrap();
    builder.end_text().unwrap();
    builder.end_item().unwrap();

    builder.begin_item().unwrap();
    builder
        .add_item_attribute(String::from("type"), String::from("text"))
        .unwrap();
    builder.begin_text().unwrap();
    write!(builder.get_writer(), "Hello!").unwrap();
    builder.end_text().unwrap();
    builder.end_item().unwrap();

    builder.end().unwrap();

    let expected_item = r#"[{"role":"user","content":[{"type":"text","text":"Hello!"}]}]"#;
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
    let expected = wrap_boilerplate(&expected_item, Some(&expected_tools));
    assert_that!(writer.get_output(), equal_to(expected));
}
