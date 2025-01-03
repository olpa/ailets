use std::cell::RefCell;
use std::io;

mod mocked_node_runtime;
use mocked_node_runtime::{clear_mocks, get_output};

use gpt::awriter::AWriter;
use gpt::rjiter::RJiter;
use gpt::{on_content, on_role};

#[test]
fn basic_pass() {
    // Arrange
    clear_mocks();
    let input = r#""assistant""hello""#;
    let mut buffer = vec![0u8; 16];
    let mut cursor = io::Cursor::new(input);
    let rjiter = RJiter::new(&mut cursor, &mut buffer);
    let rjiter_cell = RefCell::new(rjiter);
    let awriter = AWriter::new("");
    let awriter_cell = RefCell::new(awriter);

    // Act
    on_role(&rjiter_cell, &awriter_cell);
    on_content(&rjiter_cell, &awriter_cell);
    awriter_cell.borrow_mut().end_message();

    // Assert
    let expected =
        r#"{"role":"assistant","content":[{"type":"text","text":"hello"}]}"#.to_owned() + "\n";
    assert_eq!(get_output(), expected);
}

#[test]
fn join_multiple_content_deltas() {
    // Arrange
    clear_mocks();
    let input = r#""role""Hello"" world""!""#;
    let mut buffer = vec![0u8; 16];
    let mut cursor = io::Cursor::new(input);
    let rjiter = RJiter::new(&mut cursor, &mut buffer);
    let awriter = AWriter::new("");
    let rjiter_cell = RefCell::new(rjiter);
    let awriter_cell = RefCell::new(awriter);

    // Act
    on_role(&rjiter_cell, &awriter_cell);
    on_content(&rjiter_cell, &awriter_cell);
    on_content(&rjiter_cell, &awriter_cell);
    on_content(&rjiter_cell, &awriter_cell);
    awriter_cell.borrow_mut().end_message();

    // Assert
    let expected =
        r#"{"role":"role","content":[{"type":"text","text":"Hello world!"}]}"#.to_owned() + "\n";
    assert_eq!(get_output(), expected);
}

#[test]
fn ignore_additional_role() {
    // Arrange
    clear_mocks();
    let input = r#""a1""a2""a3""#;
    let mut buffer = vec![0u8; 16];
    let mut cursor = io::Cursor::new(input);
    let rjiter = RJiter::new(&mut cursor, &mut buffer);
    let awriter = AWriter::new("");
    let rjiter_cell = RefCell::new(rjiter);
    let awriter_cell = RefCell::new(awriter);

    // Act
    on_role(&rjiter_cell, &awriter_cell);
    on_role(&rjiter_cell, &awriter_cell);
    on_role(&rjiter_cell, &awriter_cell);
    awriter_cell.borrow_mut().end_message();

    // Assert
    let expected = r#"{"role":"a1","content":[]}"#.to_owned() + "\n";
    assert_eq!(get_output(), expected);
}

#[test]
fn create_message_without_input_role() {
    // Arrange
    clear_mocks();
    let input = r#""hello""#;
    let mut buffer = vec![0u8; 16];
    let mut cursor = io::Cursor::new(input);
    let rjiter = RJiter::new(&mut cursor, &mut buffer);
    let awriter = AWriter::new("");
    let rjiter_cell = RefCell::new(rjiter);
    let awriter_cell = RefCell::new(awriter);

    // Act
    on_content(&rjiter_cell, &awriter_cell);
    awriter_cell.borrow_mut().end_message();

    // Assert
    let expected =
        r#"{"role":"assistant","content":[{"type":"text","text":"hello"}]}"#.to_owned() + "\n";
    assert_eq!(get_output(), expected);
}

#[test]
fn can_call_end_message_multiple_times() {
    // Arrange
    clear_mocks();
    let input = r#""hello""#;
    let mut buffer = vec![0u8; 16];
    let mut cursor = io::Cursor::new(input);
    let rjiter = RJiter::new(&mut cursor, &mut buffer);
    let awriter = AWriter::new("");
    let rjiter_cell = RefCell::new(rjiter);
    let awriter_cell = RefCell::new(awriter);

    // Act
    on_content(&rjiter_cell, &awriter_cell);
    awriter_cell.borrow_mut().end_message();
    awriter_cell.borrow_mut().end_message();
    awriter_cell.borrow_mut().end_message();

    // Assert
    let expected =
        r#"{"role":"assistant","content":[{"type":"text","text":"hello"}]}"#.to_owned() + "\n";
    assert_eq!(get_output(), expected);
}
