use actor_io::{error_kind_to_str, AReader, AWriter};
use embedded_io::Write;

/// Val actor: writes a constant value to stdout (ignores stdin)
///
/// # Errors
///
/// Returns an error if:
/// - Writing to the output fails
/// - Closing the reader or writer fails
pub fn execute<'a>(mut reader: AReader<'a>, mut writer: AWriter<'a>) -> Result<(), String> {
    // Write the constant value
    let data = b"(mee too)";
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
