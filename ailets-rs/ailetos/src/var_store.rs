//! In-process variable store for actor configuration.
//!
//! `VarStore` is owned by `Environment` and consulted by `VarKV` when an
//! `/env/` path is opened. The CLI populates values via `set` before launching
//! actors. Supports per-actor overrides: each entry carries an optional actor
//! id (`None` = global / pid 0).

use parking_lot::RwLock;

pub struct VarStore {
    vars: RwLock<Vec<(Option<u32>, String, String)>>,
}

impl VarStore {
    #[must_use]
    pub fn new() -> Self {
        Self {
            vars: RwLock::new(Vec::new()),
        }
    }

    pub fn set(&self, pid: Option<u32>, key: impl Into<String>, value: impl Into<String>) {
        self.vars.write().push((pid, key.into(), value.into()));
    }

    #[must_use]
    pub fn get(&self, pid: u32, key: &str) -> Option<String> {
        let vars = self.vars.read();
        // per-actor match first (last set wins)
        if let Some((_, _, v)) = vars.iter().rev().find(|(p, k, _)| *p == Some(pid) && k == key) {
            return Some(v.clone());
        }
        // global fallback (last set wins)
        vars.iter()
            .rev()
            .find(|(p, k, _)| p.is_none() && k == key)
            .map(|(_, _, v)| v.clone())
    }

    /// Return the union of per-actor and global keys for the given pid.
    #[must_use]
    pub fn keys(&self, pid: u32) -> Vec<String> {
        let vars = self.vars.read();
        let mut keys: Vec<String> = vars
            .iter()
            .filter(|(p, _, _)| p.is_none() || *p == Some(pid))
            .map(|(_, k, _)| k.clone())
            .collect();
        keys.dedup();
        keys
    }
}

impl Default for VarStore {
    fn default() -> Self {
        Self::new()
    }
}
