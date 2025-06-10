use crate::structure_builder::StructureBuilder;
use scan_json::rjiter::jiter::Peek;
use scan_json::RJiter;
use scan_json::StreamOp;
use std::cell::RefCell;
use std::io::Write;

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
            );
        }
    };
    if let Err(e) = builder_cell.borrow_mut().handle_role(role) {
        return StreamOp::Error(e.into());
    }
    StreamOp::ValueIsConsumed
}

pub fn on_item_begin<W: Write>(
    _rjiter_cell: &RefCell<RJiter>,
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> StreamOp {
    let mut builder = builder_cell.borrow_mut();
    if let Err(e) = builder.begin_item() {
        return StreamOp::Error(e.into());
    }
    StreamOp::None
}

pub fn on_item_end<W: Write>(
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = builder_cell.borrow_mut();
    builder.end_item()?;
    Ok(())
}

pub fn on_item_type<W: Write>(
    rjiter_cell: &RefCell<RJiter>,
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> StreamOp {
    let mut rjiter = rjiter_cell.borrow_mut();
    let item_type = match rjiter.next_str() {
        Ok(t) => t,
        Err(e) => {
            return StreamOp::Error(
                format!("Error getting type value. Expected string, got: {e:?}").into(),
            )
        }
    };
    if let Err(e) = builder_cell
        .borrow_mut()
        .add_item_attribute(String::from("type"), item_type.to_string())
    {
        return StreamOp::Error(e.into());
    }
    StreamOp::ValueIsConsumed
}

pub fn on_item_text<W: Write>(
    rjiter_cell: &RefCell<RJiter>,
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> StreamOp {
    let mut rjiter = rjiter_cell.borrow_mut();
    let peeked = match rjiter.peek() {
        Ok(p) => p,
        Err(e) => return StreamOp::Error(Box::new(e)),
    };
    if peeked != Peek::String {
        let idx = rjiter.current_index();
        let pos = rjiter.error_position(idx);
        return StreamOp::Error(
            format!(
                "Expected string for 'text' value, got {peeked:?} at index {idx}, position {pos}"
            )
            .into(),
        );
    }

    let mut builder = builder_cell.borrow_mut();

    if let Err(e) = builder.begin_text() {
        return StreamOp::Error(e.into());
    }
    let writer = builder.get_writer();
    if let Err(e) = rjiter.write_long_bytes(writer) {
        return StreamOp::Error(e.into());
    }
    if let Err(e) = builder.end_text() {
        return StreamOp::Error(e.into());
    }

    StreamOp::ValueIsConsumed
}

pub fn on_item_image_url<W: Write>(
    rjiter_cell: &RefCell<RJiter>,
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> StreamOp {
    let mut rjiter = rjiter_cell.borrow_mut();
    let peeked = match rjiter.peek() {
        Ok(p) => p,
        Err(e) => return StreamOp::Error(Box::new(e)),
    };
    if peeked != Peek::String {
        let idx = rjiter.current_index();
        let pos = rjiter.error_position(idx);
        return StreamOp::Error(
            format!(
                "Expected string for 'image_url' value, got {peeked:?} at index {idx}, position {pos}"
            )
            .into(),
        );
    }

    let mut builder = builder_cell.borrow_mut();

    if let Err(e) = builder.begin_image_url() {
        return StreamOp::Error(e.into());
    }
    let writer = builder.get_writer();
    if let Err(e) = rjiter.write_long_bytes(writer) {
        return StreamOp::Error(e.into());
    }
    if let Err(e) = builder.end_image_url() {
        return StreamOp::Error(e.into());
    }

    StreamOp::ValueIsConsumed
}

pub fn on_item_image_key<W: Write>(
    rjiter_cell: &RefCell<RJiter>,
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> StreamOp {
    let mut rjiter = rjiter_cell.borrow_mut();
    let key = match rjiter.next_str() {
        Ok(k) => k,
        Err(e) => {
            return StreamOp::Error(
                format!("Error getting key value. Expected string, got: {e:?}").into(),
            );
        }
    };
    if let Err(e) = builder_cell.borrow_mut().image_key(key) {
        return StreamOp::Error(e.into());
    }
    StreamOp::ValueIsConsumed
}

pub fn on_item_attribute_content_type<W: Write>(
    rjiter_cell: &RefCell<RJiter>,
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> StreamOp {
    let mut rjiter = rjiter_cell.borrow_mut();
    let value = match rjiter.next_str() {
        Ok(v) => v,
        Err(e) => {
            return StreamOp::Error(
                format!("Error getting attribute 'content_type'. Expected string, got: {e:?}")
                    .into(),
            );
        }
    };
    let mut builder = builder_cell.borrow_mut();
    if let Err(e) = builder.add_item_attribute(String::from("content_type"), String::from(value)) {
        return StreamOp::Error(e.into());
    }
    StreamOp::ValueIsConsumed
}

pub fn on_item_attribute_detail<W: Write>(
    rjiter_cell: &RefCell<RJiter>,
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> StreamOp {
    let mut rjiter = rjiter_cell.borrow_mut();
    let value = match rjiter.next_str() {
        Ok(v) => v,
        Err(e) => {
            return StreamOp::Error(
                format!("Error getting attribute 'detail'. Expected string, got: {e:?}").into(),
            );
        }
    };
    let mut builder = builder_cell.borrow_mut();
    if let Err(e) = builder.add_item_attribute(String::from("detail"), String::from(value)) {
        return StreamOp::Error(e.into());
    }
    StreamOp::ValueIsConsumed
}

pub fn on_func_id<W: Write>(
    rjiter_cell: &RefCell<RJiter>,
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> StreamOp {
    let mut rjiter = rjiter_cell.borrow_mut();
    let id = match rjiter.next_str() {
        Ok(i) => i,
        Err(e) => {
            return StreamOp::Error(
                format!("Error getting function id value. Expected string, got: {e:?}").into(),
            );
        }
    };
    if let Err(e) = builder_cell.borrow_mut().func_id(id) {
        return StreamOp::Error(e.into());
    }
    StreamOp::ValueIsConsumed
}

pub fn on_func_name<W: Write>(
    rjiter_cell: &RefCell<RJiter>,
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> StreamOp {
    let mut rjiter = rjiter_cell.borrow_mut();
    let name = match rjiter.next_str() {
        Ok(n) => n,
        Err(e) => {
            return StreamOp::Error(
                format!("Error getting function name value. Expected string, got: {e:?}").into(),
            );
        }
    };
    if let Err(e) = builder_cell.borrow_mut().func_name(name) {
        return StreamOp::Error(e.into());
    }
    StreamOp::ValueIsConsumed
}

pub fn on_func_arguments<W: Write>(
    rjiter_cell: &RefCell<RJiter>,
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> StreamOp {
    let mut rjiter = rjiter_cell.borrow_mut();
    let peeked = match rjiter.peek() {
        Ok(p) => p,
        Err(e) => return StreamOp::Error(Box::new(e)),
    };
    if peeked != Peek::String {
        let idx = rjiter.current_index();
        let pos = rjiter.error_position(idx);
        return StreamOp::Error(
            format!(
                "Expected string for 'arguments' value, got {peeked:?} at index {idx}, position {pos}"
            )
            .into(),
        );
    }

    let mut builder = builder_cell.borrow_mut();

    if let Err(e) = builder.begin_arguments() {
        return StreamOp::Error(e.into());
    }
    let writer = builder.get_writer();
    if let Err(e) = rjiter.write_long_bytes(writer) {
        return StreamOp::Error(e.into());
    }
    if let Err(e) = builder.end_arguments() {
        return StreamOp::Error(e.into());
    }

    StreamOp::ValueIsConsumed
}
