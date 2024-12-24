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
                level = Level::Top;

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
                level = Level::Top;
            } else {
                panic!("Unexpected level {level:?}");
            }

            at_begin = false;
            continue;
        }
        let key = key.unwrap();

        let key_str = std::str::from_utf8(&key).unwrap(); // FIXME
        println!("key in the big loop: {key_str}"); // FIXME

        //
        // Message level: loop through content items
        //
        if level == Level::Message {
            if key != b"choices" {
                rjiter.next_skip().unwrap();
                continue;
            }

            // FIXME: just to satisfy the linter
            writer.start_message();
            writer.str("{\"role\":\"assistant\",\"content\":[");

            level = Level::Choices;
            at_begin = true;
            continue;
        }

        panic!("Unexpected level: {level:?}");
    }
}
