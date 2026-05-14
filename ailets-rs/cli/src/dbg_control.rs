//! Configuration registry for dbg actors
//!
//! Stores per-actor configuration (`bytes_before_pause`). Pause/resume
//! synchronisation is handled by `SuspensionState` in the ailetos crate.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use ailetos::Handle;
use parking_lot::Mutex;

/// Global registry: node handle → `bytes_before_pause`
static REGISTRY: LazyLock<Mutex<HashMap<Handle, Option<usize>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Set of handles that have been killed.
static KILLED: LazyLock<Mutex<HashSet<Handle>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

/// Register a dbg actor with its configuration.
pub fn register_dbg_actor(handle: Handle, bytes_before_pause: Option<usize>) {
    REGISTRY.lock().insert(handle, bytes_before_pause);
}

/// Get the `bytes_before_pause` config for a registered dbg actor.
///
/// Returns `None` if the handle is not registered.
#[allow(clippy::option_option)]
pub fn get_bytes_before_pause(handle: Handle) -> Option<Option<usize>> {
    REGISTRY.lock().get(&handle).copied()
}

/// Mark a dbg actor as killed. The actor checks this after waking from suspension.
pub fn kill_dbg_actor(handle: Handle) {
    KILLED.lock().insert(handle);
}

/// Returns true if the actor has been killed.
pub fn is_killed(handle: Handle) -> bool {
    KILLED.lock().contains(&handle)
}
