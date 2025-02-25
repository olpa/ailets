use std::cell::RefCell;
use std::io;
use std::io::Write;

use actor_runtime_mocked::RcWriter;
use gpt::handlers::{on_content, on_role};
use gpt::structure_builder::StructureBuilder;
use scan_json::RJiter;

#[test]
fn basic_pass() {
    // Arrange
    let mut writer = RcWriter::new();
    let mut builder = StructureBuilder::new(writer.clone());

    // Act
    builder.begin_message();
    builder.role("assistant").unwrap();
    builder.begin_text_chunk().unwrap();
    writer.write_all(b"hello").unwrap();
    builder.end_message().unwrap();

    // Assert
    let expected =
        r#"{"role":"assistant","content":[{"type":"text","text":"hello"}]}"#.to_owned() + "\n";
    assert_eq!(writer.get_output(), expected);
}

#[test]
fn join_multiple_content_deltas() {
    // Arrange
    let input = r#""role""Hello"" world""!""#;
    let mut buffer = vec![0u8; 16];
    let mut cursor = io::Cursor::new(input);
    let rjiter = RJiter::new(&mut cursor, &mut buffer);
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone());
    let rjiter_cell = RefCell::new(rjiter);
    let builder_cell = RefCell::new(builder);

    // Act
    on_role(&rjiter_cell, &builder_cell);
    on_content(&rjiter_cell, &builder_cell);
    on_content(&rjiter_cell, &builder_cell);
    on_content(&rjiter_cell, &builder_cell);
    builder_cell.borrow_mut().end_message().unwrap();

    // Assert
    let expected =
        r#"{"role":"role","content":[{"type":"text","text":"Hello world!"}]}"#.to_owned() + "\n";
    assert_eq!(writer.get_output(), expected);
}

#[test]
fn ignore_additional_role() {
    // Arrange
    let input = r#""a1""a2""a3""#;
    let mut buffer = vec![0u8; 16];
    let mut cursor = io::Cursor::new(input);
    let rjiter = RJiter::new(&mut cursor, &mut buffer);
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone());
    let rjiter_cell = RefCell::new(rjiter);
    let builder_cell = RefCell::new(builder);

    // Act
    on_role(&rjiter_cell, &builder_cell);
    on_role(&rjiter_cell, &builder_cell);
    on_role(&rjiter_cell, &builder_cell);
    builder_cell.borrow_mut().end_message().unwrap();

    // Assert
    let expected = r#"{"role":"a1","content":[]}"#.to_owned() + "\n";
    assert_eq!(writer.get_output(), expected);
}

#[test]
fn create_message_without_input_role() {
    // Arrange
    let input = r#""hello""#;
    let mut buffer = vec![0u8; 16];
    let mut cursor = io::Cursor::new(input);
    let rjiter = RJiter::new(&mut cursor, &mut buffer);
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone());
    let rjiter_cell = RefCell::new(rjiter);
    let builder_cell = RefCell::new(builder);

    // Act
    on_content(&rjiter_cell, &builder_cell);
    builder_cell.borrow_mut().end_message().unwrap();

    // Assert
    let expected =
        r#"{"role":"assistant","content":[{"type":"text","text":"hello"}]}"#.to_owned() + "\n";
    assert_eq!(writer.get_output(), expected);
}

#[test]
fn can_call_end_message_multiple_times() {
    // Arrange
    let input = r#""hello""#;
    let mut buffer = vec![0u8; 16];
    let mut cursor = io::Cursor::new(input);
    let rjiter = RJiter::new(&mut cursor, &mut buffer);
    let writer = RcWriter::new();
    let builder = StructureBuilder::new(writer.clone());
    let rjiter_cell = RefCell::new(rjiter);
    let builder_cell = RefCell::new(builder);

    // Act
    on_content(&rjiter_cell, &builder_cell);
    builder_cell.borrow_mut().end_message().unwrap();
    builder_cell.borrow_mut().end_message().unwrap();
    builder_cell.borrow_mut().end_message().unwrap();

    // Assert
    let expected =
        r#"{"role":"assistant","content":[{"type":"text","text":"hello"}]}"#.to_owned() + "\n";
    assert_eq!(writer.get_output(), expected);
}
