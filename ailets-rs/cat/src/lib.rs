use actor_io::{AReader, AWriter};
use std::io;

#[no_mangle]
#[allow(clippy::missing_panics_doc)]
#[allow(clippy::panic)]
pub extern "C" fn execute() -> i32 {
    let mut reader = AReader::new(c"").unwrap_or_else(|e| {
        panic!("Failed to open to read: {e}");
    });
    let mut writer = AWriter::new(c"").unwrap_or_else(|e| {
        panic!("Failed to open to write: {e}");
    });

    io::copy(&mut reader, &mut writer).unwrap_or_else(|e| {
        panic!("Failed to copy: {e}");
    });

    writer.close().unwrap_or_else(|e| {
        panic!("Failed to close writer: {e}");
    });
    reader.close().unwrap_or_else(|e| {
        panic!("Failed to close reader: {e}");
    });

    0
}
