use actor_io::{AReader, AWriter};
use std::io;

#[no_mangle]
#[allow(clippy::missing_panics_doc)]
#[allow(clippy::expect_used)]
pub extern "C" fn execute() -> i32 {
    let mut reader = AReader::new(c"my_stream").expect("Failed to open to read");
    let mut writer = AWriter::new(c"my_stream").expect("Failed to open to write");

    io::copy(&mut reader, &mut writer).expect("Failed to copy");

    writer.close().expect("Failed to close writer");
    reader.close().expect("Failed to close reader");

    0
}
