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
    TopOutside,
    TopObject,
    Choices,
    Choice,
    Message,
}

#[no_mangle]
#[allow(clippy::missing_panics_doc)]
pub extern "C" fn process_gpt() {
    let mut reader = AReader::new("");
    let mut writer = AWriter::new("");

    let mut buffer = vec![0u8; BUFFER_SIZE as usize];

    let mut rjiter = RJiter::new(&mut reader, &mut buffer);

    let mut level = Level::TopOutside;
    let mut at_begin = true;

    loop {
        //
        // Top level: outside the objects
        //
        if level == Level::TopOutside {
            if rjiter.finish().is_ok() {
                break;
            }
            let peek = rjiter.peek();
            assert!(peek.is_ok(), "Error: {peek:?}");
            assert!(peek == Ok(Peek::Object), "Expected object at top level");

            level = Level::TopObject;
            at_begin = true;
            // do not continue, but fall-through
        }

        //
        // Choices level: loop through individual  choices
        //

        if level == Level::Choices {
            let next = if at_begin {
                rjiter.next_array()
            } else {
                rjiter.array_step()
            };
            assert!(next.is_ok(), "Error on the choices level: {next:?}");

            if next.unwrap().is_none() {
                level = Level::TopObject;

                at_begin = false;
                continue;
            }

            level = Level::Choice;
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

        println!("next in the big loop: {next:?}"); // FIXME

        //
        // End of object: level up
        //
        let key = next.unwrap();
        if key.is_none() {
            if level == Level::Message {
                writer.end_message();
                level = Level::Choice;
            } else if level == Level::Choice {
                level = Level::Choices;
            } else if level == Level::Choices {
                level = Level::TopObject;
            } else if level == Level::TopObject {
                level = Level::TopOutside;
            } else {
                panic!("Unexpected level {level:?}");
            }

            at_begin = false;
            continue;
        }
        let key = key.unwrap();

        let key_str = std::str::from_utf8(key).unwrap(); // FIXME
        println!("key in the big loop: {key_str}"); // FIXME

        //
        // Top object level: loop through content items
        //
        if level == Level::TopObject {
            if key != b"choices" {
                rjiter.next_skip().unwrap();
                continue;
            }

            level = Level::Choices;
            at_begin = true;
            continue;
        }

        //
        // Choice level: loop until "message"
        //
        if level == Level::Choice {
            if key != b"message" {
                rjiter.next_skip().unwrap();
                continue;
            }

            writer.begin_message();

            level = Level::Message;
            at_begin = true;
            continue;
        }

        //
        // Message level: write content
        //
        if level == Level::Message {
            if key == b"role" {
                let role = rjiter.next_str().unwrap();
                writer.role(role);
                continue;
            }

            if key == b"content" {
                let peeked = rjiter.peek();
                assert!(peeked.is_ok(), "Error on the content item level: {peeked:?}");
                assert!(peeked == Ok(Peek::String), "Expected string at content level");

                writer.begin_text_content();
                let wb = rjiter.write_bytes(&mut writer);
                assert!(wb.is_ok(), "Error on the content item level: {wb:?}");
                writer.end_text_content();
                continue;
            }

            rjiter.next_skip().unwrap();
            continue;
        }

        panic!("Unexpected level: {level:?}");
    }
}
