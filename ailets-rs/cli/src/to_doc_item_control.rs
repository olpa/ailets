//! Configuration registry for `to_doc_item` actors.
//!
//! Stores per-node attributes (e.g. `type`, `content_type`) encoded as
//! newline-separated `key=value` pairs — the same format written into the
//! node's `explain` field. A `None` entry means no user-specified attributes.

use std::collections::HashMap;
use std::sync::LazyLock;

use ailetos::Handle;
use parking_lot::Mutex;

static REGISTRY: LazyLock<Mutex<HashMap<Handle, Vec<(String, String)>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Register a `to_doc_item` node with its attributes.
pub fn register(handle: Handle, attrs: Vec<(String, String)>) {
    REGISTRY.lock().insert(handle, attrs);
}

/// Get the attributes for a registered `to_doc_item` node.
///
/// Returns `None` if the handle is not registered.
pub fn get_attrs(handle: Handle) -> Option<Vec<(String, String)>> {
    REGISTRY.lock().get(&handle).cloned()
}
