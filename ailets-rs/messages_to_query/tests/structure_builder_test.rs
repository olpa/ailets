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

    builder.start_message().unwrap();
    builder.add_role("user").unwrap();
    builder.start_content().unwrap();
    builder.start_text_item().unwrap();
    builder.add_text("Hello!").unwrap();
    builder.end_text_item().unwrap();
    builder.end_content().unwrap();
    builder.end_message().unwrap();

    assert_that!(
        writer.get_output(),
        equal_to(String::from(
            r#"{"role":"user","content":[{"type":"text","text":"Hello!"}]}"#
        ))
    );
}
