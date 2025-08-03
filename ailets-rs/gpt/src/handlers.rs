//! Handlers for processing JSON stream operations in message processing.
//!
//! Each handler works with a `StructureBuilder` to construct the message structure
//! and a `RJiter` for JSON stream iteration. The handlers return `StreamOp` to
//! indicate the result of their operations.

use std::cell::RefCell;
use std::io::Write;

use crate::fcw_trait::FunCallsWrite;
use crate::structure_builder::StructureBuilder;
use scan_json::rjiter::jiter::{NumberInt, Peek};
use scan_json::RJiter;
use scan_json::StreamOp;

pub fn on_begin_message<W1: Write, W2: FunCallsWrite>(
    _rjiter: &RefCell<RJiter>,
    builder_cell: &RefCell<StructureBuilder<W1, W2>>,
) -> StreamOp {
    builder_cell.borrow_mut().begin_message();
    StreamOp::None
}

/// # Errors
/// If anything goes wrong.
pub fn on_end_message<W1: Write, W2: FunCallsWrite>(
    builder_cell: &RefCell<StructureBuilder<W1, W2>>,
) -> Result<(), Box<dyn std::error::Error>> {
    builder_cell.borrow_mut().end_message()?;
    Ok(())
}

pub fn on_role<W1: Write, W2: FunCallsWrite>(
    rjiter_cell: &RefCell<RJiter>,
    builder_cell: &RefCell<StructureBuilder<W1, W2>>,
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

pub fn on_content<W1: Write, W2: FunCallsWrite>(
    rjiter_cell: &RefCell<RJiter>,
    builder_cell: &RefCell<StructureBuilder<W1, W2>>,
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
        let idx = rjiter.current_index();
        let pos = rjiter.error_position(idx);
        let error: Box<dyn std::error::Error> = format!(
            "Expected string for 'content' value, got {peeked:?}, at index {idx}, position {pos}"
        )
        .into();
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

pub fn on_function_id<W1: Write, W2: FunCallsWrite>(
    rjiter_cell: &RefCell<RJiter>,
    builder_cell: &RefCell<StructureBuilder<W1, W2>>,
) -> StreamOp {
    let mut rjiter = rjiter_cell.borrow_mut();
    let value = match rjiter.next_str() {
        Ok(value) => value,
        Err(e) => {
            let error: Box<dyn std::error::Error> =
                format!("Expected string as the function id, got {e:?}").into();
            return StreamOp::Error(error);
        }
    };

    let mut builder = builder_cell.borrow_mut();
    if let Err(e) = builder.tool_call_id(value) {
        let error: Box<dyn std::error::Error> = format!("Error handling function id: {e}").into();
        return StreamOp::Error(error);
    }

    StreamOp::ValueIsConsumed
}

pub fn on_function_name<W1: Write, W2: FunCallsWrite>(
    rjiter_cell: &RefCell<RJiter>,
    builder_cell: &RefCell<StructureBuilder<W1, W2>>,
) -> StreamOp {
    let mut rjiter = rjiter_cell.borrow_mut();
    let value = match rjiter.next_str() {
        Ok(value) => value,
        Err(e) => {
            let error: Box<dyn std::error::Error> =
                format!("Expected string as the function name, got {e:?}").into();
            return StreamOp::Error(error);
        }
    };

    let mut builder = builder_cell.borrow_mut();
    if let Err(e) = builder.tool_call_name(value) {
        let error: Box<dyn std::error::Error> = format!("Error handling function name: {e}").into();
        return StreamOp::Error(error);
    }

    StreamOp::ValueIsConsumed
}

pub fn on_function_arguments<W1: Write, W2: FunCallsWrite>(
    rjiter_cell: &RefCell<RJiter>,
    builder_cell: &RefCell<StructureBuilder<W1, W2>>,
) -> StreamOp {
    let mut rjiter = rjiter_cell.borrow_mut();
    let peeked = match rjiter.peek() {
        Ok(p) => p,
        Err(e) => return StreamOp::Error(Box::new(e)),
    };
    if peeked != Peek::String {
        let idx = rjiter.current_index();
        let pos = rjiter.error_position(idx);
        let error: Box<dyn std::error::Error> = format!(
            "Expected string for 'arguments' value, got {peeked:?}, at index {idx}, position {pos}"
        )
        .into();
        return StreamOp::Error(error);
    }
    let mut builder = builder_cell.borrow_mut();
    let mut writer = builder.get_arguments_chunk_writer();
    if let Err(e) = rjiter.write_long_bytes(&mut writer) {
        return StreamOp::Error(Box::new(e));
    }
    StreamOp::ValueIsConsumed
}

pub fn on_function_index<W1: Write, W2: FunCallsWrite>(
    rjiter_cell: &RefCell<RJiter>,
    builder_cell: &RefCell<StructureBuilder<W1, W2>>,
) -> StreamOp {
    let mut rjiter = rjiter_cell.borrow_mut();
    let value = match rjiter.next_int() {
        Ok(value) => value,
        Err(e) => {
            let error: Box<dyn std::error::Error> =
                format!("Expected integer as the function index, got {e:?}").into();
            return StreamOp::Error(error);
        }
    };
    let idx = rjiter.current_index();
    let pos = rjiter.error_position(idx);
    let call_idx: usize = match value {
        NumberInt::BigInt(_) => {
            let error: Box<dyn std::error::Error> =
                format!("Can't convert the function index to usize, got {value:?} at index {idx}, position {pos}").into();
            return StreamOp::Error(error);
        }
        NumberInt::Int(i) => {
            if let Ok(idx) = usize::try_from(i) {
                idx
            } else {
                let error: Box<dyn std::error::Error> =
                    format!("Can't convert the function index to usize, got {value:?} at index {idx}, position {pos}").into();
                return StreamOp::Error(error);
            }
        }
    };

    let mut builder = builder_cell.borrow_mut();
    if let Err(e) = builder.tool_call_index(call_idx) {
        let error: Box<dyn std::error::Error> =
            format!("Error handling function call index {call_idx}: {e}").into();
        return StreamOp::Error(error);
    }

    StreamOp::ValueIsConsumed
}

/// # Errors
/// Should never happen.
pub fn on_function_end<W1: Write, W2: FunCallsWrite>(
    builder_cell: &RefCell<StructureBuilder<W1, W2>>,
) -> Result<(), Box<dyn std::error::Error>> {
    builder_cell.borrow_mut().tool_call_end_direct()?;
    Ok(())
}
