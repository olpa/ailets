use std::cell::RefCell;
use std::io;

mod mocked_node_runtime;
use mocked_node_runtime::{clear_mocks, get_output};

use gpt::awriter::AWriter;
use gpt::rjiter::RJiter;
use gpt::sse_handler::{on_delta_content, on_delta_role, SSEHandler};

#[test]
fn basic_pass() {
    // Arrange
    clear_mocks();
    let input = r#""assistant""hello""#;
    let mut buffer = vec![0u8; 16];
    let mut cursor = io::Cursor::new(input);
    let rjiter = RJiter::new(&mut cursor, &mut buffer);
    let awriter = AWriter::new("");
    let rjiter_cell = RefCell::new(rjiter);
    let handler = SSEHandler::new(RefCell::new(awriter));
    let handler_cell = RefCell::new(handler);

    // Act
    on_delta_role(&rjiter_cell, &handler_cell);
    on_delta_content(&rjiter_cell, &handler_cell);
    handler_cell.borrow_mut().end();

    // Assert
    let expected =
        r#"{"role":"assistant","content":[{"type":"text","text":"hello"}]}"#.to_owned() + "\n";
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
    let handler = SSEHandler::new(RefCell::new(awriter));
    let handler_cell = RefCell::new(handler);

    // Act
    on_delta_role(&rjiter_cell, &handler_cell);
    on_delta_role(&rjiter_cell, &handler_cell);
    on_delta_role(&rjiter_cell, &handler_cell);
    handler_cell.borrow_mut().end();

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
    let handler = SSEHandler::new(RefCell::new(awriter));
    let handler_cell = RefCell::new(handler);

    // Act
    on_delta_content(&rjiter_cell, &handler_cell);
    handler_cell.borrow_mut().end();

    // Assert
    let expected =
        r#"{"role":"assistant","content":[{"type":"text","text":"hello"}]}"#.to_owned() + "\n";
    assert_eq!(get_output(), expected);
}
