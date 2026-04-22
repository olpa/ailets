//! Actor suspension support
//!
//! `SuspensionState` is owned by `Environment` and shared (via `Arc`) with every
//! `BlockingActorRuntime`. Actors call `check_and_wait` at I/O yield points.
//!
//! ## Self-suspend vs external suspend
//!
//! There are two suspension flows:
//!
//! * **External** (`suspend` + `check_and_wait`): the CLI suspends an actor at its
//!   next I/O yield point.
//! * **Self-suspend** (`self_suspend_and_wait`): the actor decides when to pause
//!   (e.g. the `dbg` actor after forwarding N bytes).
//!
//! For self-suspend, `resume` is **idempotent**: calling it before the actor has
//! reached its pause point pre-signals the handle so that `self_suspend_and_wait`
//! returns immediately.

use std::collections::{HashMap, HashSet};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use parking_lot::{Condvar, Mutex};

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
        let mut guard = self.suspended.lock();
        while *guard {
            self.condvar.wait(&mut guard);
        }
    }

    fn signal_resume(&self) {
        let mut guard = self.suspended.lock();
        *guard = false;
        self.condvar.notify_one();
    }
}

struct Registry {
    /// Actors currently blocked waiting for a resume signal.
    suspended: HashMap<Handle, Arc<SuspensionControl>>,
    /// Handles that have been pre-resumed before the actor suspended.
    /// `self_suspend_and_wait` skips blocking when it finds the handle here.
    pre_resumed: HashSet<Handle>,
}

impl Registry {
    fn is_empty(&self) -> bool {
        self.suspended.is_empty() && self.pre_resumed.is_empty()
    }
}

/// Shared suspension state owned by `Environment`.
pub struct SuspensionState {
    /// Global hint: true if any actor is currently suspended or pre-resumed.
    /// Actors check this with Relaxed ordering for a fast no-cost path in production.
    any_suspended: Arc<AtomicBool>,
    registry: Mutex<Registry>,
}

impl SuspensionState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            any_suspended: Arc::new(AtomicBool::new(false)),
            registry: Mutex::new(Registry {
                suspended: HashMap::new(),
                pre_resumed: HashSet::new(),
            }),
        }
    }

    /// Externally suspend an actor. Warns if already suspended (no-op).
    pub fn suspend(&self, handle: Handle) {
        let mut reg = self.registry.lock();
        if reg.suspended.contains_key(&handle) {
            warn!(actor = ?handle, "suspend: actor is already suspended");
            return;
        }
        reg.suspended
            .insert(handle, Arc::new(SuspensionControl::new()));
        self.any_suspended.store(true, Ordering::Relaxed);
    }

    /// Resume a suspended actor, or pre-signal it if it has not suspended yet.
    ///
    /// When called before the actor has reached its suspend point (e.g. `resume`
    /// issued in a script right after `run --bg`), the handle is recorded as
    /// pre-resumed so that the next `self_suspend_and_wait` call returns
    /// immediately without blocking.
    pub fn resume(&self, handle: Handle) {
        let mut reg = self.registry.lock();
        if let Some(control) = reg.suspended.remove(&handle) {
            if reg.is_empty() {
                self.any_suspended.store(false, Ordering::Relaxed);
            }
            control.signal_resume();
        } else {
            // Pre-resume: the actor will skip its next self-suspend.
            reg.pre_resumed.insert(handle);
            self.any_suspended.store(true, Ordering::Relaxed);
        }
    }

    /// Actor self-suspend: block until `resume` is called.
    ///
    /// If `resume` was already called for this handle (pre-resume), returns
    /// immediately without blocking.
    pub fn self_suspend_and_wait(&self, handle: Handle) {
        let control = {
            let mut reg = self.registry.lock();
            if reg.pre_resumed.remove(&handle) {
                // Already pre-resumed — skip waiting.
                if reg.is_empty() {
                    self.any_suspended.store(false, Ordering::Relaxed);
                }
                return;
            }
            let control = Arc::new(SuspensionControl::new());
            reg.suspended.insert(handle, Arc::clone(&control));
            self.any_suspended.store(true, Ordering::Relaxed);
            control
        };
        control.wait();
        // Clean up after being woken.
        let mut reg = self.registry.lock();
        reg.suspended.remove(&handle);
        if reg.is_empty() {
            self.any_suspended.store(false, Ordering::Relaxed);
        }
    }

    /// Deregister an actor at shutdown. Wakes it if it was waiting, so it can exit cleanly.
    pub fn deregister(&self, handle: Handle) {
        let mut reg = self.registry.lock();
        let control = reg.suspended.remove(&handle);
        reg.pre_resumed.remove(&handle);
        if reg.is_empty() {
            self.any_suspended.store(false, Ordering::Relaxed);
        }
        drop(reg);
        if let Some(c) = control {
            c.signal_resume();
        }
    }

    /// Returns `true` if the actor is currently suspended (blocked).
    #[must_use]
    pub fn is_suspended(&self, handle: Handle) -> bool {
        self.registry.lock().suspended.contains_key(&handle)
    }

    /// Check if this actor should suspend and block until resumed (external suspend).
    /// Fast no-op when `any_suspended` is false.
    pub fn check_and_wait(&self, handle: Handle) {
        if !self.any_suspended.load(Ordering::Relaxed) {
            return;
        }
        let control = {
            let reg = self.registry.lock();
            reg.suspended.get(&handle).cloned()
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
