//! Handlers for processing JSON stream operations in message processing.
//!
//! Each handler works with a `StructureBuilder` to construct the message structure
//! and a `RJiter` for JSON stream iteration. The handlers return `StreamOp` to
//! indicate the result of their operations.

use std::cell::RefCell;

use crate::dagops::DagOpsTrait;
use crate::structure_builder::StructureBuilder;
use scan_json::rjiter::jiter::{NumberInt, Peek};
use scan_json::RJiter;
use scan_json::StreamOp;

pub fn on_begin_message<W: embedded_io::Write, D: DagOpsTrait, R: embedded_io::Read>(
    _rjiter: &mut RJiter<R>,
    builder_cell: &RefCell<StructureBuilder<W, D>>,
) -> StreamOp {
    if let Err(_e) = builder_cell.borrow_mut().begin_message() {
        return StreamOp::Error("Failed to begin message");
    }
    StreamOp::None
}

/// # Errors
/// If anything goes wrong.
pub fn on_end_message<W: embedded_io::Write, D: DagOpsTrait>(
    builder_cell: &RefCell<StructureBuilder<W, D>>,
) -> Result<(), &'static str> {
    builder_cell
        .borrow_mut()
        .end_message()
        .map_err(|_| "Failed to end message")?;
    Ok(())
}

pub fn on_role<W: embedded_io::Write, D: DagOpsTrait, R: embedded_io::Read>(
    rjiter: &mut RJiter<R>,
    builder_cell: &RefCell<StructureBuilder<W, D>>,
) -> StreamOp {
    let Ok(role) = rjiter.next_str() else {
        return StreamOp::Error("Error getting role value");
    };
    if builder_cell.borrow_mut().role(role).is_err() {
        return StreamOp::Error("Failed to set role");
    }
    StreamOp::ValueIsConsumed
}

pub fn on_content<W: embedded_io::Write, D: DagOpsTrait, R: embedded_io::Read>(
    rjiter: &mut RJiter<R>,
    builder_cell: &RefCell<StructureBuilder<W, D>>,
) -> StreamOp {
    let Ok(peeked) = rjiter.peek() else {
        return StreamOp::Error("Peek error for content");
    };
    if peeked == Peek::Null {
        if rjiter.known_null().is_err() {
            return StreamOp::Error("Error consuming null");
        }
        return StreamOp::ValueIsConsumed;
    }
    if peeked != Peek::String {
        return StreamOp::Error("Expected string for content value");
    }
    let mut builder = builder_cell.borrow_mut();
    if builder.begin_text_chunk().is_err() {
        return StreamOp::Error("Failed to begin text chunk");
    }
    let writer = builder.get_writer();
    if rjiter.write_long_bytes(writer).is_err() {
        return StreamOp::Error("Failed to write content bytes");
    }
    StreamOp::ValueIsConsumed
}

pub fn on_function_id<W: embedded_io::Write, D: DagOpsTrait, R: embedded_io::Read>(
    rjiter: &mut RJiter<R>,
    builder_cell: &RefCell<StructureBuilder<W, D>>,
) -> StreamOp {
    let Ok(value) = rjiter.next_str() else {
        return StreamOp::Error("Expected string as function id");
    };

    let mut builder = builder_cell.borrow_mut();
    if builder.tool_call_id(value).is_err() {
        return StreamOp::Error("Error handling function id");
    }

    StreamOp::ValueIsConsumed
}

pub fn on_function_name<W: embedded_io::Write, D: DagOpsTrait, R: embedded_io::Read>(
    rjiter: &mut RJiter<R>,
    builder_cell: &RefCell<StructureBuilder<W, D>>,
) -> StreamOp {
    let Ok(value) = rjiter.next_str() else {
        return StreamOp::Error("Expected string as function name");
    };

    let mut builder = builder_cell.borrow_mut();
    if builder.tool_call_name(value).is_err() {
        return StreamOp::Error("Error handling function name");
    }

    StreamOp::ValueIsConsumed
}

pub fn on_function_arguments<W: embedded_io::Write, D: DagOpsTrait, R: embedded_io::Read>(
    rjiter: &mut RJiter<R>,
    builder_cell: &RefCell<StructureBuilder<W, D>>,
) -> StreamOp {
    let Ok(peeked) = rjiter.peek() else {
        return StreamOp::Error("Peek error for arguments");
    };
    if peeked != Peek::String {
        return StreamOp::Error("Expected string for arguments value");
    }
    let mut builder = builder_cell.borrow_mut();
    let mut writer = builder.get_arguments_chunk_writer();
    if rjiter.write_long_bytes(&mut writer).is_err() {
        return StreamOp::Error("Failed to write arguments bytes");
    }
    StreamOp::ValueIsConsumed
}

pub fn on_function_index<W: embedded_io::Write, D: DagOpsTrait, R: embedded_io::Read>(
    rjiter: &mut RJiter<R>,
    builder_cell: &RefCell<StructureBuilder<W, D>>,
) -> StreamOp {
    let Ok(value) = rjiter.next_int() else {
        return StreamOp::Error("Expected integer as function index");
    };
    let call_idx: usize = match value {
        NumberInt::BigInt(_) => {
            return StreamOp::Error("Function index too large for usize");
        }
        NumberInt::Int(i) => {
            if let Ok(idx) = usize::try_from(i) {
                idx
            } else {
                return StreamOp::Error("Can't convert function index to usize");
            }
        }
    };

    let mut builder = builder_cell.borrow_mut();
    if builder.tool_call_index(call_idx).is_err() {
        return StreamOp::Error("Error handling function call index");
    }

    StreamOp::ValueIsConsumed
}

/// # Errors
pub fn on_function_end<W: embedded_io::Write, D: DagOpsTrait>(
    builder_cell: &RefCell<StructureBuilder<W, D>>,
) -> Result<(), &'static str> {
    builder_cell
        .borrow_mut()
        .tool_call_end_if_direct()
        .map_err(|_| "Failed to end tool call")?;
    Ok(())
}
