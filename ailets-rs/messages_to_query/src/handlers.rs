use scan_json::StreamOp;
use crate::structure_builder::StructureBuilder;
use std::cell::RefCell;
use std::io::Write;
use scan_json::RJiter;

pub fn on_role<W: Write>(rjiter_cell: &RefCell<RJiter>, builder_cell: &RefCell<StructureBuilder<W>>) -> StreamOp {
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
