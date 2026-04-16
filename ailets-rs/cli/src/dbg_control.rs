//! Configuration registry for dbg actors
//!
//! Stores per-actor configuration (bytes_before_pause). Pause/resume
//! synchronisation is handled by `SuspensionState` in the ailetos crate.

use std::collections::HashMap;
use std::sync::Mutex;

use ailetos::Handle;
use once_cell::sync::Lazy;

/// Global registry: node handle → bytes_before_pause
static REGISTRY: Lazy<Mutex<HashMap<Handle, Option<usize>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Register a dbg actor with its configuration.
pub fn register_dbg_actor(handle: Handle, bytes_before_pause: Option<usize>) {
    REGISTRY.lock().unwrap().insert(handle, bytes_before_pause);
}

/// Get the bytes_before_pause config for a registered dbg actor.
///
/// Returns `None` if the handle is not registered.
pub fn get_bytes_before_pause(handle: Handle) -> Option<Option<usize>> {
    REGISTRY.lock().unwrap().get(&handle).copied()
}
