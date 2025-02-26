use actor_runtime_mocked::RcWriter;
use gpt::handlers::on_content;
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
