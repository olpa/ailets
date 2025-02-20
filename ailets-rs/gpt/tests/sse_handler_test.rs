use std::cell::RefCell;
use std::io;

use gpt::structure_builder::StructureBuilder;
use gpt::{on_content, on_role};
use scan_json::RJiter;

#[test]
fn basic_pass() {
    // Arrange
    let input = r#""assistant""hello""#;
    let mut buffer = vec![0u8; 16];
    let mut cursor = io::Cursor::new(input);
    let rjiter = RJiter::new(&mut cursor, &mut buffer);
    let rjiter_cell = RefCell::new(rjiter);
    let builder = StructureBuilder::new(Vec::new());
    let builder_cell = RefCell::new(builder);

    // Act
    on_role(&rjiter_cell, &builder_cell);
    on_content(&rjiter_cell, &builder_cell);
    builder_cell.borrow_mut().end_message();

    // Assert
    let expected =
        r#"{"role":"assistant","content":[{"type":"text","text":"hello"}]}"#.to_owned() + "\n";
    assert_eq!(get_output(&builder_cell), expected);
}

#[test]
fn join_multiple_content_deltas() {
    // Arrange
    let input = r#""role""Hello"" world""!""#;
    let mut buffer = vec![0u8; 16];
    let mut cursor = io::Cursor::new(input);
    let rjiter = RJiter::new(&mut cursor, &mut buffer);
    let builder = StructureBuilder::new(Vec::new());
    let rjiter_cell = RefCell::new(rjiter);
    let builder_cell = RefCell::new(builder);

    // Act
    on_role(&rjiter_cell, &builder_cell);
    on_content(&rjiter_cell, &builder_cell);
    on_content(&rjiter_cell, &builder_cell);
    on_content(&rjiter_cell, &builder_cell);
    builder_cell.borrow_mut().end_message();

    // Assert
    let expected =
        r#"{"role":"role","content":[{"type":"text","text":"Hello world!"}]}"#.to_owned() + "\n";
    assert_eq!(get_output(&builder_cell), expected);
}

#[test]
fn ignore_additional_role() {
    // Arrange
    let input = r#""a1""a2""a3""#;
    let mut buffer = vec![0u8; 16];
    let mut cursor = io::Cursor::new(input);
    let rjiter = RJiter::new(&mut cursor, &mut buffer);
    let builder = StructureBuilder::new(Vec::new());
    let rjiter_cell = RefCell::new(rjiter);
    let builder_cell = RefCell::new(builder);

    // Act
    on_role(&rjiter_cell, &builder_cell);
    on_role(&rjiter_cell, &builder_cell);
    on_role(&rjiter_cell, &builder_cell);
    builder_cell.borrow_mut().end_message();

    // Assert
    let expected = r#"{"role":"a1","content":[]}"#.to_owned() + "\n";
    assert_eq!(get_output(&builder_cell), expected);
}

#[test]
fn create_message_without_input_role() {
    // Arrange
    let input = r#""hello""#;
    let mut buffer = vec![0u8; 16];
    let mut cursor = io::Cursor::new(input);
    let rjiter = RJiter::new(&mut cursor, &mut buffer);
    let builder = StructureBuilder::new(Vec::new());
    let rjiter_cell = RefCell::new(rjiter);
    let builder_cell = RefCell::new(builder);

    // Act
    on_content(&rjiter_cell, &builder_cell);
    builder_cell.borrow_mut().end_message();

    // Assert
    let expected =
        r#"{"role":"assistant","content":[{"type":"text","text":"hello"}]}"#.to_owned() + "\n";
    assert_eq!(get_output(&builder_cell), expected);
}

#[test]
fn can_call_end_message_multiple_times() {
    // Arrange
    let input = r#""hello""#;
    let mut buffer = vec![0u8; 16];
    let mut cursor = io::Cursor::new(input);
    let rjiter = RJiter::new(&mut cursor, &mut buffer);
    let builder = StructureBuilder::new(Vec::new());
    let rjiter_cell = RefCell::new(rjiter);
    let builder_cell = RefCell::new(builder);

    // Act
    on_content(&rjiter_cell, &builder_cell);
    builder_cell.borrow_mut().end_message();
    builder_cell.borrow_mut().end_message();
    builder_cell.borrow_mut().end_message();

    // Assert
    let expected =
        r#"{"role":"assistant","content":[{"type":"text","text":"hello"}]}"#.to_owned() + "\n";
    assert_eq!(get_output(&builder_cell), expected);
}

fn get_output(builder_cell: &RefCell<StructureBuilder<Vec<u8>>>) -> String {
    let builder = builder_cell.borrow();
    let writer = builder.get_writer();
    String::from_utf8_lossy(writer).to_string()
}
