//! Actor suspension support
//!
//! `SuspensionState` is owned by `Environment` and shared (via `Arc`) with every
//! `BlockingActorRuntime`. Actors call `check_and_wait` at I/O yield points.

use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Condvar, Mutex,
};

use tracing::warn;

use crate::idgen::Handle;

struct SuspensionControl {
    /// True while the actor should remain suspended
    suspended: Mutex<bool>,
    condvar: Condvar,
}

impl SuspensionControl {
    fn new() -> Self {
        Self {
            suspended: Mutex::new(true),
            condvar: Condvar::new(),
        }
    }

    fn wait(&self) {
        let mut guard = self.suspended.lock().unwrap();
        while *guard {
            guard = self.condvar.wait(guard).unwrap();
        }
    }

    fn signal_resume(&self) {
        let mut guard = self.suspended.lock().unwrap();
        *guard = false;
        self.condvar.notify_one();
    }
}

/// Shared suspension state owned by `Environment`.
pub struct SuspensionState {
    /// Global hint: true if any actor is currently suspended.
    /// Actors check this with Relaxed ordering for a fast no-cost path in production.
    any_suspended: Arc<AtomicBool>,
    registry: Mutex<HashMap<Handle, Arc<SuspensionControl>>>,
}

impl SuspensionState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            any_suspended: Arc::new(AtomicBool::new(false)),
            registry: Mutex::new(HashMap::new()),
        }
    }

    /// Suspend an actor. Warns if already suspended (no-op in that case).
    pub fn suspend(&self, handle: Handle) {
        let mut registry = self.registry.lock().unwrap();
        if registry.contains_key(&handle) {
            warn!(actor = ?handle, "suspend: actor is already suspended");
            return;
        }
        registry.insert(handle, Arc::new(SuspensionControl::new()));
        self.any_suspended.store(true, Ordering::Relaxed);
    }

    /// Resume a suspended actor. Warns if not suspended (no-op in that case).
    pub fn resume(&self, handle: Handle) {
        let control = {
            let mut registry = self.registry.lock().unwrap();
            let entry = registry.remove(&handle);
            if registry.is_empty() {
                self.any_suspended.store(false, Ordering::Relaxed);
            }
            entry
        };
        if let Some(c) = control {
            c.signal_resume();
        } else {
            warn!(actor = ?handle, "resume: actor is not suspended");
        }
    }

    /// Deregister an actor at shutdown. Wakes it if it was waiting, so it can exit cleanly.
    pub fn deregister(&self, handle: Handle) {
        let control = {
            let mut registry = self.registry.lock().unwrap();
            let entry = registry.remove(&handle);
            if registry.is_empty() {
                self.any_suspended.store(false, Ordering::Relaxed);
            }
            entry
        };
        if let Some(c) = control {
            c.signal_resume();
        }
    }

    /// Returns `true` if the actor is currently suspended.
    #[must_use]
    pub fn is_suspended(&self, handle: Handle) -> bool {
        self.registry.lock().unwrap().contains_key(&handle)
    }

    /// Check if this actor should suspend and block until resumed.
    /// Fast no-op when `any_suspended` is false.
    pub fn check_and_wait(&self, handle: Handle) {
        if !self.any_suspended.load(Ordering::Relaxed) {
            return;
        }
        let control = {
            let registry = self.registry.lock().unwrap();
            registry.get(&handle).cloned()
        };
        if let Some(c) = control {
            c.wait();
        }
    }
}

impl Default for SuspensionState {
    fn default() -> Self {
        Self::new()
    }
}
