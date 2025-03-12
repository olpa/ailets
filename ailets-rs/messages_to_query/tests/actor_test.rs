#[macro_use]
extern crate hamcrest;
use actor_runtime_mocked::RcWriter;
use hamcrest::prelude::*;
use messages_to_query::_process_query;
use serde_json::Value;
use std::io::Cursor;

#[test]
fn test_text_items() {
    let fixture_content = std::fs::read_to_string("tests/fixture/text_items.txt")
        .expect("Failed to read fixture file 'text_items.txt'");
    let reader = Cursor::new(fixture_content.clone());
    let writer = RcWriter::new();

    _process_query(reader, writer.clone()).unwrap();

    let input_json: Value =
        serde_json::from_str(&fixture_content).expect("Failed to parse input as JSON");
    let output_json: Value =
        serde_json::from_str(&writer.get_output()).expect("Failed to parse output as JSON");

    assert_that!(input_json, equal_to(output_json));
}
