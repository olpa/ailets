use crate::structure_builder::StructureBuilder;
use scan_json::rjiter::jiter::Peek;
use scan_json::RJiter;
use scan_json::StreamOp;
use std::cell::RefCell;
use std::io::Write;

pub fn on_message_begin<W: Write>(
    _rjiter_cell: &RefCell<RJiter>,
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> StreamOp {
    let mut builder = builder_cell.borrow_mut();
    if let Err(e) = builder.begin_message() {
        return StreamOp::Error(e.into());
    }
    StreamOp::None
}

pub fn on_message_end<W: Write>(
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = builder_cell.borrow_mut();
    builder.end_message()?;
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
            );
        }
    };
    if let Err(e) = builder_cell.borrow_mut().add_role(role) {
        return StreamOp::Error(e.into());
    }
    StreamOp::ValueIsConsumed
}
pub fn on_content_begin<W: Write>(
    _rjiter_cell: &RefCell<RJiter>,
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> StreamOp {
    let mut builder = builder_cell.borrow_mut();
    if let Err(e) = builder.begin_content() {
        return StreamOp::Error(e.into());
    }
    StreamOp::None
}

pub fn on_content_end<W: Write>(
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = builder_cell.borrow_mut();
    builder.end_content()?;
    Ok(())
}

pub fn on_content_item_begin<W: Write>(
    _rjiter_cell: &RefCell<RJiter>,
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> StreamOp {
    let mut builder = builder_cell.borrow_mut();
    if let Err(e) = builder.begin_content_item() {
        return StreamOp::Error(e.into());
    }
    StreamOp::None
}

pub fn on_content_item_end<W: Write>(
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = builder_cell.borrow_mut();
    builder.end_content_item()?;
    Ok(())
}

pub fn on_content_item_type<W: Write>(
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
        .add_item_type(item_type.to_string())
    {
        return StreamOp::Error(e.into());
    }
    StreamOp::ValueIsConsumed
}

pub fn on_content_text<W: Write>(
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
