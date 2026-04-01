//! Control registry for debug actors (dbg)
//!
//! This module provides a global registry for controlling debug actors that can be
//! paused and resumed. This is used for testing on-demand actor spawning.
//!
//! Architecture: Actors get their control structure by looking up their node handle
//! in the global registry. No thread-local storage needed.

use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex};

use ailetos::Handle;
use once_cell::sync::Lazy;

/// State of a debug actor
pub enum DbgControlState {
    /// Actor is running normally
    Running,
    /// Actor is paused and waiting on the condvar
    Paused,
}

/// Control structure for a debug actor
pub struct DbgControl {
    state: Mutex<DbgControlState>,
    condvar: Condvar,
    /// Number of bytes to pass before pausing
    bytes_before_pause: Option<usize>,
}

impl DbgControl {
    fn new(bytes_before_pause: Option<usize>) -> Self {
        Self {
            state: Mutex::new(DbgControlState::Paused),
            condvar: Condvar::new(),
            bytes_before_pause,
        }
    }

    /// Get the configured number of bytes to pass before pausing
    pub fn bytes_before_pause(&self) -> Option<usize> {
        self.bytes_before_pause
    }

    /// Wait until the actor is resumed
    pub fn wait_for_resume(&self) {
        let mut state = self.state.lock().unwrap();
        while matches!(*state, DbgControlState::Paused) {
            state = self.condvar.wait(state).unwrap();
        }
    }

    /// Resume the actor
    pub fn resume(&self) {
        let mut state = self.state.lock().unwrap();
        *state = DbgControlState::Running;
        self.condvar.notify_all();
    }
}

/// Global registry of debug actor controls indexed by node handle
static REGISTRY: Lazy<Mutex<HashMap<Handle, Arc<DbgControl>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Register a new debug actor with configuration
///
/// # Arguments
/// * `handle` - The node handle for this actor
/// * `bytes_before_pause` - Number of bytes to pass before pausing (None = default 100)
pub fn register_dbg_actor(handle: Handle, bytes_before_pause: Option<usize>) -> Arc<DbgControl> {
    let mut registry = REGISTRY.lock().unwrap();
    let control = Arc::new(DbgControl::new(bytes_before_pause));
    registry.insert(handle, Arc::clone(&control));
    control
}

/// Get the debug control for a specific actor by its node handle
///
/// Returns None if the actor hasn't been registered.
pub fn get_dbg_control(handle: Handle) -> Option<Arc<DbgControl>> {
    let registry = REGISTRY.lock().unwrap();
    registry.get(&handle).cloned()
}

/// Resume a debug actor by its handle
///
/// Returns `Ok(())` if the actor was found and resumed, or an error message otherwise.
pub fn resume_dbg_actor(handle: Handle) -> Result<(), String> {
    let registry = REGISTRY.lock().unwrap();
    if let Some(control) = registry.get(&handle) {
        control.resume();
        Ok(())
    } else {
        Err(format!("Debug actor with handle {:?} not found", handle))
    }
}

/// Get a list of all registered debug actors
pub fn list_dbg_actors() -> Vec<Handle> {
    let registry = REGISTRY.lock().unwrap();
    registry.keys().copied().collect()
}
