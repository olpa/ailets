use crate::structure_builder::StructureBuilder;
use scan_json::RJiter;
use scan_json::StreamOp;
use std::cell::RefCell;
use std::io::Write;

pub fn on_message_begin<W: Write>(
    _rjiter_cell: &RefCell<RJiter>,
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> StreamOp {
    let mut builder = builder_cell.borrow_mut();
    if let Err(e) = builder.start_message() {
        return StreamOp::Error(Box::new(e));
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
            )
        }
    };
    if let Err(e) = builder_cell.borrow_mut().add_role(role) {
        return StreamOp::Error(Box::new(e));
    }
    StreamOp::ValueIsConsumed
}
