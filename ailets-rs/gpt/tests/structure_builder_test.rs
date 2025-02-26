use std::io::Write;

use actor_runtime_mocked::RcWriter;
use gpt::structure_builder::StructureBuilder;

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
fn ignore_additional_role() {
    // Arrange
    let writer = RcWriter::new();
    let mut builder = StructureBuilder::new(writer.clone());

    // Act
    builder.begin_message();
    builder.role("a1").unwrap();
    builder.role("a2").unwrap(); // Should be ignored
    builder.role("a3").unwrap(); // Should be ignored
    builder.end_message().unwrap();

    // Assert
    let expected = r#"{"role":"a1","content":[]}"#.to_owned() + "\n";
    assert_eq!(writer.get_output(), expected);
}

#[test]
fn create_message_without_input_role() {
    // Arrange
    let mut writer = RcWriter::new();
    let mut builder = StructureBuilder::new(writer.clone());

    // Act without "builder.role()"
    builder.begin_message();
    builder.begin_text_chunk().unwrap();
    writer.write_all(b"hello").unwrap();
    builder.end_message().unwrap();

    // Assert
    let expected =
        r#"{"role":"assistant","content":[{"type":"text","text":"hello"}]}"#.to_owned() + "\n";
    assert_eq!(writer.get_output(), expected);
}

#[test]
fn can_call_end_message_multiple_times() {
    // Arrange
    let mut writer = RcWriter::new();
    let mut builder = StructureBuilder::new(writer.clone());

    // Act
    builder.begin_message();
    builder.begin_text_chunk().unwrap();
    writer.write_all(b"hello").unwrap();
    builder.end_message().unwrap();
    builder.end_message().unwrap(); // Should be ok
    builder.end_message().unwrap(); // Should be ok

    // Assert
    let expected =
        r#"{"role":"assistant","content":[{"type":"text","text":"hello"}]}"#.to_owned() + "\n";
    assert_eq!(writer.get_output(), expected);
}
