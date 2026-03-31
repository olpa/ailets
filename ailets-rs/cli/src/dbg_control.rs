//! Control registry for debug actors (dbg)
//!
//! This module provides a global registry for controlling debug actors that can be
//! paused and resumed. This is used for testing on-demand actor spawning.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex};

use ailetos::Handle;

thread_local! {
    static CURRENT_DBG_CONTROL: RefCell<Option<Arc<DbgControl>>> = const { RefCell::new(None) };
    static CURRENT_DBG_BYTE_LIMIT: RefCell<Option<usize>> = const { RefCell::new(None) };
}

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
}

impl DbgControl {
    fn new() -> Self {
        Self {
            state: Mutex::new(DbgControlState::Paused),
            condvar: Condvar::new(),
        }
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

/// Global registry of debug actor controls
static REGISTRY: Mutex<Option<HashMap<Handle, Arc<DbgControl>>>> = Mutex::new(None);

/// Initialize the registry (call once at startup)
fn ensure_registry_initialized() {
    let mut registry = REGISTRY.lock().unwrap();
    if registry.is_none() {
        *registry = Some(HashMap::new());
    }
}

/// Register a new debug actor and return its control handle
///
/// This also sets the control in thread-local storage so the actor can access it.
pub fn register_dbg_actor(handle: Handle) -> Arc<DbgControl> {
    register_dbg_actor_with_config(handle, None)
}

/// Init hook for registering deb actors (discards return value)
///
/// This is the function signature expected by ActorInitFn.
pub fn init_dbg_actor(handle: Handle) {
    let _ = register_dbg_actor(handle);
}

/// Register a new debug actor with optional byte limit configuration
///
/// This also sets the control and byte limit in thread-local storage so the actor can access them.
pub fn register_dbg_actor_with_config(handle: Handle, byte_limit: Option<usize>) -> Arc<DbgControl> {
    ensure_registry_initialized();
    let mut registry = REGISTRY.lock().unwrap();
    let control = Arc::new(DbgControl::new());
    registry
        .as_mut()
        .unwrap()
        .insert(handle, Arc::clone(&control));

    // Set in thread-local for actor access
    CURRENT_DBG_CONTROL.with(|cell| {
        *cell.borrow_mut() = Some(Arc::clone(&control));
    });

    // Set byte limit in thread-local
    CURRENT_DBG_BYTE_LIMIT.with(|cell| {
        *cell.borrow_mut() = byte_limit;
    });

    control
}

/// Get the current debug actor's control handle from thread-local storage
///
/// Returns None if not called from within a debug actor context.
pub fn get_current_dbg_control() -> Option<Arc<DbgControl>> {
    CURRENT_DBG_CONTROL.with(|cell| cell.borrow().clone())
}

/// Get the current debug actor's byte limit from thread-local storage
///
/// Returns None if not set.
pub fn get_current_dbg_byte_limit() -> Option<usize> {
    CURRENT_DBG_BYTE_LIMIT.with(|cell| *cell.borrow())
}

/// Resume a debug actor by its handle
///
/// Returns `Ok(())` if the actor was found and resumed, or an error message otherwise.
pub fn resume_dbg_actor(handle: Handle) -> Result<(), String> {
    ensure_registry_initialized();
    let registry = REGISTRY.lock().unwrap();
    if let Some(map) = registry.as_ref() {
        if let Some(control) = map.get(&handle) {
            control.resume();
            Ok(())
        } else {
            Err(format!("Debug actor with handle {:?} not found", handle))
        }
    } else {
        Err("Registry not initialized".to_string())
    }
}

/// Get a list of all registered debug actors
pub fn list_dbg_actors() -> Vec<Handle> {
    ensure_registry_initialized();
    let registry = REGISTRY.lock().unwrap();
    registry
        .as_ref()
        .map(|map| map.keys().copied().collect())
        .unwrap_or_default()
}
