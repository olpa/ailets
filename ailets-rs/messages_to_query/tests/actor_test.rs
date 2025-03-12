#[macro_use]
extern crate hamcrest;
use actor_runtime_mocked::RcWriter;
use hamcrest::prelude::*;
use messages_to_query::_process_query;
use serde_json::Value;
use std::io::Cursor;

fn parse_jsonl(jsonl: &str) -> serde_json::Result<Value> {
    let lines: Vec<_> = jsonl.lines().filter(|line| !line.is_empty()).collect();
    let vec_str = format!("[{}]", lines.join(","));

    serde_json::from_str(&vec_str).map_err(|e| {
        println!("Failed to parse JSON from: {}", vec_str);
        e
    })
}

#[test]
fn test_text_items() {
    let fixture_content = std::fs::read_to_string("tests/fixture/text_items.txt")
        .expect("Failed to read fixture file 'text_items.txt'");
    let reader = Cursor::new(fixture_content.clone());
    let writer = RcWriter::new();

    _process_query(reader, writer.clone()).unwrap();

    let input_json = parse_jsonl(&fixture_content).expect("Failed to parse input as JSON");
    let output_json = parse_jsonl(&writer.get_output()).expect("Failed to parse output as JSON");

    assert_that!(input_json, equal_to(output_json));
}
