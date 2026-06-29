//! Actor: converts a raw value from `input_raw` into a structured content item.
//!
//! Streams raw bytes from stdin, wrapping them in a JSON content-item array on stdout.
//! Attrs (e.g. `AILETS_DOC_ITEM_type`, `AILETS_DOC_ITEM_content_type`) are read
//! from `/var/{pid}/AILETS_DOC_ITEM_*` entries.
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

const PREFIX: &str = "AILETS_DOC_ITEM_";

fn write_attr<W: embedded_io::Write>(
    writer: &mut W,
    key: &str,
    value: &str,
) -> Result<(), String> {
    let part = format!(r#","{key}":"{value}""#);
    writer
        .write_all(part.as_bytes())
        .map_err(|e| format!("to_doc_item: write error: {e:?}"))
}

/// Writes the opening frame (before the streamed content) directly to `writer`.
///
/// Reads all `AILETS_DOC_ITEM_*` variables from the runtime and includes them
/// as JSON fields. The caller is responsible for streaming the content and then
/// writing [`SUFFIX`].
///
/// # Errors
/// Returns an error if the `type` attr is unrecognised, `content_type` is missing
/// for images, or a write fails.
pub fn build_frame<W: embedded_io::Write>(
    runtime: &dyn ActorRuntime,
    writer: &mut W,
) -> Result<(), String> {
    let item_type = read_var(runtime, &format!("{PREFIX}type"))?
        .unwrap_or_else(|| "text".to_string());

    let write_err =
        |e: W::Error| format!("to_doc_item: write error: {e:?}");

    match item_type.as_str() {
        "text" => {
            writer
                .write_all(br#"[{"type":"text""#)
                .map_err(write_err)?;
            for key in list_var_keys(runtime)
                .filter(|k| k.starts_with(PREFIX) && k != &format!("{PREFIX}type"))
            {
                if let Some(value) = read_var(runtime, &key)? {
                    write_attr(writer, &key[PREFIX.len()..], &value)?;
                }
            }
            writer
                .write_all(br#"},{"text":""#)
                .map_err(|e| format!("to_doc_item: write error: {e:?}"))
        }
        "image" => {
            let content_type = read_var(runtime, &format!("{PREFIX}content_type"))?.ok_or_else(
                || "to_doc_item: image item requires 'content_type' attr".to_string(),
            )?;
            writer
                .write_all(br#"[{"type":"image""#)
                .map_err(write_err)?;
            write_attr(writer, "content_type", &content_type)?;
            for key in list_var_keys(runtime).filter(|k| {
                k.starts_with(PREFIX)
                    && k != &format!("{PREFIX}type")
                    && k != &format!("{PREFIX}content_type")
            }) {
                if let Some(value) = read_var(runtime, &key)? {
                    write_attr(writer, &key[PREFIX.len()..], &value)?;
                }
            }
            writer
                .write_all(br#"},{"image_key":""#)
                .map_err(|e| format!("to_doc_item: write error: {e:?}"))
        }
        other => Err(format!("to_doc_item: unsupported item type '{other}'")),
    }
}

/// # Errors
/// Returns an error if I/O fails or if the attrs specify an unsupported type.
pub fn execute(runtime: &dyn ActorRuntime) -> Result<(), String> {
    let mut reader = AReader::new_from_std(runtime, StdHandle::Stdin);
    let mut writer = AWriter::new_from_std(runtime, StdHandle::Stdout);

    build_frame(runtime, &mut writer)?;

    std::io::copy(&mut reader, &mut writer)
        .map_err(|e| format!("to_doc_item: copy error: {e}"))?;

    writer
        .write_all(br#""}]"#)
        .map_err(|e| format!("to_doc_item: write error: {e:?}"))
}
