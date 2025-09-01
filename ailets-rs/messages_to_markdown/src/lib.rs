mod structure_builder;

use actor_io::{AReader, AWriter};
use actor_runtime::{err_to_heap_c_string, extract_errno, StdHandle};
use scan_json::jiter::Peek;
use scan_json::RJiter;
use scan_json::{scan, BoxedAction, ParentParentAndName, StreamOp, Trigger};
use std::cell::RefCell;
use std::ffi::c_char;
use std::io::Write;
use structure_builder::StructureBuilder;

const BUFFER_SIZE: u32 = 1024;

type BA<'a, W> = BoxedAction<'a, StructureBuilder<W>>;

/// # Errors
/// If anything goes wrong.
fn on_content_text<W: Write>(
    rjiter_cell: &RefCell<RJiter>,
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> StreamOp {
    let mut rjiter = rjiter_cell.borrow_mut();

    let peeked = match rjiter.peek() {
        Ok(p) => p,
        Err(e) => return StreamOp::Error(Box::new(e)),
    };
    let idx = rjiter.current_index();
    let pos = rjiter.error_position(idx);
    if peeked != Peek::String {
        return StreamOp::Error(
            format!(
                "Expected string for 'text' value, got {peeked:?} at index {idx}, position {pos}"
            )
            .into(),
        );
    }

    let mut builder = builder_cell.borrow_mut();

    if let Err(e) = builder.start_paragraph() {
        return StreamOp::Error(Box::new(e));
    }
    let writer = builder.get_writer();
    if let Err(e) = rjiter.write_long_str(writer) {
        return StreamOp::Error(Box::new(e));
    }

    StreamOp::ValueIsConsumed
}

/// Convert a JSON message format to markdown.
///
/// # Errors
/// If anything goes wrong.
#[allow(clippy::used_underscore_items)]
pub fn _messages_to_markdown<W: Write>(
    mut reader: impl std::io::Read,
    writer: W,
) -> Result<(), Box<dyn std::error::Error>> {
    let builder_cell = RefCell::new(StructureBuilder::new(writer));

    let mut buffer = [0u8; BUFFER_SIZE as usize];
    let rjiter_cell = RefCell::new(RJiter::new(&mut reader, &mut buffer));

    let content_text = Trigger::new(
        Box::new(ParentParentAndName::new(
            "#top".to_string(),
            "#array".to_string(),
            "text".to_string(),
        )),
        Box::new(on_content_text) as BA<'_, W>,
    );

    scan(
        &[content_text],
        &[],
        &rjiter_cell,
        &builder_cell,
        &scan_json::Options::default(),
    )?;
    builder_cell.borrow_mut().finish_with_newline()?;
    Ok(())
}

/// # Panics
/// If anything goes wrong.
#[no_mangle]
pub extern "C" fn messages_to_markdown() -> *const c_char {
    let reader = AReader::new_from_std(StdHandle::Stdin);
    let writer = AWriter::new_from_std(StdHandle::Stdout);

    #[allow(clippy::used_underscore_items)]
    if let Err(e) = _messages_to_markdown(reader, writer) {
        return err_to_heap_c_string(
            extract_errno(&e),
            &format!("Failed to process messages to markdown: {e}"),
        );
    }
    std::ptr::null()
}
