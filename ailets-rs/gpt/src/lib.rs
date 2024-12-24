mod areader;
mod awriter;
mod node_runtime;

use std::io::{Read, Write};

use areader::AReader;
use awriter::AWriter;

const BUFFER_SIZE: usize = 1024;

/// Processes GPT messages
///
/// # Panics
///
/// This function will panic if there are I/O errors
#[no_mangle]
pub extern "C" fn process_gpt() {
    let mut reader = AReader::new("");
    let mut writer = AWriter::new("");

    let mut buffer = vec![0u8; BUFFER_SIZE];
    let mut bytes_read;

    // Read input in chunks
    loop {
        bytes_read = reader.read(&mut buffer).unwrap();
        if bytes_read == 0 {
            break;
        }

        // Process the buffer here
        // For now, just echo the input
        writer.write_all(&buffer[..bytes_read]).unwrap();
    }
}
