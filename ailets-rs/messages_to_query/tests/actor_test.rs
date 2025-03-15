#[macro_use]
extern crate hamcrest;
use actor_runtime_mocked::RcWriter;
use hamcrest::prelude::*;
use messages_to_query::_process_query;
use serde_json::Value;
use std::io::Cursor;

fn fix_json(json: &str) -> String {
    if json.contains("}\n{") {
        let json = json.replace("}\n{", "},\n{");
        let json = format!("[{}]", json);
        return json;
    }
    json.to_string()
}

#[test]
fn test_text_items() {
    let fixture_content = std::fs::read_to_string("tests/fixture/text_items.txt")
        .expect("Failed to read fixture file 'text_items.txt'");
    let reader = Cursor::new(fixture_content.clone());
    let writer = RcWriter::new();

    _process_query(reader, writer.clone()).unwrap();

    let input_json: Value = serde_json::from_str(fix_json(&fixture_content).as_str())
        .expect("Failed to parse input as JSON");
    println!("output: {}", writer.get_output()); // FIXME
    let output_json: Value = serde_json::from_str(fix_json(&writer.get_output()).as_str())
        .expect("Failed to parse output as JSON");

    assert_that!(output_json, equal_to(input_json));
}
