//! Handlers for processing JSON stream operations in message processing.
//!
//! Each handler works with a `StructureBuilder` to construct the message structure
//! and a `RJiter` for JSON stream iteration. The handlers return `StreamOp` to
//! indicate the result of their operations.

use std::cell::RefCell;
use std::io::Write;

use crate::structure_builder::StructureBuilder;
use scan_json::rjiter::jiter::{NumberInt, Peek};
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

pub fn on_function_id<W: Write>(
    rjiter_cell: &RefCell<RJiter>,
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> StreamOp {
    on_function_str_field(rjiter_cell, builder_cell, "id", |funcalls, value| {
        if let Err(e) = funcalls.delta_id(value) {
            return Err(e);
        }
        Ok(())
    })
}

pub fn on_function_name<W: Write>(
    rjiter_cell: &RefCell<RJiter>,
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> StreamOp {
    on_function_str_field(rjiter_cell, builder_cell, "name", |funcalls, value| {
        if let Err(e) = funcalls.delta_function_name(value) {
            return Err(e);
        }
        Ok(())
    })
}

pub fn on_function_arguments<W: Write>(
    rjiter_cell: &RefCell<RJiter>,
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> StreamOp {
    let mut rjiter = rjiter_cell.borrow_mut();
    
    // Check if we have a string value
    match rjiter.peek() {
        Ok(Peek::String) => {
            // Use write_long_bytes for both streaming and non-streaming modes
            let mut args_buffer = Vec::new();
            if let Err(e) = rjiter.write_long_bytes(&mut args_buffer) {
                let error: Box<dyn std::error::Error> = 
                    format!("Error reading function arguments with write_long_bytes: {e:?}").into();
                return StreamOp::Error(error);
            }
            
            // Convert bytes to string and parse JSON to extract the content
            if let Ok(json_str) = String::from_utf8(args_buffer) {
                // Parse the JSON string to extract the actual content
                match serde_json::from_str::<String>(&json_str) {
                    Ok(args_content) => {
                        builder_cell.borrow_mut().get_funcalls_mut().delta_function_arguments(&args_content);
                    }
                    Err(_) => {
                        // If JSON parsing fails, use the raw string (might be partial)
                        builder_cell.borrow_mut().get_funcalls_mut().delta_function_arguments(&json_str);
                    }
                }
            } else {
                let error: Box<dyn std::error::Error> = 
                    "Invalid UTF-8 in function arguments".into();
                return StreamOp::Error(error);
            }
            
            StreamOp::ValueIsConsumed
        }
        Ok(peeked) => {
            let error: Box<dyn std::error::Error> = 
                format!("Expected string for function arguments, got {peeked:?}").into();
            StreamOp::Error(error)
        }
        Err(e) => {
            let error: Box<dyn std::error::Error> = 
                format!("Error peeking function arguments: {e:?}").into();
            StreamOp::Error(error)
        }
    }
}


pub fn on_function_index<W: Write>(
    rjiter_cell: &RefCell<RJiter>,
    builder_cell: &RefCell<StructureBuilder<W>>,
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
    let idx: usize = match value {
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
    if let Err(e) = builder_cell
        .borrow_mut()
        .get_funcalls_mut()
        .delta_index(idx) {
        let error: Box<dyn std::error::Error> = 
            format!("Streaming assumption violation: {e}").into();
        return StreamOp::Error(error);
    }
    StreamOp::ValueIsConsumed
}

/// # Errors
/// Should never happen.
pub fn on_function_end<W: Write>(
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> Result<(), Box<dyn std::error::Error>> {
    builder_cell.borrow_mut().get_funcalls_mut().end_current();
    Ok(())
}

fn on_function_str_field<W: Write, F>(
    rjiter_cell: &RefCell<RJiter>,
    builder_cell: &RefCell<StructureBuilder<W>>,
    field_name: &str,
    apply_field: F,
) -> StreamOp
where
    F: FnOnce(&mut crate::funcalls::FunCalls, &str) -> Result<(), String>,
{
    let mut rjiter = rjiter_cell.borrow_mut();
    let value = match rjiter.next_str() {
        Ok(value) => value,
        Err(e) => {
            let error: Box<dyn std::error::Error> =
                format!("Expected string as the function {field_name}, got {e:?}").into();
            return StreamOp::Error(error);
        }
    };
    if let Err(e) = apply_field(builder_cell.borrow_mut().get_funcalls_mut(), value) {
        let error: Box<dyn std::error::Error> = 
            format!("Streaming assumption violation in {field_name}: {e}").into();
        return StreamOp::Error(error);
    }
    StreamOp::ValueIsConsumed
}
