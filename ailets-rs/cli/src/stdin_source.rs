use actor_io::{error_kind_to_str, AReader, AWriter};
use embedded_io::Write;
use std::io::Read;

/// Stdin source actor: reads from OS stdin and writes to actor stdout
///
/// # Errors
///
/// Returns an error if:
/// - Reading from OS stdin fails
/// - Writing to the output fails
pub fn execute<'a>(_reader: AReader<'a>, mut writer: AWriter<'a>) -> Result<(), String> {
    let mut stdin = std::io::stdin();
    let mut buffer = [0u8; 8192];

    loop {
        match stdin.read(&mut buffer) {
            Ok(0) => break, // EOF
            Ok(n) => {
                let Some(data) = buffer.get(..n) else {
                    return Err("Buffer slice out of bounds".to_string());
                };
                if let Err(e) = writer.write_all(data) {
                    let error_msg = error_kind_to_str(e);
                    return Err(format!("Failed to write: {error_msg}"));
                }
            }
            Err(e) => {
                return Err(format!("Failed to read from stdin: {e}"));
            }
        }
    }

    // Note: Actors never close stdout/stdin - they didn't open them.
    // SystemRuntime will close these pipes during ActorShutdown.

    Ok(())
}
