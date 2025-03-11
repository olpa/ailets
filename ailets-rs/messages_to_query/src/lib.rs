use scan_json::{scan, BoxedAction, BoxedEndAction, ContextFrame, Name, ParentAndName, Trigger};
use std::io::Write;
use std::cell::RefCell;
use scan_json::StreamOp;
use scan_json::RJiter;

fn on_message_begin(rjiter_cell: &RefCell<RJiter>, writer_cell: &RefCell<dyn Write>) -> StreamOp {
    let writer = writer_cell.borrow_mut();
    writer.write_all(b"{").unwrap();
    StreamOp::Continue
}

fn on_message_end(rjiter_cell: &RefCell<RJiter>, writer_cell: &RefCell<dyn Write>) -> StreamOp {
    let writer = writer_cell.borrow_mut();
    writer.write_all(b"}").unwrap();
    StreamOp::Continue
}

pub fn _process_query<W: Write>(
    mut reader: impl std::io::Read,
    writer: W,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut rjiter_cell = RefCell::new(RJiter::new(&mut reader, &mut buffer));
    let mut builder_cell = RefCell::new(QueryBuilder::new(writer));

    let message_begin = Trigger::new(
        Box::new(ParentAndName::new(
            "#top".to_string(),
            "#object".to_string(),
        )),
        Box::new(on_message_begin) as BA<'_, W>,
    );
    let message_end = Trigger::new(
        Box::new(ParentAndName::new(
            "#top".to_string(),
            "#object".to_string(),
        )),
        Box::new(on_message_end) as BA<'_, W>,
    );

    scan(&[message_begin], &[message_end], &[], &rjiter_cell, &builder_cell)?;
    Ok(())
}

/// # Panics
/// If anything goes wrong.
#[no_mangle]
pub extern "C" fn process_query() -> *const c_char {
    let reader = match AReader::new(c"") {
        Ok(reader) => reader,
        Err(e) => return err_to_heap_c_string(&format!("Failed to create reader: {e:?}")),
    };
    let writer = match AWriter::new(c"") {
        Ok(writer) => writer,
        Err(e) => return err_to_heap_c_string(&format!("Failed to create writer: {e:?}")),
    };
    if let Err(e) = _process_query(reader, writer) {
        return err_to_heap_c_string(&format!("Failed to process query: {e}"));
    }
    std::ptr::null()
}

