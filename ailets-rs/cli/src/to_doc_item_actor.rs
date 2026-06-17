//! Actor: converts a raw value from `input_raw` into a structured content item.
//!
//! Reads raw bytes from stdin and writes a JSON content-item array to stdout:
//!   `[{"type":"text"},{"text":"<content>"}]`
//!
//! User-specified attributes (e.g. `content_type=image/png`) are stored in the
//! node's `explain` field as newline-separated `key=value` pairs. Reading them
//! at runtime requires a working Env fd, which is not yet implemented; for now
//! this actor always produces a text content item.
//!
//! Note: the raw bytes are embedded as-is into the JSON string field without
//! escaping. Proper JSON escaping is deferred.

use actor_io::{AReader, AWriter};
use actor_runtime::{ActorRuntime, StdHandle};
use embedded_io::Write as _;
use std::io::Read as _;

const PREFIX: &[u8] = br#"[{"type":"text"},{"text":""#;
const SUFFIX: &[u8] = br#""}]"#;

/// # Errors
/// Returns an error if reading stdin or writing stdout fails.
pub fn execute(runtime: &dyn ActorRuntime) -> Result<(), String> {
    let mut reader = AReader::new_from_std(runtime, StdHandle::Stdin);
    let mut writer = AWriter::new_from_std(runtime, StdHandle::Stdout);

    let mut raw = Vec::new();
    reader
        .read_to_end(&mut raw)
        .map_err(|e| format!("to_doc_item: read error: {e}"))?;

    writer
        .write_all(PREFIX)
        .map_err(|e| format!("to_doc_item: write error: {e:?}"))?;
    writer
        .write_all(&raw)
        .map_err(|e| format!("to_doc_item: write error: {e:?}"))?;
    writer
        .write_all(SUFFIX)
        .map_err(|e| format!("to_doc_item: write error: {e:?}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(input: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(PREFIX);
        out.extend_from_slice(input);
        out.extend_from_slice(SUFFIX);
        out
    }

    #[test]
    fn plain_text() {
        assert_eq!(run(b"hello world"), br#"[{"type":"text"},{"text":"hello world"}]"#);
    }

    #[test]
    fn empty_input() {
        assert_eq!(run(b""), br#"[{"type":"text"},{"text":""}]"#);
    }
}
