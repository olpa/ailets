//! Actor: converts a raw value from `input_raw` into a structured content item.
//!
//! Reads raw bytes from stdin and writes a JSON content-item array to stdout.
//! User-specified attributes (e.g. `type`, `content_type`) are passed via the
//! `control` registry, which is keyed by node handle. The `explain`
//! field on the DAG node carries the same data for human inspection.
//!
//! Supported output formats:
//!   text  → `[{"type":"text"},{"text":"<content>"}]`
//!   image → `[{"type":"image","content_type":"<ct>"},{"image_key":"<key>"}]`
//!            (image key is emitted as-is; the raw bytes must already be in KV)
//!
//! Note: raw bytes are embedded without JSON escaping. Proper escaping is deferred.

mod actor_registry;
pub mod control;

use actor_io::{AReader, AWriter};
use actor_runtime::{ActorRuntime, StdHandle};
use ailetos::Handle;
use embedded_io::Write as _;
use std::io::Read as _;

fn attr<'a>(attrs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    attrs.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

/// # Errors
/// Returns an error if I/O fails or if the attrs specify an unsupported type.
pub fn execute(runtime: &dyn ActorRuntime) -> Result<(), String> {
    let my_handle = Handle::new(runtime.node_handle());
    let attrs = control::get_attrs(my_handle).unwrap_or_default();

    let mut reader = AReader::new_from_std(runtime, StdHandle::Stdin);
    let mut writer = AWriter::new_from_std(runtime, StdHandle::Stdout);

    let mut raw = Vec::new();
    reader
        .read_to_end(&mut raw)
        .map_err(|e| format!("to_doc_item: read error: {e}"))?;

    let output = build_content_item(&raw, &attrs)?;

    writer
        .write_all(&output)
        .map_err(|e| format!("to_doc_item: write error: {e:?}"))?;

    Ok(())
}

/// Build a content-item JSON array from raw bytes and attrs.
///
/// # Errors
/// Returns an error if the `type` attr is set to an unrecognised value.
pub fn build_content_item(raw: &[u8], attrs: &[(String, String)]) -> Result<Vec<u8>, String> {
    let item_type = attr(attrs, "type").unwrap_or("text");

    match item_type {
        "text" => {
            let mut out = Vec::with_capacity(raw.len() + 32);
            out.extend_from_slice(br#"[{"type":"text"},{"text":""#);
            out.extend_from_slice(raw);
            out.extend_from_slice(br#""}]"#);
            Ok(out)
        }
        "image" => {
            let content_type = attr(attrs, "content_type")
                .ok_or_else(|| "to_doc_item: image item requires 'content_type' attr".to_string())?;
            // raw holds the image_key written by file_value, not the image bytes
            let mut out = Vec::with_capacity(raw.len() + 64);
            out.extend_from_slice(br#"[{"type":"image","content_type":""#);
            out.extend_from_slice(content_type.as_bytes());
            out.extend_from_slice(br#""},{"image_key":""#);
            out.extend_from_slice(raw);
            out.extend_from_slice(br#""}]"#);
            Ok(out)
        }
        other => Err(format!("to_doc_item: unsupported item type '{other}'")),
    }
}
