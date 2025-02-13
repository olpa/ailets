mod areader;
mod awriter;

use areader::AReader;
use awriter::AWriter;
use scan_json::jiter::Peek;
use scan_json::RJiter;
use scan_json::{scan, BoxedAction, ParentParentAndName, StreamOp, Trigger};
use std::cell::RefCell;

const BUFFER_SIZE: u32 = 1024;

type BA<'a> = BoxedAction<'a, AWriter>;

#[allow(clippy::missing_panics_doc)]
fn on_content_text(rjiter_cell: &RefCell<RJiter>, writer_cell: &RefCell<AWriter>) -> StreamOp {
    let mut rjiter = rjiter_cell.borrow_mut();

    let peeked = rjiter.peek();
    assert!(peeked.is_ok(), "Error peeking 'text' value: {peeked:?}");
    let peeked = peeked.unwrap();
    assert!(
        peeked == Peek::String,
        "Expected string for 'text' value, got {peeked:?}"
    );

    let mut writer = writer_cell.borrow_mut();

    writer.start_paragraph();
    let wb = rjiter.write_long_str(&mut *writer);
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
#[no_mangle]
pub extern "C" fn messages_to_markdown() {
    let mut reader = AReader::new("");
    let writer_cell = RefCell::new(AWriter::new(""));

    let mut buffer = [0u8; BUFFER_SIZE as usize];
    let rjiter_cell = RefCell::new(RJiter::new(&mut reader, &mut buffer));

    let content_text = Trigger::new(
        Box::new(ParentParentAndName::new(
            "content".to_string(),
            "#array".to_string(),
            "text".to_string(),
        )),
        Box::new(on_content_text) as BA,
    );

    scan(&[content_text], &[], &[], &rjiter_cell, &writer_cell).unwrap();
    writer_cell.borrow_mut().finish_with_newline();
}
