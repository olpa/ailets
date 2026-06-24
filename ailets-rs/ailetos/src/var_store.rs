//! In-process variable store for actor configuration.
//!
//! `VarStore` is owned by `Environment` and consulted by `VarKV` when a
//! `/var/` path is opened. The CLI populates values via `set` before launching
//! actors. Supports per-actor overrides: each entry carries an optional actor
//! id (`None` = global / pid 0).

use std::sync::Arc;

use parking_lot::RwLock;

pub struct VarStore {
    vars: RwLock<Vec<(Option<u32>, Arc<str>, Arc<str>)>>,
}

impl VarStore {
    #[must_use]
    pub fn new() -> Self {
        Self {
            vars: RwLock::new(Vec::new()),
        }
    }

    pub fn set(&self, pid: Option<u32>, key: impl Into<Arc<str>>, value: impl Into<Arc<str>>) {
        self.vars.write().push((pid, key.into(), value.into()));
    }

    #[must_use]
    pub fn get(&self, pid: u32, key: &str) -> Option<Arc<str>> {
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

    #[must_use]
    pub fn getenv(&self, pid: u32, key: &str) -> Option<Arc<str>> {
        self.get(pid, key).or_else(|| std::env::var(key).ok().map(Into::into))
    }

    /// Return the union of per-actor and global keys for the given pid.
    #[must_use]
    pub fn keys(&self, pid: u32) -> Vec<Arc<str>> {
        let vars = self.vars.read();
        let mut seen = std::collections::HashSet::new();
        vars.iter()
            .filter(|(p, _, _)| p.is_none() || *p == Some(pid))
            .map(|(_, k, _)| Arc::clone(k))
            .filter(|k| seen.insert(k.clone()))
            .collect()
    }
}

impl Default for VarStore {
    fn default() -> Self {
        Self::new()
    }
}
