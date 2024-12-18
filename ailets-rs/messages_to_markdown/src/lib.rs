mod areader;
mod awriter;
mod node_runtime;
pub mod rjiter;

use areader::AReader;
use awriter::AWriter;
use rjiter::{Peek, RJiter};

const BUFFER_SIZE: u32 = 1024;

#[derive(Debug, PartialEq)]
enum Level {
    Top,
    Message,
    Body,
    ContentItem,
}

/// Converts a JSON message format to markdown.
///
/// # Panics
///
/// This function will panic if:
/// - The input JSON is malformed
/// - The JSON structure doesn't match the expected format of
///   ```
#[no_mangle]
pub extern "C" fn messages_to_markdown() {
    let mut reader = AReader::new("");
    let mut writer = AWriter::new("");

    let mut buffer = [0u8; BUFFER_SIZE as usize];

    let mut rjiter = RJiter::new(&mut reader, &mut buffer);

    let mut level = Level::Top;
    let mut at_begin = true;

    loop {
        //
        // Top level
        //
        if level == Level::Top {
            if rjiter.finish().is_ok() {
                break;
            }
            let peek = rjiter.peek();
            assert!(peek.is_ok(), "Error: {peek:?}");
            assert!(peek == Ok(Peek::Object), "Expected object at top level");

            level = Level::Message;
            at_begin = true;
            // not continue, but fall-through
        }

        //
        // Message body level: loop through content items
        //

        if level == Level::Body {
            let next = if at_begin {
                rjiter.next_array()
            } else {
                rjiter.array_step()
            };
            assert!(next.is_ok(), "Error on the message body level: {next:?}");

            if next.unwrap().is_none() {
                level = Level::Message;

                at_begin = false;
                continue;
            }

            level = Level::ContentItem;
            at_begin = true;
            // not continue, but fall-through
        }

        //
        // Get the next object key
        //
        let next = if at_begin {
            rjiter.next_object_bytes()
        } else {
            rjiter.next_key_bytes()
        };
        at_begin = false;

        //
        // End of object: level up
        //
        let key = next.unwrap();
        if key.is_none() {
            if level == Level::ContentItem {
                level = Level::Body;
            } else if level == Level::Body {
                level = Level::Message;
            } else if level == Level::Message {
                level = Level::Top;
            } else {
                panic!("Unexpected level {level:?}");
            }

            at_begin = false;
            continue;
        }
        let key = key.unwrap();

        //
        // Content item level
        //
        if level == Level::ContentItem {
            if key != b"text" {
                rjiter.next_skip().unwrap();
                continue;
            }

            writer.start_paragraph();
            let wb = rjiter.write_bytes(&mut writer);
            assert!(wb.is_ok(), "Error on the content item level: {wb:?}");

            continue;
        }

        //
        // Message level: loop through content items
        //
        if level == Level::Message {
            if key != b"content" {
                rjiter.next_skip().unwrap();
                continue;
            }

            level = Level::Body;
            at_begin = true;
            continue;
        }

        panic!("Unexpected level: {level:?}");
    }

    writer.str("\n");
}
