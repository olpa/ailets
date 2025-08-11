//! Write llm-message with function call requests to the chat

use crate::fcw_trait::{FunCallResult, FunCallsWrite};

// https://github.com/olpa/ailets/issues/185
fn escape_json_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Each function call is written as a single JSON line in the format:
/// `[{"type":"function","id":"...","name":"..."},{"arguments":"..."}]`
pub struct FunCallsToChat<W: std::io::Write> {
    writer: W,
    header_written: bool,
}

impl<W: std::io::Write> FunCallsToChat<W> {
    #[must_use]
    pub const fn new(writer: W) -> Self {
        Self {
            writer,
            header_written: false,
        }
    }

    fn ensure_header(&mut self) -> std::io::Result<()> {
        if !self.header_written {
            self.writer
                .write_all(b"[{\"type\":\"ctl\"},{\"role\":\"assistant\"}]\n")?;
            self.header_written = true;
        }
        Ok(())
    }
}

impl<W: std::io::Write> FunCallsWrite for FunCallsToChat<W> {
    fn new_item<T: crate::dagops::DagOpsTrait>(
        &mut self,
        id: &str,
        name: &str,
        _dagops: &mut T,
    ) -> FunCallResult {
        self.ensure_header()?;

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
        writeln!(self.writer, "\"}}]")?;
        Ok(())
    }

    fn end(&mut self) -> FunCallResult {
        Ok(())
    }
}
