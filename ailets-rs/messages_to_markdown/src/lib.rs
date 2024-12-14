mod areader;
mod node_runtime;
mod writer;

use jiter::{Jiter, Peek};
use std::io::Read;

use areader::AReader;
use writer::Writer;

// const BUFFER_SIZE: u32 = 1024;

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
pub fn messages_to_markdown() {
    let mut reader = AReader::new("");
    let mut writer = Writer::new("");

    let mut buffer = Vec::new();
    reader.read_to_end(&mut buffer).unwrap();

    let mut jiter = Jiter::new(&buffer);
    let mut level = Level::Top;
    let mut at_begin = true;

    loop {
        //
        // Top level
        //
        if level == Level::Top {
            if jiter.finish().is_ok() {
                break;
            }
            let peek = jiter.peek();
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
                jiter.next_array()
            } else {
                jiter.array_step()
            };
            println!("! message body next: {next:?}"); // FIXME
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
            jiter.next_object()
        } else {
            jiter.next_key()
        };
        println!("! top loop next: {next:?} in level: {level:?}");
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
            if key != "text" {
                jiter.next_skip().unwrap();
                continue;
            }

            writer.start_paragraph();
            let text = jiter.next_str();
            assert!(text.is_ok(), "Error on the content item level: {text:?}");
            let text = text.unwrap();
            writer.str(text);

            continue;
        }

        //
        // Message level: loop through content items
        //
        if level == Level::Message {
            if key != "content" {
                jiter.next_skip().unwrap();
                continue;
            }

            level = Level::Body;
            at_begin = true;
            continue;
        }

        panic!("Unexpected level: {level:?}");
    }
}
