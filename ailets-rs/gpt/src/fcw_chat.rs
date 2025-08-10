//! Chat-style function call writer for JSON output
//!
//! This module provides a function call writer that outputs JSON format
//! suitable for chat systems, with proper JSON escaping to prevent
//! injection attacks and ensure valid JSON output.

use crate::fcw_trait::{FunCallResult, FunCallsWrite};

/// Escapes a string for safe inclusion in JSON by escaping backslashes and quotes
///
/// # Arguments
/// * `s` - The string to escape
///
/// # Returns
/// A new string with `\` escaped as `\\` and `"` escaped as `\"`
fn escape_json_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Chat-style function call writer for JSON output
///
/// Writes function calls in JSON format suitable for chat systems.
/// Each function call is written as a single JSON line in the format:
/// `[{"type":"function","id":"...","name":"..."},{"arguments":"..."}]`
///
/// All string values (id, name, arguments) are properly JSON-escaped to prevent
/// injection attacks and ensure valid JSON output.
///
/// # Type Parameters
/// * `W` - Any type implementing `std::io::Write`
pub struct FunCallsToChat<W: std::io::Write> {
    writer: W,
    header_written: bool,
}

impl<W: std::io::Write> FunCallsToChat<W> {
    /// Creates a new chat-style function call writer
    ///
    /// # Arguments
    /// * `writer` - The underlying writer for output
    ///
    /// # Returns
    /// A new `FunCallsToChat` instance that will write JSON-formatted function calls
    #[must_use]
    pub const fn new(writer: W) -> Self {
        Self {
            writer,
            header_written: false,
        }
    }

    /// Ensures the header is written once at the beginning
    fn ensure_header(&mut self) -> std::io::Result<()> {
        if !self.header_written {
            self.writer
                .write_all(b"[{\"type\":\"ctl\"},{\"role\":\"assistant\"}]\n")?;
            self.header_written = true;
        }
        Ok(())
    }
}

impl<W: std::io::Write> std::io::Write for FunCallsToChat<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.writer.write(buf)
    }
    fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        self.writer.write_all(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
}

impl<W: std::io::Write> FunCallsWrite for FunCallsToChat<W> {
    fn new_item<T: crate::dagops::DagOpsTrait>(
        &mut self,
        id: &str,
        name: &str,
        _dagops: &mut T,
    ) -> FunCallResult {
        // Ensure header is written once
        self.ensure_header()?;

        // Write the JSON structure opening with escaped id and name
        write!(
            self.writer,
            r#"[{{"type":"function","id":"{}","name":"{}"}},{{"arguments":""#,
            escape_json_string(id),
            escape_json_string(name)
        )?;
        Ok(())
    }

    fn arguments_chunk(&mut self, chunk: &[u8]) -> FunCallResult {
        // Write the argument chunk directly (it's already correctly escaped JSON)
        self.writer.write_all(chunk)?;
        Ok(())
    }

    fn end_item(&mut self) -> FunCallResult {
        // Close the arguments string and the JSON array with newline
        writeln!(self.writer, "\"}}]")?;
        Ok(())
    }

    fn end(&mut self) -> FunCallResult {
        // No additional cleanup required for chat format
        Ok(())
    }
}
