//! Actor stub: converts a raw value from `input_raw` into a structured content item.
//!
//! User-specified attributes (e.g. `content_type=image/png`) are passed via the
//! node's `explain` field as newline-separated `key=value` pairs. Not yet implemented.

use actor_runtime::ActorRuntime;

/// # Errors
/// Always returns an error because this actor is not yet implemented.
pub fn execute(_runtime: &dyn ActorRuntime) -> Result<(), String> {
    Err("to_doc_item actor is not yet implemented".to_string())
}
