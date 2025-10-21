//! Handlers for processing JSON stream operations in markdown conversion.
//!
//! Each handler works with a `StructureBuilder` to construct the markdown output
//! and a `RJiter` for JSON stream iteration. The handlers return `StreamOp` to
//! indicate the result of their operations.

use std::cell::RefCell;

use crate::action_error::ActionError;
use crate::structure_builder::StructureBuilder;
use scan_json::rjiter::jiter::Peek;
use scan_json::rjiter::RJiter;
use scan_json::StreamOp;

/// Handler for the "text" field in messages array
pub fn on_content_text<W: embedded_io::Write, R: embedded_io::Read>(
    rjiter: &mut RJiter<R>,
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> StreamOp {
    let peeked = match rjiter.peek() {
        Ok(p) => p,
        Err(e) => {
            let error = ActionError::PeekText(format!("{e:?}"));
            builder_cell.borrow_mut().set_error(error);
            return StreamOp::Error("Peek error for text");
        }
    };

    if peeked != Peek::String {
        let idx = rjiter.current_index();
        let pos = rjiter.error_position(idx);
        let error = ActionError::TextNotString {
            got: peeked,
            index: idx,
            line: pos.line,
            column: pos.column,
        };
        builder_cell.borrow_mut().set_error(error);
        return StreamOp::Error("Expected string for text value");
    }

    let mut builder = builder_cell.borrow_mut();

    if let Err(e) = builder.start_paragraph() {
        let error = ActionError::StartParagraph(format!("{e:?}"));
        builder.set_error(error);
        return StreamOp::Error("Failed to start paragraph");
    }

    let writer = builder.get_writer();
    if let Err(e) = rjiter.write_long_str(writer) {
        let error = ActionError::WriteText(format!("{e:?}"));
        builder.set_error(error);
        return StreamOp::Error("Failed to write text");
    }

    StreamOp::ValueIsConsumed
}
