mod areader;
mod awriter;
mod node_runtime;
pub mod rjiter;
pub mod scan_json;

use areader::AReader;
use awriter::AWriter;
use rjiter::{Peek, RJiter};
use scan_json::{scan_json, Matcher, Trigger};

const BUFFER_SIZE: u32 = 1024;

#[no_mangle]
#[allow(clippy::missing_panics_doc)]
pub extern "C" fn process_gpt() {
    let mut reader = AReader::new("");
    let mut writer = AWriter::new("");

    let mut buffer = vec![0u8; BUFFER_SIZE as usize];

    let mut rjiter = RJiter::new(&mut reader, &mut buffer);

    let begin_of_message = Trigger::new(
        Matcher::new("message".to_string(), None, None, None),
        Box::new(|_rjiter: &mut RJiter, writer: &mut AWriter| {
            writer.begin_message();
        }),
    );
    let end_of_message = Trigger::new(
        Matcher::new("end".to_string(), Some("message".to_string()), None, None),
        Box::new(|_rjiter: &mut RJiter, writer: &mut AWriter| {
            writer.end_message();
        }),
    );
    let message_role = Trigger::new(
        Matcher::new("role".to_string(), Some("message".to_string()), None, None),
        Box::new(|rjiter: &mut RJiter, writer: &mut AWriter| {
            let role = rjiter.next_str().unwrap();
            writer.role(role);
        }),
    );
    let message_content = Trigger::new(
        Matcher::new("content".to_string(), Some("message".to_string()), None, None),
        Box::new(|rjiter: &mut RJiter, writer: &mut AWriter| {
            let peeked = rjiter.peek();
            assert!(
                peeked.is_ok(),
                "Error on the content item level: {peeked:?}"
            );
            assert!(
                peeked == Ok(Peek::String),
                "Expected string at content level"
            );

            writer.begin_text_content();
            let wb = rjiter.write_bytes(Some(writer));
            assert!(wb.is_ok(), "Error on the content item level: {wb:?}");
            writer.end_text_content();
        }),
    );
    scan_json::<&mut AWriter>(
        &[
            begin_of_message,
            end_of_message,
            message_role,
            message_content,
        ],
        &mut rjiter,
        &mut writer,
    );
}
