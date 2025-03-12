use actor_io::{AReader, AWriter};
use actor_runtime::err_to_heap_c_string;
use scan_json::RJiter;
use scan_json::StreamOp;
use scan_json::{scan, BoxedAction, BoxedEndAction, ParentAndName, Trigger};
use std::cell::RefCell;
use std::ffi::c_char;
use std::io::Write;

const BUFFER_SIZE: u32 = 1024;

fn on_message_begin<W: Write>(
    _rjiter_cell: &RefCell<RJiter>,
    writer_cell: &RefCell<W>,
) -> StreamOp {
    let mut writer = writer_cell.borrow_mut();
    writer.write_all(b"{").unwrap();
    StreamOp::None
}

fn on_message_end<W: Write>(writer_cell: &RefCell<W>) -> Result<(), Box<dyn std::error::Error>> {
    let mut writer = writer_cell.borrow_mut();
    writer.write_all(b"}\n").unwrap();
    Ok(())
}

pub fn _process_query<W: Write>(
    mut reader: impl std::io::Read,
    writer: W,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut buffer = vec![0u8; BUFFER_SIZE as usize];
    let rjiter_cell = RefCell::new(RJiter::new(&mut reader, &mut buffer));
    let builder_cell = RefCell::new(writer);

    let message_begin = Trigger::new(
        Box::new(ParentAndName::new(
            "#top".to_string(),
            "#object".to_string(),
        )),
        Box::new(on_message_begin) as BoxedAction<'_, W>,
    );
    let message_end = Trigger::new(
        Box::new(ParentAndName::new(
            "#top".to_string(),
            "#object".to_string(),
        )),
        Box::new(on_message_end) as BoxedEndAction<'_, W>,
    );

    scan(
        &[message_begin],
        &[message_end],
        &[],
        &rjiter_cell,
        &builder_cell,
    )?;
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
