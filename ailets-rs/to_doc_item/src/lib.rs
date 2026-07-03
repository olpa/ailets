//! Actor: converts a raw value from `input_raw` into a structured content item.
//!
//! Streams raw bytes from stdin, wrapping them in a JSON content-item array on stdout.
//! Attrs (e.g. `AILETS_DOC_ITEM_type`, `AILETS_DOC_ITEM_content_type`) are read
//! from `/var/{pid}/AILETS_DOC_ITEM_*` entries.
//!
//! Supported output formats:
//!   text  → `[{"type":"text"},{"text":"<content>"}]`
//!
//! Images are not currently supported: `file_value` no longer uploads image
//! bytes to KV, so there is no key to emit for `image_key`. See
//! <https://github.com/olpa/ailets/issues/19>.
//!
//! Note: raw bytes are embedded without JSON escaping. Proper escaping is deferred.

use actor_io::{AReader, AWriter};
use actor_runtime::{ActorRuntime, StdHandle};
use actor_vars::{list_var_keys, read_var};
use tracing::error;

/// Prefix for `to_doc_item` attr vars, e.g. `/var/{pid}/AILETS_DOC_ITEM_type`.
///
/// Shared with the DAG-wiring code (`cli/src/user_input.rs`) that writes these vars.
pub const PREFIX: &str = "AILETS_DOC_ITEM_";

fn wr(w: &mut impl std::io::Write, bytes: &[u8]) -> Result<(), String> {
    w.write_all(bytes)
        .map_err(|e| format!("to_doc_item: write error: {e}"))
}

fn write_attr(writer: &mut impl std::io::Write, key: &str, value: &str) -> Result<(), String> {
    wr(writer, b",\"")?;
    wr(writer, key.as_bytes())?;
    wr(writer, b"\":\"")?;
    wr(writer, value.as_bytes())?;
    wr(writer, b"\"")
}

fn write_opening(
    writer: &mut impl std::io::Write,
    runtime: &dyn ActorRuntime,
    type_key: &str,
    type_name: &str,
    content_field: &str,
) -> Result<(), String> {
    wr(writer, b"[{\"type\":\"")?;
    wr(writer, type_name.as_bytes())?;
    wr(writer, b"\"")?;
    for key in list_var_keys(runtime).filter(|k| k.starts_with(PREFIX) && k != type_key) {
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
fn build_frame(runtime: &dyn ActorRuntime, writer: &mut impl std::io::Write) -> Result<(), String> {
    let type_key = format!("{PREFIX}type");
    let item_type = read_var(runtime, &type_key)?.unwrap_or_else(|| "text".to_string());

    match item_type.as_str() {
        "text" => write_opening(writer, runtime, &type_key, "text", "text"),
        "image" => {
            let msg = "to_doc_item: images are not supported, see \
                        https://github.com/olpa/ailets/issues/19"
                .to_string();
            error!("{msg}");
            Err(msg)
        }
        other => Err(format!("to_doc_item: unsupported item type '{other}'")),
    }
}

/// # Errors
/// Returns an error if I/O fails or if the attrs specify an unsupported type.
pub fn execute_impl(
    runtime: &dyn ActorRuntime,
    mut reader: impl std::io::Read,
    mut writer: impl std::io::Write,
) -> Result<(), String> {
    build_frame(runtime, &mut writer)?;
    std::io::copy(&mut reader, &mut writer).map_err(|e| format!("to_doc_item: copy error: {e}"))?;
    wr(&mut writer, br#""}]"#)
}

/// # Errors
/// Returns an error if I/O fails or if the attrs specify an unsupported type.
pub fn execute(runtime: &dyn ActorRuntime) -> Result<(), String> {
    let mut reader = AReader::new_from_std(runtime, StdHandle::Stdin);
    let mut writer = AWriter::new_from_std(runtime, StdHandle::Stdout);
    execute_impl(runtime, &mut reader, &mut writer)
}
