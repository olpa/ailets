use actor_io::{error_kind_to_str, AReader, AWriter};
use embedded_io::Write;

/// Stdin source actor: simulates reading from OS stdin and writes to stdout
/// TODO: implement actual OS stdin reading
///
/// # Errors
///
/// Returns an error if:
/// - Writing to the output fails
/// - Closing the reader or writer fails
pub fn execute<'a>(mut reader: AReader<'a>, mut writer: AWriter<'a>) -> Result<(), String> {
    // For now, write simulated stdin data
    let data = b"simulated stdin\n";
    if let Err(e) = writer.write_all(data) {
        let error_msg = error_kind_to_str(e);
        return Err(format!("Failed to write: {error_msg}"));
    }

    // Close both reader and writer
    if let Err(e) = writer.close() {
        let error_msg = error_kind_to_str(e);
        return Err(format!("Failed to close writer: {error_msg}"));
    }
    if let Err(e) = reader.close() {
        let error_msg = error_kind_to_str(e);
        return Err(format!("Failed to close reader: {error_msg}"));
    }

    Ok(())
}
