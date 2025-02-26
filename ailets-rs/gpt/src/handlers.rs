use std::cell::RefCell;
use std::io::Write;

use crate::structure_builder::StructureBuilder;
use scan_json::rjiter::jiter::Peek;
use scan_json::RJiter;
use scan_json::StreamOp;

pub fn on_begin_message<W: Write>(
    _rjiter: &RefCell<RJiter>,
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> StreamOp {
    builder_cell.borrow_mut().begin_message();
    StreamOp::None
}

/// # Errors
/// If anything goes wrong.
pub fn on_end_message<W: Write>(
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> Result<(), Box<dyn std::error::Error>> {
    builder_cell.borrow_mut().end_message()?;
    Ok(())
}

pub fn on_role<W: Write>(
    rjiter_cell: &RefCell<RJiter>,
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> StreamOp {
    let mut rjiter = rjiter_cell.borrow_mut();
    let role = match rjiter.next_str() {
        Ok(r) => r,
        Err(e) => {
            return StreamOp::Error(
                format!("Error getting role value. Expected string, got: {e:?}").into(),
            )
        }
    };
    if let Err(e) = builder_cell.borrow_mut().role(role) {
        return StreamOp::Error(Box::new(e));
    }
    StreamOp::ValueIsConsumed
}

pub fn on_content<W: Write>(
    rjiter_cell: &RefCell<RJiter>,
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> StreamOp {
    let mut rjiter = rjiter_cell.borrow_mut();
    let peeked = match rjiter.peek() {
        Ok(p) => p,
        Err(e) => return StreamOp::Error(Box::new(e)),
    };
    if peeked == Peek::Null {
        if let Err(e) = rjiter.known_null() {
            return StreamOp::Error(Box::new(e));
        }
        return StreamOp::ValueIsConsumed;
    }
    if peeked != Peek::String {
        let error: Box<dyn std::error::Error> =
            format!("Expected string for 'content' value, got {peeked:?}").into();
        return StreamOp::Error(error);
    }
    let mut builder = builder_cell.borrow_mut();
    if let Err(e) = builder.begin_text_chunk() {
        return StreamOp::Error(Box::new(e));
    }
    let writer = builder.get_writer();
    if let Err(e) = rjiter.write_long_bytes(writer) {
        return StreamOp::Error(Box::new(e));
    }
    StreamOp::ValueIsConsumed
}
