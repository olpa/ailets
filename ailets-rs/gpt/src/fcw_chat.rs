//! Chat-style function call writer for JSON output
//!
//! This module provides a function call writer that outputs JSON format
//! suitable for chat systems, with proper JSON escaping to prevent
//! injection attacks and ensure valid JSON output.

use crate::fcw_trait::{FunCallResult, FunCallsWrite};
use std::io::Write;

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
pub struct FunCallsToChat<W: Write> {
    writer: W,
}

impl<W: Write> FunCallsToChat<W> {
    /// Creates a new chat-style function call writer
    ///
    /// # Arguments
    /// * `writer` - The underlying writer for output
    ///
    /// # Returns
    /// A new `FunCallsToChat` instance that will write JSON-formatted function calls
    #[must_use]
    pub const fn new(writer: W) -> Self {
        Self { writer }
    }
}

impl<W: Write> FunCallsWrite for FunCallsToChat<W> {
    fn new_item(&mut self, id: &str, name: &str) -> FunCallResult {
        // Write the JSON structure opening with escaped id and name
        write!(
            self.writer,
            r#"[{{"type":"function","id":"{}","name":"{}"}},{{"arguments":""#,
            escape_json_string(id),
            escape_json_string(name)
        )?;
        Ok(())
    }

    fn arguments_chunk(&mut self, chunk: &str) -> FunCallResult {
        // Write the escaped argument chunk directly into the JSON string
        write!(self.writer, "{}", escape_json_string(chunk))?;
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
