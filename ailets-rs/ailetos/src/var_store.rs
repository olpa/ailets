//! In-process variable store for actor configuration.
//!
//! `VarStore` is owned by `Environment` and consulted by `VarKV` when a
//! `/var/` path is opened. The CLI populates values via `set` before launching
//! actors.
//!
//! Resolution order (first match wins, last-set-wins within each tier):
//! 1. Per-actor entry stored with `Some(pid)` matching the requested actor.
//! 2. Global entry stored with `None`.
//! 3. OS environment variable (only via `getenv`, not `get`).

use std::sync::Arc;

use parking_lot::RwLock;

pub struct VarStore {
    vars: RwLock<Vec<(Option<i64>, Arc<str>, Arc<str>)>>,
}

impl VarStore {
    #[must_use]
    pub fn new() -> Self {
        Self {
            vars: RwLock::new(Vec::new()),
        }
    }

    pub fn set(&self, pid: Option<i64>, key: impl Into<Arc<str>>, value: impl Into<Arc<str>>) {
        self.vars.write().push((pid, key.into(), value.into()));
    }

    /// Look up a variable: per-actor `Some(pid)` first, then global `None`. Does not consult the OS environment; see `getenv`.
    #[must_use]
    pub fn get(&self, pid: i64, key: &str) -> Option<Arc<str>> {
        let vars = self.vars.read();
        // per-actor match first (last set wins)
        if let Some((_, _, v)) = vars.iter().rev().find(|(p, k, _)| *p == Some(pid) && k.as_ref() == key) {
            return Some(Arc::clone(v));
        }
        // global fallback (last set wins)
        vars.iter()
            .rev()
            .find(|(p, k, _)| p.is_none() && k.as_ref() == key)
            .map(|(_, _, v)| Arc::clone(v))
    }

    /// Like `get`, but falls back to the OS environment if the variable is not found in the store.
    #[must_use]
    pub fn getenv(&self, pid: i64, key: &str) -> Option<Arc<str>> {
        self.get(pid, key).or_else(|| std::env::var(key).ok().map(Into::into))
    }

    /// Return the union of per-actor and global keys for the given pid. Does not include OS environment keys; see `keysenv`.
    #[must_use]
    pub fn keys(&self, pid: i64) -> Vec<Arc<str>> {
        self.keys_impl(pid, false)
    }

    /// Like `keys`, but also includes keys from the OS environment.
    #[must_use]
    pub fn keysenv(&self, pid: i64) -> Vec<Arc<str>> {
        self.keys_impl(pid, true)
    }

    fn keys_impl(&self, pid: i64, include_os_env: bool) -> Vec<Arc<str>> {
        let vars = self.vars.read();
        let mut seen = std::collections::HashSet::new();
        let mut result: Vec<Arc<str>> = vars
            .iter()
            .filter(|(p, _, _)| p.is_none() || *p == Some(pid))
            .map(|(_, k, _)| Arc::clone(k))
            .filter(|k| seen.insert(k.clone()))
            .collect();
        if include_os_env {
            for (k, _) in std::env::vars() {
                let k: Arc<str> = Arc::from(k.as_str());
                if seen.insert(k.clone()) {
                    result.push(k);
                }
            }
        }
        result
    }
}

impl Default for VarStore {
    fn default() -> Self {
        Self::new()
    }
}
