use actor_runtime_mocked::RcWriter;
use dagops_mock::TrackedDagOps;
use gpt::handlers::{on_content, on_function_index, on_function_name};
use gpt::structure_builder::StructureBuilder;

pub mod dagops_mock;
use scan_json::{RJiter, StreamOp};
use std::cell::RefCell;

#[test]
fn content_writes_to_builder() {
    // Arrange
    let writer = RcWriter::new();
    let tracked_dagops = TrackedDagOps::default();
    let builder = StructureBuilder::new(writer.clone(), tracked_dagops);
    let builder_cell = RefCell::new(builder);

    let json = r#""hello world""#;
    let mut json_reader = json.as_bytes();
    let mut buffer = [0u8; 8];
    let mut rjiter = RJiter::new(&mut json_reader, &mut buffer);

    // Act
    let result = on_content(&mut rjiter, &builder_cell);

    // Assert
    assert!(matches!(result, StreamOp::ValueIsConsumed));
    let expected = "[{\"type\":\"ctl\"},{\"role\":\"assistant\"}]\n[{\"type\":\"text\"},{\"text\":\"hello world";
    assert_eq!(writer.get_output(), expected);
}

#[test]
fn content_can_be_null() {
    // Arrange
    let writer = RcWriter::new();
    let tracked_dagops = TrackedDagOps::default();
    let builder = StructureBuilder::new(writer.clone(), tracked_dagops);
    let builder_cell = RefCell::new(builder);

    let json = r#"null"#;
    let mut json_reader = json.as_bytes();
    let mut buffer = [0u8; 8];
    let mut rjiter = RJiter::new(&mut json_reader, &mut buffer);

    // Act
    let result = on_content(&mut rjiter, &builder_cell);

    // Assert
    assert!(matches!(result, StreamOp::ValueIsConsumed));
    assert_eq!(writer.get_output(), "");
}

#[test]
fn on_function_string_field_invalid_value_type() {
    // Arrange
    let buffer = RcWriter::new();
    let tracked_dagops = TrackedDagOps::default();
    let builder = StructureBuilder::new(buffer.clone(), tracked_dagops);
    let builder_cell = RefCell::new(builder);

    // Arrange: Setup with invalid JSON (number instead of string)
    let json = "true"; // Invalid - should be a string
    let mut json_reader = json.as_bytes();
    let mut buffer = [0u8; 32];
    let mut rjiter = RJiter::new(&mut json_reader, &mut buffer);

    // Act
    let result = on_function_name(&mut rjiter, &builder_cell);

    // Assert
    assert!(matches!(result, StreamOp::Error(_)));
}

#[test]
fn on_function_index_invalid_value_type() {
    // Arrange: Setup with invalid JSON (float instead of integer)
    let buffer = RcWriter::new();
    let tracked_dagops = TrackedDagOps::default();
    let builder = StructureBuilder::new(buffer.clone(), tracked_dagops);
    let builder_cell = RefCell::new(builder);

    let json = r#"3.14"#; // Invalid - should be an integer
    let mut json_reader = json.as_bytes();
    let mut buffer = [0u8; 32];
    let mut rjiter = RJiter::new(&mut json_reader, &mut buffer);

    // Act
    let result = on_function_index(&mut rjiter, &builder_cell);

    // Assert
    assert!(matches!(result, StreamOp::Error(_)));
}
