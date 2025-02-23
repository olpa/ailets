mod structure_builder;

use actor_io::{AReader, AWriter};
use scan_json::jiter::Peek;
use scan_json::RJiter;
use scan_json::{scan, BoxedAction, ParentParentAndName, StreamOp, Trigger};
use std::cell::RefCell;
use std::io::Write;
use structure_builder::StructureBuilder;

const BUFFER_SIZE: u32 = 1024;

type BA<'a, W> = BoxedAction<'a, StructureBuilder<W>>;

fn on_content_text<W: Write>(
    rjiter_cell: &RefCell<RJiter>,
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> StreamOp {
    let mut rjiter = rjiter_cell.borrow_mut();

    let peeked = rjiter.peek();
    assert!(peeked.is_ok(), "Error peeking 'text' value: {peeked:?}");
    let peeked = peeked.unwrap();
    assert!(
        peeked == Peek::String,
        "Expected string for 'text' value, got {peeked:?}"
    );

    let mut builder = builder_cell.borrow_mut();

    builder.start_paragraph();
    let writer = builder.get_writer();
    let wb = rjiter.write_long_str(writer);
    assert!(wb.is_ok(), "Error on the content item level: {wb:?}");

    StreamOp::ValueIsConsumed
}

/// Converts a JSON message format to markdown.
///
/// # Panics
///
/// This function will panic if:
/// - The input JSON is malformed
/// - The JSON structure doesn't match the expected format of
///   ```
pub fn _messages_to_markdown<W: Write>(mut reader: impl std::io::Read, writer: W) {
    let builder_cell = RefCell::new(StructureBuilder::new(writer));

    let mut buffer = [0u8; BUFFER_SIZE as usize];
    let rjiter_cell = RefCell::new(RJiter::new(&mut reader, &mut buffer));

    let content_text = Trigger::new(
        Box::new(ParentParentAndName::new(
            "content".to_string(),
            "#array".to_string(),
            "text".to_string(),
        )),
        Box::new(on_content_text) as BA<'_, W>,
    );

    scan(&[content_text], &[], &[], &rjiter_cell, &builder_cell).unwrap();
    builder_cell.borrow_mut().finish_with_newline();
}

#[no_mangle]
pub extern "C" fn messages_to_markdown() {
    let reader = AReader::new(c"").unwrap();
    let writer = AWriter::new(c"").unwrap();
    _messages_to_markdown(reader, writer);
}
