//! Configuration registry for `to_doc_item` actors.
//!
//! Stores per-node attributes (e.g. `type`, `content_type`).
//! A missing entry means no user-specified attributes.

use ailetos::Handle;

use crate::actor_registry::ActorRegistry;

static REGISTRY: ActorRegistry<Vec<(String, String)>> = ActorRegistry::new();

/// Register a `to_doc_item` node with its attributes.
pub fn register(handle: Handle, attrs: Vec<(String, String)>) {
    REGISTRY.insert(handle, attrs);
}

/// Get the attributes for a registered `to_doc_item` node.
///
/// Returns `None` if the handle is not registered.
pub fn get_attrs(handle: Handle) -> Option<Vec<(String, String)>> {
    REGISTRY.get_cloned(handle)
}
