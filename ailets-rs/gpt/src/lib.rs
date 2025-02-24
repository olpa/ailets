pub mod funcall;
pub mod structure_builder;

use actor_io::{AReader, AWriter};
use scan_json::jiter::Peek;
use scan_json::RJiter;
use scan_json::{scan, BoxedAction, BoxedEndAction, Name, ParentAndName, StreamOp, Trigger};
use std::cell::RefCell;
use std::io::Write;
use structure_builder::StructureBuilder;

const BUFFER_SIZE: u32 = 1024;

fn on_begin_message<W: Write>(
    _rjiter: &RefCell<RJiter>,
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> StreamOp {
    builder_cell.borrow_mut().begin_message();
    StreamOp::None
}

fn on_end_message<W: Write>(
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
    if peeked != Peek::String {
        let error: Box<dyn std::error::Error> =
            format!("Expected string for 'content' value, got {peeked:?}").into();
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

type BA<'a, W> = BoxedAction<'a, StructureBuilder<W>>;

/// # Errors
/// If anything goes wrong.
pub fn _process_gpt<W: Write>(
    mut reader: impl std::io::Read,
    writer: W,
) -> Result<(), Box<dyn std::error::Error>> {
    let builder = StructureBuilder::new(writer);
    let builder_cell = RefCell::new(builder);

    let mut buffer = vec![0u8; BUFFER_SIZE as usize];

    let rjiter_cell = RefCell::new(RJiter::new(&mut reader, &mut buffer));

    let begin_message = Trigger::new(
        Box::new(Name::new("message".to_string())),
        Box::new(on_begin_message) as BA<'_, W>,
    );
    let end_message = Trigger::new(
        Box::new(Name::new("message".to_string())),
        Box::new(on_end_message) as BoxedEndAction<StructureBuilder<W>>,
    );
    let message_role = Trigger::new(
        Box::new(ParentAndName::new(
            "message".to_string(),
            "role".to_string(),
        )),
        Box::new(on_role) as BA<'_, W>,
    );
    let delta_role = Trigger::new(
        Box::new(ParentAndName::new("delta".to_string(), "role".to_string())),
        Box::new(on_role) as BA<'_, W>,
    );
    let message_content = Trigger::new(
        Box::new(ParentAndName::new(
            "message".to_string(),
            "content".to_string(),
        )),
        Box::new(on_content) as BA<'_, W>,
    );
    let delta_content = Trigger::new(
        Box::new(ParentAndName::new(
            "delta".to_string(),
            "content".to_string(),
        )),
        Box::new(on_content) as BA<'_, W>,
    );
    let triggers = vec![
        begin_message,
        message_role,
        message_content,
        delta_role,
        delta_content,
    ];
    let triggers_end = vec![end_message];
    let sse_tokens = vec!["data:", "DONE"];
    scan(
        &triggers,
        &triggers_end,
        &sse_tokens,
        &rjiter_cell,
        &builder_cell,
    )?;
    builder_cell.borrow_mut().end_message()?;
    Ok(())
}

/// # Panics
/// If anything goes wrong.
#[no_mangle]
#[allow(clippy::panic)]
pub extern "C" fn process_gpt() {
    let reader = AReader::new(c"").unwrap_or_else(|e| {
        panic!("Failed to create reader: {e:?}");
    });
    let writer = AWriter::new(c"").unwrap_or_else(|e| {
        panic!("Failed to create writer: {e:?}");
    });
    _process_gpt(reader, writer).unwrap_or_else(|e| {
        panic!("Failed to process GPT: {e:?}");
    });
}
