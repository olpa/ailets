pub mod action_error;
pub mod handlers;
mod structure_builder;

use actor_io::{AReader, AWriter};
use actor_runtime::{err_to_heap_c_string, StdHandle};
use handlers::on_content_text;
use scan_json::matcher::StructuralPseudoname;
use scan_json::rjiter::RJiter;
use scan_json::stack::ContextIter;
use scan_json::{iter_match, scan, Action, Options};
use std::cell::RefCell;
use std::ffi::c_char;
use structure_builder::StructureBuilder;
use u8pool::U8Pool;

const BUFFER_SIZE: u32 = 1024;

/// Convert a JSON message format to markdown.
///
/// # Errors
/// If anything goes wrong, including if the `U8Pool` cannot be created.
#[allow(clippy::used_underscore_items)]
pub fn _messages_to_markdown<W: embedded_io::Write>(
    mut reader: impl embedded_io::Read,
    writer: W,
) -> Result<(), String> {
    let builder_cell = RefCell::new(StructureBuilder::new(writer));

    let mut buffer = [0u8; BUFFER_SIZE as usize];
    let mut rjiter = RJiter::new(&mut reader, &mut buffer);

    let find_action = |structural_pseudoname: StructuralPseudoname,
                       context: ContextIter,
                       _baton: &RefCell<StructureBuilder<W>>|
     -> Option<Action<&RefCell<StructureBuilder<W>>, _>> {
        // Match pattern: text key in an array at top level
        // Order is: element name, parent name, grandparent name, etc.
        if iter_match(
            || ["text".as_bytes(), "#array".as_bytes(), "#top".as_bytes()],
            structural_pseudoname,
            context,
        ) {
            Some(on_content_text)
        } else {
            None
        }
    };

    let find_end_action =
        |_structural_pseudoname: StructuralPseudoname,
         _context: ContextIter,
         _baton: &RefCell<StructureBuilder<W>>|
         -> Option<scan_json::EndAction<&RefCell<StructureBuilder<W>>>> { None };

    // Create working buffer for context stack (512 bytes, up to 20 nesting levels)
    let mut working_buffer = [0u8; 512];
    let mut context = U8Pool::new(&mut working_buffer, 20)
        .map_err(|e| format!("Failed to create context pool: {e:?}"))?;

    let scan_result = scan(
        find_action,
        find_end_action,
        &mut rjiter,
        &builder_cell,
        &mut context,
        &Options::new(),
    );

    // Check if there's a detailed error in the baton before returning scan error
    if let Err(e) = scan_result {
        let mut builder = builder_cell.borrow_mut();
        if let Some(detailed_error) = builder.take_error() {
            return Err(detailed_error.to_string());
        }
        return Err(format!("Scan error: {e:?}"));
    }

    builder_cell
        .borrow_mut()
        .finish_with_newline()
        .map_err(|e| format!("Failed to finish with newline: {e:?}"))?;

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
        return err_to_heap_c_string(1, &format!("Failed to process messages to markdown: {e}"));
    }
    std::ptr::null()
}
