mod handlers;
pub mod structure_builder;

use actor_io::{AReader, AWriter};
use actor_runtime::err_to_heap_c_string;
use scan_json::RJiter;
use scan_json::StreamOp;
use scan_json::{scan, BoxedAction, BoxedEndAction, ParentAndName, Trigger};
use std::cell::RefCell;
use std::ffi::c_char;
use std::io::Write;
use structure_builder::StructureBuilder;

const BUFFER_SIZE: u32 = 1024;

fn on_message_begin<W: Write>(
    _rjiter_cell: &RefCell<RJiter>,
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> StreamOp {
    let mut builder = builder_cell.borrow_mut();
    builder.start_message().unwrap();
    StreamOp::None
}

fn on_message_end<W: Write>(builder_cell: &RefCell<StructureBuilder<W>>) -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = builder_cell.borrow_mut();
    builder.end_message().unwrap();
    Ok(())
}

/// # Errors
/// If anything goes wrong.
pub fn _process_query<W: Write>(
    mut reader: impl std::io::Read,
    writer: W,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut buffer = vec![0u8; BUFFER_SIZE as usize];
    let rjiter_cell = RefCell::new(RJiter::new(&mut reader, &mut buffer));
    let builder = StructureBuilder::new(writer);
    let builder_cell = RefCell::new(builder);

    let message_begin = Trigger::new(
        Box::new(ParentAndName::new(
            "#top".to_string(),
            "#object".to_string(),
        )),
        Box::new(on_message_begin) as BoxedAction<'_, StructureBuilder<W>>,
    );
    let message_end = Trigger::new(
        Box::new(ParentAndName::new(
            "#top".to_string(),
            "#object".to_string(),
        )),
        Box::new(on_message_end) as BoxedEndAction<'_, StructureBuilder<W>>,
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
