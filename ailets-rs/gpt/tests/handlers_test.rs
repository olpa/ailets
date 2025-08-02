use actor_runtime_mocked::RcWriter;
use gpt::handlers::{on_content, on_function_index, on_function_name};
use gpt::structure_builder::StructureBuilder;
use scan_json::{RJiter, StreamOp};
use std::cell::RefCell;
use std::io::{Cursor, Write};
use gpt::fcw_trait::{FunCallResult, FunCallsWrite};

/// Simple wrapper to make Vec<u8> implement FunCallsWrite for basic tests
struct DummyDagWriter(Vec<u8>);

impl DummyDagWriter {
    fn new() -> Self {
        Self(Vec::new())
    }
}

impl Write for DummyDagWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

impl FunCallsWrite for DummyDagWriter {
    fn new_item(&mut self, _id: &str, _name: &str) -> FunCallResult {
        Ok(())
    }

    fn arguments_chunk(&mut self, _chunk: &[u8]) -> FunCallResult {
        Ok(())
    }

    fn end_item(&mut self) -> FunCallResult {
        Ok(())
    }

    fn end(&mut self) -> FunCallResult {
        Ok(())
    }
}

#[test]
fn content_writes_to_builder() {
    // Arrange
    let writer = RcWriter::new();
    let dag_writer = DummyDagWriter::new();
    let builder = StructureBuilder::new(writer.clone(), dag_writer);
    let builder_cell = RefCell::new(builder);

    let mut json_reader = Cursor::new(r#""hello world""#);
    let mut buffer = [0u8; 8];
    let rjiter = RJiter::new(&mut json_reader, &mut buffer);
    let rjiter_cell = RefCell::new(rjiter);

    // Act
    let result = on_content(&rjiter_cell, &builder_cell);

    // Assert
    assert!(matches!(result, StreamOp::ValueIsConsumed));
    let expected = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"text"},{"text":"hello world"#;
    assert_eq!(writer.get_output(), expected);
}

#[test]
fn content_can_be_null() {
    // Arrange
    let writer = RcWriter::new();
    let dag_writer = DummyDagWriter::new();
    let builder = StructureBuilder::new(writer.clone(), dag_writer);
    let builder_cell = RefCell::new(builder);

    let mut json_reader = Cursor::new(r#"null"#);
    let mut buffer = [0u8; 8];
    let rjiter = RJiter::new(&mut json_reader, &mut buffer);
    let rjiter_cell = RefCell::new(rjiter);

    // Act
    let result = on_content(&rjiter_cell, &builder_cell);

    // Assert
    assert!(matches!(result, StreamOp::ValueIsConsumed));
    assert_eq!(writer.get_output(), "");
}

#[test]
fn on_function_string_field_invalid_value_type() {
    // Arrange
    let mut buffer = Cursor::new(Vec::new());
    let dag_writer = DummyDagWriter::new();
    let builder = StructureBuilder::new(&mut buffer, dag_writer);
    let builder_cell = RefCell::new(builder);

    // Arrange: Setup with invalid JSON (number instead of string)
    let json = "true"; // Invalid - should be a string
    let mut json_reader = Cursor::new(json);
    let mut buffer = [0u8; 32];
    let rjiter = RJiter::new(&mut json_reader, &mut buffer);
    let rjiter_cell = RefCell::new(rjiter);

    // Act
    let result = on_function_name(&rjiter_cell, &builder_cell);

    // Assert
    assert!(matches!(result, StreamOp::Error(_)));
}

#[test]
fn on_function_index_invalid_value_type() {
    // Arrange: Setup with invalid JSON (float instead of integer)
    let mut buffer = Cursor::new(Vec::new());
    let dag_writer = DummyDagWriter::new();
    let builder = StructureBuilder::new(&mut buffer, dag_writer);
    let builder_cell = RefCell::new(builder);

    let json = r#"3.14"#; // Invalid - should be an integer
    let mut json_reader = Cursor::new(json);
    let mut buffer = [0u8; 32];
    let rjiter = RJiter::new(&mut json_reader, &mut buffer);
    let rjiter_cell = RefCell::new(rjiter);

    // Act
    let result = on_function_index(&rjiter_cell, &builder_cell);

    // Assert
    assert!(matches!(result, StreamOp::Error(_)));
}
