mod structure_builder;

use actor_io::{AReader, AWriter};
use actor_runtime::{err_to_heap_c_string, extract_errno, StdHandle};
use scan_json::matcher::StructuralPseudoname;
use scan_json::rjiter::{jiter::Peek, RJiter};
use scan_json::stack::ContextIter;
use scan_json::{iter_match, scan, BoxedAction, Options, StreamOp};
use std::cell::RefCell;
use std::ffi::c_char;
use std::io::Write;
use structure_builder::StructureBuilder;
use u8pool::U8Pool;

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
/// If anything goes wrong, including if the `U8Pool` cannot be created.
#[allow(clippy::used_underscore_items)]
pub fn _messages_to_markdown<W: Write>(
    mut reader: impl std::io::Read,
    writer: W,
) -> Result<(), Box<dyn std::error::Error>> {
    let builder_cell = RefCell::new(StructureBuilder::new(writer));

    let mut buffer = [0u8; BUFFER_SIZE as usize];
    let rjiter_cell = RefCell::new(RJiter::new(&mut reader, &mut buffer));

    let find_action =
        |structural_pseudoname: StructuralPseudoname, context: ContextIter| -> Option<BA<'_, W>> {
            // Match pattern: text key in an array at top level
            if iter_match(
                || ["text".as_bytes(), "#array".as_bytes(), "#top".as_bytes()],
                structural_pseudoname,
                context,
            ) {
                Some(Box::new(on_content_text))
            } else {
                None
            }
        };

    let find_end_action = |_structural_pseudoname: StructuralPseudoname,
                           _context: ContextIter|
     -> Option<scan_json::BoxedEndAction<StructureBuilder<W>>> { None };

    // Create working buffer for context stack (512 bytes, up to 20 nesting levels)
    let mut working_buffer = [0u8; 512];
    let mut context = U8Pool::new(&mut working_buffer, 20)
        .map_err(|e| format!("Failed to create context pool: {e}"))?;

    scan(
        find_action,
        find_end_action,
        &rjiter_cell,
        &builder_cell,
        &mut context,
        &Options::new(),
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
