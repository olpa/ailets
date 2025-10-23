use crate::action_error::ActionError;
use crate::structure_builder::StructureBuilder;
use scan_json::rjiter::jiter::Peek;
use scan_json::RJiter;
use scan_json::StreamOp;
use std::cell::RefCell;

pub fn on_role<W: embedded_io::Write, R: embedded_io::Read>(
    rjiter: &mut RJiter<R>,
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> StreamOp {
    let role = match rjiter.next_str() {
        Ok(r) => r,
        Err(e) => {
            let error = ActionError::GetRole(format!("{e:?}"));
            builder_cell.borrow_mut().set_error(error);
            return StreamOp::Error("Error getting role value");
        }
    };
    let mut builder = builder_cell.borrow_mut();
    if let Err(e) = builder.handle_role(role) {
        let error = ActionError::HandleRole(format!("{e:?}"));
        builder.set_error(error);
        return StreamOp::Error("Failed to handle role");
    }
    StreamOp::ValueIsConsumed
}

pub fn on_item_begin<W: embedded_io::Write, R: embedded_io::Read>(
    _rjiter: &mut RJiter<R>,
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> StreamOp {
    let mut builder = builder_cell.borrow_mut();
    if let Err(e) = builder.begin_item() {
        let error = ActionError::BeginItem(format!("{e:?}"));
        builder.set_error(error);
        return StreamOp::Error("Failed to begin item");
    }
    StreamOp::None
}

pub fn on_item_end<W: embedded_io::Write>(
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> Result<(), &'static str> {
    let mut builder = builder_cell.borrow_mut();
    if let Err(e) = builder.end_item() {
        let error = ActionError::EndItem(format!("{e:?}"));
        builder.set_error(error);
        return Err("Failed to end item");
    }
    Ok(())
}

pub fn on_item_type<W: embedded_io::Write, R: embedded_io::Read>(
    rjiter: &mut RJiter<R>,
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> StreamOp {
    let item_type = match rjiter.next_str() {
        Ok(t) => t,
        Err(e) => {
            let error = ActionError::GetType(format!("{e:?}"));
            builder_cell.borrow_mut().set_error(error);
            return StreamOp::Error("Error getting type value");
        }
    };
    let mut builder = builder_cell.borrow_mut();
    if let Err(e) = builder.add_item_attribute(String::from("type"), item_type.to_string()) {
        let error = ActionError::AddItemType(format!("{e:?}"));
        builder.set_error(error);
        return StreamOp::Error("Failed to add item type");
    }
    StreamOp::ValueIsConsumed
}

pub fn on_text<W: embedded_io::Write, R: embedded_io::Read>(
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

    if let Err(e) = builder.begin_text() {
        let error = ActionError::BeginText(format!("{e:?}"));
        builder.set_error(error);
        return StreamOp::Error("Failed to begin text");
    }
    let writer = builder.get_writer();
    if let Err(e) = rjiter.write_long_bytes(writer) {
        let error = ActionError::WriteTextBytes(format!("{e:?}"));
        builder.set_error(error);
        return StreamOp::Error("Failed to write text bytes");
    }
    if let Err(e) = builder.end_text() {
        let error = ActionError::EndText(format!("{e:?}"));
        builder.set_error(error);
        return StreamOp::Error("Failed to end text");
    }

    StreamOp::ValueIsConsumed
}

pub fn on_image_url<W: embedded_io::Write, R: embedded_io::Read>(
    rjiter: &mut RJiter<R>,
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> StreamOp {
    let peeked = match rjiter.peek() {
        Ok(p) => p,
        Err(e) => {
            let error = ActionError::PeekImageUrl(format!("{e:?}"));
            builder_cell.borrow_mut().set_error(error);
            return StreamOp::Error("Peek error for image_url");
        }
    };
    if peeked != Peek::String {
        let idx = rjiter.current_index();
        let pos = rjiter.error_position(idx);
        let error = ActionError::ImageUrlNotString {
            got: peeked,
            index: idx,
            line: pos.line,
            column: pos.column,
        };
        builder_cell.borrow_mut().set_error(error);
        return StreamOp::Error("Expected string for image_url value");
    }

    let mut builder = builder_cell.borrow_mut();

    if let Err(e) = builder.begin_image_url() {
        let error = ActionError::BeginImageUrl(format!("{e:?}"));
        builder.set_error(error);
        return StreamOp::Error("Failed to begin image_url");
    }
    let writer = builder.get_writer();
    if let Err(e) = rjiter.write_long_bytes(writer) {
        let error = ActionError::WriteImageUrlBytes(format!("{e:?}"));
        builder.set_error(error);
        return StreamOp::Error("Failed to write image_url bytes");
    }
    if let Err(e) = builder.end_image_url() {
        let error = ActionError::EndImageUrl(format!("{e:?}"));
        builder.set_error(error);
        return StreamOp::Error("Failed to end image_url");
    }

    StreamOp::ValueIsConsumed
}

pub fn on_image_key<W: embedded_io::Write, R: embedded_io::Read>(
    rjiter: &mut RJiter<R>,
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> StreamOp {
    let key = match rjiter.next_str() {
        Ok(k) => k,
        Err(e) => {
            let error = ActionError::GetKey(format!("{e:?}"));
            builder_cell.borrow_mut().set_error(error);
            return StreamOp::Error("Error getting key value");
        }
    };
    let mut builder = builder_cell.borrow_mut();
    if let Err(e) = builder.image_key(key) {
        let error = ActionError::SetImageKey(format!("{e:?}"));
        builder.set_error(error);
        return StreamOp::Error("Failed to set image key");
    }
    StreamOp::ValueIsConsumed
}

pub fn on_image_content_type<W: embedded_io::Write, R: embedded_io::Read>(
    rjiter: &mut RJiter<R>,
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> StreamOp {
    let value = match rjiter.next_str() {
        Ok(v) => v,
        Err(e) => {
            let error = ActionError::GetContentType(format!("{e:?}"));
            builder_cell.borrow_mut().set_error(error);
            return StreamOp::Error("Error getting content_type");
        }
    };
    let mut builder = builder_cell.borrow_mut();
    if let Err(e) = builder.add_item_attribute(String::from("content_type"), String::from(value)) {
        let error = ActionError::AddContentType(format!("{e:?}"));
        builder.set_error(error);
        return StreamOp::Error("Failed to add content_type");
    }
    StreamOp::ValueIsConsumed
}

pub fn on_image_detail<W: embedded_io::Write, R: embedded_io::Read>(
    rjiter: &mut RJiter<R>,
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> StreamOp {
    let value = match rjiter.next_str() {
        Ok(v) => v,
        Err(e) => {
            let error = ActionError::GetDetail(format!("{e:?}"));
            builder_cell.borrow_mut().set_error(error);
            return StreamOp::Error("Error getting detail");
        }
    };
    let mut builder = builder_cell.borrow_mut();
    if let Err(e) = builder.add_item_attribute(String::from("detail"), String::from(value)) {
        let error = ActionError::AddDetail(format!("{e:?}"));
        builder.set_error(error);
        return StreamOp::Error("Failed to add detail");
    }
    StreamOp::ValueIsConsumed
}

pub fn on_func_id<W: embedded_io::Write, R: embedded_io::Read>(
    rjiter: &mut RJiter<R>,
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> StreamOp {
    let id = match rjiter.next_str() {
        Ok(i) => i,
        Err(e) => {
            let error = ActionError::GetFunctionId(format!("{e:?}"));
            builder_cell.borrow_mut().set_error(error);
            return StreamOp::Error("Error getting function id");
        }
    };
    let mut builder = builder_cell.borrow_mut();
    if let Err(e) = builder.add_item_attribute(String::from("id"), String::from(id)) {
        let error = ActionError::AddFunctionId(format!("{e:?}"));
        builder.set_error(error);
        return StreamOp::Error("Failed to add function id");
    }
    StreamOp::ValueIsConsumed
}

pub fn on_func_name<W: embedded_io::Write, R: embedded_io::Read>(
    rjiter: &mut RJiter<R>,
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> StreamOp {
    let name = match rjiter.next_str() {
        Ok(n) => n,
        Err(e) => {
            let error = ActionError::GetFunctionName(format!("{e:?}"));
            builder_cell.borrow_mut().set_error(error);
            return StreamOp::Error("Error getting function name");
        }
    };
    let mut builder = builder_cell.borrow_mut();
    if let Err(e) = builder.add_item_attribute(String::from("name"), String::from(name)) {
        let error = ActionError::AddFunctionName(format!("{e:?}"));
        builder.set_error(error);
        return StreamOp::Error("Failed to add function name");
    }
    StreamOp::ValueIsConsumed
}

pub fn on_func_arguments<W: embedded_io::Write, R: embedded_io::Read>(
    rjiter: &mut RJiter<R>,
    builder_cell: &RefCell<StructureBuilder<W>>,
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

    if let Err(e) = builder.begin_function_arguments() {
        let error = ActionError::BeginFunctionArguments(format!("{e:?}"));
        builder.set_error(error);
        return StreamOp::Error("Failed to begin function arguments");
    }
    let writer = builder.get_writer();
    if let Err(e) = rjiter.write_long_bytes(writer) {
        let error = ActionError::WriteArgumentsBytes(format!("{e:?}"));
        builder.set_error(error);
        return StreamOp::Error("Failed to write arguments bytes");
    }
    if let Err(e) = builder.end_function_arguments() {
        let error = ActionError::EndFunctionArguments(format!("{e:?}"));
        builder.set_error(error);
        return StreamOp::Error("Failed to end function arguments");
    }

    StreamOp::ValueIsConsumed
}

pub fn on_toolspec<W: embedded_io::Write, R: embedded_io::Read>(
    rjiter: &mut RJiter<R>,
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> StreamOp {
    let mut builder = builder_cell.borrow_mut();
    if let Err(e) = builder.toolspec_rjiter(rjiter) {
        let error = ActionError::SetToolspecKey(format!("{e:?}"));
        builder.set_error(error);
        return StreamOp::Error("Failed to process toolspec");
    }
    StreamOp::ValueIsConsumed
}

pub fn on_toolspec_key<W: embedded_io::Write, R: embedded_io::Read>(
    rjiter: &mut RJiter<R>,
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> StreamOp {
    let key = match rjiter.next_str() {
        Ok(k) => k,
        Err(e) => {
            let error = ActionError::GetToolspecKey(format!("{e:?}"));
            builder_cell.borrow_mut().set_error(error);
            return StreamOp::Error("Error getting toolspec key");
        }
    };
    let mut builder = builder_cell.borrow_mut();
    if let Err(e) = builder.toolspec_key(key) {
        let error = ActionError::SetToolspecKey(format!("{e:?}"));
        builder.set_error(error);
        return StreamOp::Error("Failed to set toolspec key");
    }
    StreamOp::ValueIsConsumed
}

pub fn on_tool_call_id<W: embedded_io::Write, R: embedded_io::Read>(
    rjiter: &mut RJiter<R>,
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> StreamOp {
    let value = match rjiter.next_str() {
        Ok(v) => v,
        Err(e) => {
            let error = ActionError::GetToolCallId(format!("{e:?}"));
            builder_cell.borrow_mut().set_error(error);
            return StreamOp::Error("Error getting tool_call_id");
        }
    };
    let mut builder = builder_cell.borrow_mut();
    if let Err(e) = builder.add_item_attribute(String::from("tool_call_id"), String::from(value)) {
        let error = ActionError::AddToolCallId(format!("{e:?}"));
        builder.set_error(error);
        return StreamOp::Error("Failed to add tool_call_id");
    }
    StreamOp::ValueIsConsumed
}
