//! Configuration registry for dbg actors
//!
//! Stores per-actor configuration (`bytes_before_pause`). Pause/resume
//! synchronisation is handled by `SuspensionState` in the ailetos crate.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use ailetos::Handle;

/// Global registry: node handle → `bytes_before_pause`
static REGISTRY: LazyLock<Mutex<HashMap<Handle, Option<usize>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Register a dbg actor with its configuration.
pub fn register_dbg_actor(handle: Handle, bytes_before_pause: Option<usize>) {
    REGISTRY
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(handle, bytes_before_pause);
}

/// Get the `bytes_before_pause` config for a registered dbg actor.
///
/// Returns `None` if the handle is not registered.
#[allow(clippy::option_option)]
pub fn get_bytes_before_pause(handle: Handle) -> Option<Option<usize>> {
    REGISTRY
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&handle)
        .copied()
}
