mod areader;
mod awriter;
mod node_runtime;
pub mod rjiter;
pub mod scan_json;

use std::cell::RefCell;

use areader::AReader;
use awriter::AWriter;
use rjiter::{Peek, RJiter};
use scan_json::{scan_json, Matcher, Trigger};

const BUFFER_SIZE: u32 = 1024;

#[no_mangle]
#[allow(clippy::missing_panics_doc)]
pub extern "C" fn process_gpt() {
    let mut reader = AReader::new("");
    let writer_cell = RefCell::new(AWriter::new(""));

    let mut buffer = vec![0u8; BUFFER_SIZE as usize];

    let rjiter_cell = RefCell::new(RJiter::new(&mut reader, &mut buffer));

    let begin_of_message = Trigger::new(
        Matcher::new("message".to_string(), None, None, None),
        Box::new(|_rjiter: &RefCell<RJiter>, writer: &RefCell<AWriter>| {
            writer.borrow_mut().begin_message();
        }),
    );
    let end_of_message = Trigger::new(
        Matcher::new("end".to_string(), Some("message".to_string()), None, None),
        Box::new(|_rjiter: &RefCell<RJiter>, writer: &RefCell<AWriter>| {
            writer.borrow_mut().end_message();
        }),
    );
    let message_role = Trigger::new(
        Matcher::new("role".to_string(), Some("message".to_string()), None, None),
        Box::new(|rjiter_cell: &RefCell<RJiter>, writer: &RefCell<AWriter>| {
            let mut rjiter = rjiter_cell.borrow_mut();
            let role = rjiter.next_str().unwrap();
            writer.borrow_mut().role(role);
        }),
    );
    let message_content = Trigger::new(
        Matcher::new("content".to_string(), Some("message".to_string()), None, None),
        Box::new(|rjiter_cell: &RefCell<RJiter>, writer_cell: &RefCell<AWriter>| {
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
            writer.begin_text_content();
            let wb = rjiter.write_bytes(&mut *writer);
            assert!(wb.is_ok(), "Error on the content item level: {wb:?}");
            writer.end_text_content();
        }),
    );
    let triggers = vec![
        begin_of_message,
        end_of_message,
        message_role,
        message_content,
    ];
    scan_json(&triggers, &rjiter_cell, &writer_cell);
}
