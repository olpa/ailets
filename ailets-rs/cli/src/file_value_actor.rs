//! Actor stub: reads a file or stdin and writes raw bytes to stdout.
//!
//! The file path is passed via the node's `explain` field. A value of `"-"`
//! means stdin. Not yet implemented.

use actor_runtime::ActorRuntime;

/// # Errors
/// Always returns an error because this actor is not yet implemented.
pub fn execute(_runtime: &dyn ActorRuntime) -> Result<(), String> {
    Err("file_value actor is not yet implemented".to_string())
}
