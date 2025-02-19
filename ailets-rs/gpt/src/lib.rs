pub mod structure_builder;

use areader::AReader;
use awriter::AWriter;
use scan_json::jiter::Peek;
use scan_json::RJiter;
use scan_json::{scan, BoxedAction, BoxedEndAction, Name, ParentAndName, StreamOp, Trigger};
use std::cell::RefCell;
use structure_builder::StructureBuilder;

const BUFFER_SIZE: u32 = 1024;

fn on_begin_message(_rjiter: &RefCell<RJiter>, writer: &RefCell<StructureBuilder>) -> StreamOp {
    writer.borrow_mut().begin_message();
    StreamOp::None
}

#[allow(clippy::unnecessary_wraps)]
fn on_end_message(writer: &RefCell<StructureBuilder>) -> Result<(), Box<dyn std::error::Error>> {
    writer.borrow_mut().end_message();
    Ok(())
}

#[allow(clippy::missing_panics_doc)]
pub fn on_role(rjiter_cell: &RefCell<RJiter>, writer: &RefCell<StructureBuilder>) -> StreamOp {
    let mut rjiter = rjiter_cell.borrow_mut();
    let role = rjiter.next_str().unwrap();
    writer.borrow_mut().role(role);
    StreamOp::ValueIsConsumed
}

#[allow(clippy::missing_panics_doc)]
pub fn on_content(
    rjiter_cell: &RefCell<RJiter>,
    builder_cell: &RefCell<StructureBuilder>,
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
    let awriter = builder.get_awriter();
    let wb = rjiter.write_long_bytes(awriter);
    assert!(wb.is_ok(), "Error on the content item level: {wb:?}");
    StreamOp::ValueIsConsumed
}

type BA<'a> = BoxedAction<'a, StructureBuilder>;

#[allow(clippy::missing_panics_doc)]
pub fn _process_gpt(mut reader: impl std::io::Read) {
    let awriter = AWriter::new(c"");
    let builder_cell = RefCell::new(StructureBuilder::new(awriter));

    let mut buffer = vec![0u8; BUFFER_SIZE as usize];

    let rjiter_cell = RefCell::new(RJiter::new(&mut reader, &mut buffer));

    let begin_message = Trigger::new(
        Box::new(Name::new("message".to_string())),
        Box::new(on_begin_message) as BA,
    );
    let end_message = Trigger::new(
        Box::new(Name::new("message".to_string())),
        Box::new(on_end_message) as BoxedEndAction<StructureBuilder>,
    );
    let message_role = Trigger::new(
        Box::new(ParentAndName::new(
            "message".to_string(),
            "role".to_string(),
        )),
        Box::new(on_role) as BA,
    );
    let delta_role = Trigger::new(
        Box::new(ParentAndName::new("delta".to_string(), "role".to_string())),
        Box::new(on_role) as BA,
    );
    let message_content = Trigger::new(
        Box::new(ParentAndName::new(
            "message".to_string(),
            "content".to_string(),
        )),
        Box::new(on_content) as BA,
    );
    let delta_content = Trigger::new(
        Box::new(ParentAndName::new(
            "delta".to_string(),
            "content".to_string(),
        )),
        Box::new(on_content) as BA,
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
#[allow(clippy::missing_panics_doc)]
pub extern "C" fn process_gpt() {
    let reader = AReader::new(c"").unwrap();
    _process_gpt(reader);
}
