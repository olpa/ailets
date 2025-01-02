pub mod areader;
pub mod awriter;
pub mod node_runtime;
pub mod rjiter;
pub mod scan_json;
pub mod sse_handler;

use std::cell::RefCell;

use areader::AReader;
use awriter::AWriter;
use rjiter::{Peek, RJiter};
use scan_json::{scan_json, ActionResult, Matcher, Trigger, TriggerEnd};

const BUFFER_SIZE: u32 = 1024;

fn on_begin_message(_rjiter: &RefCell<RJiter>, writer: &RefCell<AWriter>) -> ActionResult {
    writer.borrow_mut().begin_message();
    ActionResult::Ok
}

fn on_end_message(writer: &RefCell<AWriter>) {
    writer.borrow_mut().end_message();
}

fn on_role(rjiter_cell: &RefCell<RJiter>, writer: &RefCell<AWriter>) -> ActionResult {
    let mut rjiter = rjiter_cell.borrow_mut();
    let role = rjiter.next_str().unwrap();
    writer.borrow_mut().role(role);
    ActionResult::OkValueIsConsumed
}

fn on_content(rjiter_cell: &RefCell<RJiter>, writer_cell: &RefCell<AWriter>) -> ActionResult {
    let mut rjiter = rjiter_cell.borrow_mut();
    let peeked = rjiter.peek();
    assert!(
        peeked.is_ok(),
        "Error on the content item level: {peeked:?}"
    );
    assert!(
        peeked == Ok(Peek::String),
        "Expected string at content level"
    );

    let mut writer = writer_cell.borrow_mut();
    writer.begin_text_chunk();
    let wb = rjiter.write_bytes(&mut *writer);
    assert!(wb.is_ok(), "Error on the content item level: {wb:?}");
    ActionResult::OkValueIsConsumed
}

#[no_mangle]
#[allow(clippy::missing_panics_doc)]
pub extern "C" fn process_gpt() {
    let mut reader = AReader::new("");
    let writer_cell = RefCell::new(AWriter::new(""));

    let mut buffer = vec![0u8; BUFFER_SIZE as usize];

    let rjiter_cell = RefCell::new(RJiter::new(&mut reader, &mut buffer));

    let begin_message = Trigger::new(
        Matcher::new("message".to_string(), None, None, None),
        Box::new(on_begin_message),
    );
    let end_message = TriggerEnd::new(
        Matcher::new("message".to_string(), None, None, None),
        Box::new(on_end_message),
    );
    let role = Trigger::new(
        Matcher::new("role".to_string(), Some("message".to_string()), None, None),
        Box::new(on_role),
    );
    let content = Trigger::new(
        Matcher::new(
            "content".to_string(),
            Some("message".to_string()),
            None,
            None,
        ),
        Box::new(on_content),
    );
    let triggers = vec![begin_message, role, content];
    let triggers_end = vec![end_message];
    let sse_tokens = vec!["data:", "DONE"];
    scan_json(
        &triggers,
        &triggers_end,
        &sse_tokens,
        &rjiter_cell,
        &writer_cell,
    );
}
