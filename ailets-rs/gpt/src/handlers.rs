//! Handlers for processing JSON stream operations in message processing.
//!
//! Each handler works with a `StructureBuilder` to construct the message structure
//! and a `RJiter` for JSON stream iteration. The handlers return `StreamOp` to
//! indicate the result of their operations.

use std::cell::RefCell;

use crate::action_error::ActionError;
use crate::dagops::DagOpsTrait;
use crate::structure_builder::StructureBuilder;
use scan_json::rjiter::jiter::{NumberInt, Peek};
use scan_json::RJiter;
use scan_json::StreamOp;

pub fn on_begin_message<W: embedded_io::Write, D: DagOpsTrait, R: embedded_io::Read>(
    _rjiter: &mut RJiter<R>,
    builder_cell: &RefCell<StructureBuilder<W, D>>,
) -> StreamOp {
    if let Err(e) = builder_cell.borrow_mut().begin_message() {
        let error = ActionError::BeginMessage(format!("{e:?}"));
        builder_cell.borrow_mut().set_error(error);
        return StreamOp::Error("Failed to begin message");
    }
    StreamOp::None
}

/// # Errors
/// If anything goes wrong.
pub fn on_end_message<W: embedded_io::Write, D: DagOpsTrait>(
    builder_cell: &RefCell<StructureBuilder<W, D>>,
) -> Result<(), &'static str> {
    if let Err(e) = builder_cell.borrow_mut().end_message() {
        let error = ActionError::EndMessage(format!("{e:?}"));
        builder_cell.borrow_mut().set_error(error);
        return Err("Failed to end message");
    }
    Ok(())
}

pub fn on_role<W: embedded_io::Write, D: DagOpsTrait, R: embedded_io::Read>(
    rjiter: &mut RJiter<R>,
    builder_cell: &RefCell<StructureBuilder<W, D>>,
) -> StreamOp {
    let role = match rjiter.next_str() {
        Ok(r) => r,
        Err(e) => {
            let error = ActionError::RoleValue(format!("{e:?}"));
            builder_cell.borrow_mut().set_error(error);
            return StreamOp::Error("Error getting role value");
        }
    };
    if let Err(e) = builder_cell.borrow_mut().role(role) {
        let error = ActionError::SetRole(format!("{e:?}"));
        builder_cell.borrow_mut().set_error(error);
        return StreamOp::Error("Failed to set role");
    }
    StreamOp::ValueIsConsumed
}

pub fn on_content<W: embedded_io::Write, D: DagOpsTrait, R: embedded_io::Read>(
    rjiter: &mut RJiter<R>,
    builder_cell: &RefCell<StructureBuilder<W, D>>,
) -> StreamOp {
    let peeked = match rjiter.peek() {
        Ok(p) => p,
        Err(e) => {
            let error = ActionError::PeekContent(format!("{e:?}"));
            builder_cell.borrow_mut().set_error(error);
            return StreamOp::Error("Peek error for content");
        }
    };
    if peeked == Peek::Null {
        if let Err(e) = rjiter.known_null() {
            let error = ActionError::ConsumeNull(format!("{e:?}"));
            builder_cell.borrow_mut().set_error(error);
            return StreamOp::Error("Error consuming null");
        }
        return StreamOp::ValueIsConsumed;
    }
    if peeked != Peek::String {
        let idx = rjiter.current_index();
        let pos = rjiter.error_position(idx);
        let error = ActionError::ContentNotString {
            got: peeked,
            index: idx,
            line: pos.line,
            column: pos.column,
        };
        builder_cell.borrow_mut().set_error(error);
        return StreamOp::Error("Expected string for content value");
    }
    let mut builder = builder_cell.borrow_mut();
    if let Err(e) = builder.begin_text_chunk() {
        let error = ActionError::BeginTextChunk(format!("{e:?}"));
        builder.set_error(error);
        return StreamOp::Error("Failed to begin text chunk");
    }
    let writer = builder.get_writer();
    if let Err(e) = rjiter.write_long_bytes(writer) {
        let error = ActionError::WriteContentBytes(format!("{e:?}"));
        builder.set_error(error);
        return StreamOp::Error("Failed to write content bytes");
    }
    StreamOp::ValueIsConsumed
}

pub fn on_function_id<W: embedded_io::Write, D: DagOpsTrait, R: embedded_io::Read>(
    rjiter: &mut RJiter<R>,
    builder_cell: &RefCell<StructureBuilder<W, D>>,
) -> StreamOp {
    let value = match rjiter.next_str() {
        Ok(v) => v,
        Err(e) => {
            let error = ActionError::FunctionIdNotString(format!("{e:?}"));
            builder_cell.borrow_mut().set_error(error);
            return StreamOp::Error("Expected string as function id");
        }
    };

    let mut builder = builder_cell.borrow_mut();
    if let Err(e) = builder.tool_call_id(value) {
        let error = ActionError::HandleFunctionId(e);
        builder.set_error(error);
        return StreamOp::Error("Error handling function id");
    }

    StreamOp::ValueIsConsumed
}

pub fn on_function_name<W: embedded_io::Write, D: DagOpsTrait, R: embedded_io::Read>(
    rjiter: &mut RJiter<R>,
    builder_cell: &RefCell<StructureBuilder<W, D>>,
) -> StreamOp {
    let value = match rjiter.next_str() {
        Ok(v) => v,
        Err(e) => {
            let error = ActionError::FunctionNameNotString(format!("{e:?}"));
            builder_cell.borrow_mut().set_error(error);
            return StreamOp::Error("Expected string as function name");
        }
    };

    let mut builder = builder_cell.borrow_mut();
    if let Err(e) = builder.tool_call_name(value) {
        let error = ActionError::HandleFunctionName(e);
        builder.set_error(error);
        return StreamOp::Error("Error handling function name");
    }

    StreamOp::ValueIsConsumed
}

pub fn on_function_arguments<W: embedded_io::Write, D: DagOpsTrait, R: embedded_io::Read>(
    rjiter: &mut RJiter<R>,
    builder_cell: &RefCell<StructureBuilder<W, D>>,
) -> StreamOp {
    let peeked = match rjiter.peek() {
        Ok(p) => p,
        Err(e) => {
            let error = ActionError::PeekArguments(format!("{e:?}"));
            builder_cell.borrow_mut().set_error(error);
            return StreamOp::Error("Peek error for arguments");
        }
    };
    if peeked != Peek::String {
        let idx = rjiter.current_index();
        let pos = rjiter.error_position(idx);
        let error = ActionError::ArgumentsNotString {
            got: peeked,
            index: idx,
            line: pos.line,
            column: pos.column,
        };
        builder_cell.borrow_mut().set_error(error);
        return StreamOp::Error("Expected string for arguments value");
    }
    let mut builder = builder_cell.borrow_mut();
    let mut writer = builder.get_arguments_chunk_writer();
    if let Err(e) = rjiter.write_long_bytes(&mut writer) {
        let error = ActionError::WriteArgumentsBytes(format!("{e:?}"));
        builder.set_error(error);
        return StreamOp::Error("Failed to write arguments bytes");
    }
    StreamOp::ValueIsConsumed
}

pub fn on_function_index<W: embedded_io::Write, D: DagOpsTrait, R: embedded_io::Read>(
    rjiter: &mut RJiter<R>,
    builder_cell: &RefCell<StructureBuilder<W, D>>,
) -> StreamOp {
    let value = match rjiter.next_int() {
        Ok(v) => v,
        Err(e) => {
            let error = ActionError::FunctionIndexNotInt(format!("{e:?}"));
            builder_cell.borrow_mut().set_error(error);
            return StreamOp::Error("Expected integer as function index");
        }
    };
    let call_idx: usize = match value {
        NumberInt::BigInt(_) => {
            let error = ActionError::FunctionIndexTooLarge;
            builder_cell.borrow_mut().set_error(error);
            return StreamOp::Error("Function index too large for usize");
        }
        NumberInt::Int(i) => {
            if let Ok(idx) = usize::try_from(i) {
                idx
            } else {
                let error = ActionError::FunctionIndexConversion(format!("{i}"));
                builder_cell.borrow_mut().set_error(error);
                return StreamOp::Error("Can't convert function index to usize");
            }
        }
    };

    let mut builder = builder_cell.borrow_mut();
    if let Err(e) = builder.tool_call_index(call_idx) {
        let error = ActionError::HandleFunctionIndex(e);
        builder.set_error(error);
        return StreamOp::Error("Error handling function call index");
    }

    StreamOp::ValueIsConsumed
}

/// # Errors
pub fn on_function_end<W: embedded_io::Write, D: DagOpsTrait>(
    builder_cell: &RefCell<StructureBuilder<W, D>>,
) -> Result<(), &'static str> {
    let mut builder = builder_cell.borrow_mut();
    if let Err(e) = builder.tool_call_end_if_direct() {
        let error = ActionError::EndToolCall(e);
        builder.set_error(error);
        return Err("Failed to end tool call");
    }
    Ok(())
}
