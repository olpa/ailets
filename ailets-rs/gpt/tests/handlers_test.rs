use actor_runtime_mocked::RcWriter;
use gpt::funcalls::ContentItemFunction;
use gpt::handlers::{on_content, on_function_begin, on_function_name};
use gpt::structure_builder::StructureBuilder;
use scan_json::{RJiter, StreamOp};
use std::cell::RefCell;
use std::io::Cursor;

#[test]
fn content_writes_to_builder() {
    // Arrange
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone());
    let builder_cell = RefCell::new(builder);

    let mut json_reader = Cursor::new(r#""hello world""#);
    let mut buffer = [0u8; 8];
    let rjiter = RJiter::new(&mut json_reader, &mut buffer);
    let rjiter_cell = RefCell::new(rjiter);

    // Act
    let result = on_content(&rjiter_cell, &builder_cell);

    // Assert
    assert!(matches!(result, StreamOp::ValueIsConsumed));
    let expected = r#"{"role":"assistant","content":[{"type":"text","text":"hello world"#;
    assert_eq!(writer.get_output(), expected);
}

#[test]
fn content_can_be_null() {
    // Arrange
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone());
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
fn on_function_string_field() {
    // Arrange
    let mut buffer = Cursor::new(Vec::new());
    let builder = StructureBuilder::new(&mut buffer);
    let builder_cell = RefCell::new(builder);

    // Arrange: Create RJiter with a test string
    let json = r#""test_function""#;
    let mut json_reader = Cursor::new(json);
    let mut buffer = [0u8; 32];
    let rjiter = RJiter::new(&mut json_reader, &mut buffer);
    let rjiter_cell = RefCell::new(rjiter);

    // Act
    on_function_begin(&rjiter_cell, &builder_cell);
    let result = on_function_name(&rjiter_cell, &builder_cell);
    assert!(matches!(result, StreamOp::ValueIsConsumed));

    // Assert: Verify that the function name was set in FunCalls
    let builder = builder_cell.borrow();
    let funcalls = builder.get_funcalls();
    assert_eq!(
        funcalls.get_tool_calls(),
        &[ContentItemFunction::new("", "test_function", "")]
    );
}

#[test]
fn on_function_string_field_invalid_value_type() {
    // Arrange
    let mut buffer = Cursor::new(Vec::new());
    let builder = StructureBuilder::new(&mut buffer);
    let builder_cell = RefCell::new(builder);

    // Arrange: Setup with invalid JSON (number instead of string)
    let json = "true"; // Invalid - should be a string
    let mut json_reader = Cursor::new(json);
    let mut buffer = [0u8; 32];
    let rjiter = RJiter::new(&mut json_reader, &mut buffer);
    let rjiter_cell = RefCell::new(rjiter);

    // Act
    on_function_begin(&rjiter_cell, &builder_cell);
    let result = on_function_name(&rjiter_cell, &builder_cell);

    // Assert
    assert!(matches!(result, StreamOp::Error(_)));
    let builder = builder_cell.borrow();
    let funcalls = builder.get_funcalls();
    assert_eq!(
        funcalls.get_tool_calls(),
        &[ContentItemFunction::new("", "", "")]
    );
}

#[test]
fn on_function_index() {
    // Arrange
    let mut buffer = Cursor::new(Vec::new());
    let builder = StructureBuilder::new(&mut buffer);
    let builder_cell = RefCell::new(builder);

    // Arrange: Create RJiter with input for 3 tests
    let json = "2 2 2";
    let rjiter = RJiter::new(json.as_bytes());
    let rjiter_cell = RefCell::new(rjiter);

    // Act and assert: Out of range
    let result = on_function_index(&rjiter_cell, &builder_cell);
    println!("result: {:?}", result); // FIXME
    assert!(matches!(result, StreamOp::Error(_)));

    // Act and assert: Valid index
    on_function_begin(&rjiter_cell, &builder_cell);
    on_function_begin(&rjiter_cell, &builder_cell);
    on_function_begin(&rjiter_cell, &builder_cell);
    let result = on_function_index(&rjiter_cell, &builder_cell);
    println!("result: {:?}", result); // FIXME
    assert!(matches!(result, StreamOp::ValueIsConsumed));

    // Act and assert: Index mismatch
    builder_cell
        .borrow_mut()
        .get_funcalls_mut()
        .start_delta_round();
    let result = on_function_index(&rjiter_cell, &builder_cell);
    println!("result: {:?}", result); // FIXME
    assert!(matches!(result, StreamOp::Error(_)));
}

#[test]
fn on_function_index_invalid_value_type() {
    // Arrange: Setup with invalid JSON (float instead of integer)
    let mut buffer = Cursor::new(Vec::new());
    let builder = StructureBuilder::new(&mut buffer);
    let builder_cell = RefCell::new(builder);

    let json = r#"3.14"#; // Invalid - should be an integer
    let rjiter = RJiter::new(json.as_bytes());
    let rjiter_cell = RefCell::new(rjiter);

    // Act
    let result = on_function_index(&rjiter_cell, &builder_cell);

    // Assert
    assert!(matches!(result, StreamOp::Error(_)));
}
