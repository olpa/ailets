use scan_json::{rjiter::jiter::Peek, RJiter, StreamOp};
use std::cell::RefCell;
use std::io::Write;

pub fn on_role<W: Write>(rjiter_cell: &RefCell<RJiter>, writer_cell: &RefCell<W>) -> StreamOp {
    let mut rjiter = rjiter_cell.borrow_mut();
    let peeked = match rjiter.peek() {
        Ok(p) => p,
        Err(e) => return StreamOp::Error(Box::new(e)),
    };
    if peeked != Peek::String {
        let error: Box<dyn std::error::Error> =
            format!("Expected string for 'role' value, got {peeked:?}").into();
        return StreamOp::Error(error);
    }

    let mut writer = writer_cell.borrow_mut();
    if let Err(e) = writer.write_all(b"\"role\":\"") {
        return StreamOp::Error(Box::new(e));
    }
    if let Err(e) = rjiter.write_long_bytes(&mut *writer) {
        return StreamOp::Error(Box::new(e));
    }
    if let Err(e) = writer.write_all(b"\"\n") {
        return StreamOp::Error(Box::new(e));
    }
    StreamOp::ValueIsConsumed
}
