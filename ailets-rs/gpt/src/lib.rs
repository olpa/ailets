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
    builder_cell.borrow_mut().end_message();
    Ok(())
}

pub fn on_role<W: Write>(
    rjiter_cell: &RefCell<RJiter>,
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> StreamOp {
    let mut rjiter = rjiter_cell.borrow_mut();
    let role = rjiter.next_str().unwrap();
    builder_cell.borrow_mut().role(role);
    StreamOp::ValueIsConsumed
}

pub fn on_content<W: Write>(
    rjiter_cell: &RefCell<RJiter>,
    builder_cell: &RefCell<StructureBuilder<W>>,
) -> StreamOp {
    let mut rjiter = rjiter_cell.borrow_mut();
    let peeked = rjiter.peek();
    assert!(peeked.is_ok(), "Error peeking 'content' value: {peeked:?}");
    let peeked = peeked.unwrap();
    assert!(
        peeked == Peek::String,
        "Expected string for 'content' value, got {peeked:?}"
    );

    let mut builder = builder_cell.borrow_mut();
    builder.begin_text_chunk();
    let writer = builder.get_writer();
    let wb = rjiter.write_long_bytes(writer);
    assert!(wb.is_ok(), "Error on the content item level: {wb:?}");
    StreamOp::ValueIsConsumed
}

type BA<'a, W> = BoxedAction<'a, StructureBuilder<W>>;

pub fn _process_gpt<W: Write>(mut reader: impl std::io::Read, writer: W) {
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
    )
    .unwrap();
    builder_cell.borrow_mut().end_message();
}

#[no_mangle]
pub extern "C" fn process_gpt() {
    let reader = AReader::new(c"").unwrap();
    let writer = AWriter::new(c"").unwrap();
    _process_gpt(reader, writer);
}
