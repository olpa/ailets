//! Actor: converts a raw value from `input_raw` into a structured content item.
//!
//! Streams raw bytes from stdin, wrapping them in a JSON content-item array on stdout.
//! Attrs (e.g. `type`, `content_type`) are read from `/var/{pid}/...` entries.
//!
//! Supported output formats:
//!   text  → `[{"type":"text"},{"text":"<content>"}]`
//!   image → `[{"type":"image","content_type":"<ct>"},{"image_key":"<key>"}]`
//!            (image key is emitted as-is; the raw bytes must already be in KV)
//!
//! Note: raw bytes are embedded without JSON escaping. Proper escaping is deferred.

use actor_io::{AReader, AWriter};
use actor_runtime::var_access::{list_var_keys, read_var};
use actor_runtime::{ActorRuntime, StdHandle};
use embedded_io::Write as _;

fn attr<'a>(attrs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

/// Returns `(prefix, suffix)` byte strings that wrap the streamed content.
///
/// # Errors
/// Returns an error if the `type` attr is unrecognised or `content_type` is missing for images.
pub fn build_frame(attrs: &[(String, String)]) -> Result<(Vec<u8>, &'static [u8]), String> {
    let item_type = attr(attrs, "type").unwrap_or("text");
    match item_type {
        "text" => Ok((br#"[{"type":"text"},{"text":""#.to_vec(), br#""}]"#)),
        "image" => {
            let content_type = attr(attrs, "content_type").ok_or_else(|| {
                "to_doc_item: image item requires 'content_type' attr".to_string()
            })?;
            let prefix =
                format!(r#"[{{"type":"image","content_type":"{content_type}"}},{{"image_key":""#)
                    .into_bytes();
            Ok((prefix, br#""}]"#))
        }
        other => Err(format!("to_doc_item: unsupported item type '{other}'")),
    }
}

/// # Errors
/// Returns an error if I/O fails or if the attrs specify an unsupported type.
pub fn execute(runtime: &dyn ActorRuntime) -> Result<(), String> {
    let attrs: Vec<(String, String)> = list_var_keys(runtime)
        .into_iter()
        .filter_map(|k| {
            let v = read_var(runtime, &k).ok()??;
            Some((k, v))
        })
        .collect();

    let mut reader = AReader::new_from_std(runtime, StdHandle::Stdin);
    let mut writer = AWriter::new_from_std(runtime, StdHandle::Stdout);

    let (prefix, suffix) = build_frame(&attrs)?;

    writer
        .write_all(&prefix)
        .map_err(|e| format!("to_doc_item: write error: {e:?}"))?;

    std::io::copy(&mut reader, &mut writer).map_err(|e| format!("to_doc_item: copy error: {e}"))?;

    writer
        .write_all(suffix)
        .map_err(|e| format!("to_doc_item: write error: {e:?}"))?;

    Ok(())
}
