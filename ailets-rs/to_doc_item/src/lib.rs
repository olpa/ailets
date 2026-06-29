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

const PREFIX: &str = "AILETS_DOC_ITEM_";

fn wr<W: embedded_io::Write>(w: &mut W, bytes: &[u8]) -> Result<(), String> {
    w.write_all(bytes)
        .map_err(|e| format!("to_doc_item: write error: {e:?}"))
}

fn write_attr<W: embedded_io::Write>(
    writer: &mut W,
    key: &str,
    value: &str,
) -> Result<(), String> {
    wr(writer, b",\"")?;
    wr(writer, key.as_bytes())?;
    wr(writer, b"\":\"")?;
    wr(writer, value.as_bytes())?;
    wr(writer, b"\"")
}

fn write_opening<W: embedded_io::Write>(
    writer: &mut W,
    runtime: &dyn ActorRuntime,
    type_name: &str,
    content_field: &str,
) -> Result<(), String> {
    wr(writer, b"[{\"type\":\"")?;
    wr(writer, type_name.as_bytes())?;
    wr(writer, b"\"")?;
    let type_key = format!("{PREFIX}type");
    for key in list_var_keys(runtime)
        .filter(|k| k.starts_with(PREFIX) && k != &type_key)
    {
        if let Some(value) = read_var(runtime, &key)? {
            write_attr(writer, &key[PREFIX.len()..], &value)?;
        }
    }
    wr(writer, b"},{\"")?;
    wr(writer, content_field.as_bytes())?;
    wr(writer, b"\":\"")
}

/// Writes the opening frame (before the streamed content) directly to `writer`.
///
/// Reads all `AILETS_DOC_ITEM_*` variables from the runtime and includes them
/// as JSON fields. The caller is responsible for streaming the content and then
/// writing the closing `"}]`.
///
/// # Errors
/// Returns an error if the `type` attr is unrecognised or a write fails.
pub fn build_frame<W: embedded_io::Write>(
    runtime: &dyn ActorRuntime,
    writer: &mut W,
) -> Result<(), String> {
    let item_type = read_var(runtime, &format!("{PREFIX}type"))?
        .unwrap_or_else(|| "text".to_string());

    match item_type.as_str() {
        "text" => write_opening(writer, runtime, "text", "text"),
        "image" => write_opening(writer, runtime, "image", "image_key"),
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

    wr(&mut writer, br#""}]"#)
}
