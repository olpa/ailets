//! Internal environment variable service for actors.
//!
//! `EnvService` is owned by `Environment` and shared with every
//! `BlockingActorRuntime`. Actors call `get` to read named parameters
//! (model, LLM URL, thinking level, etc.) without touching OS env vars.
//!
//! The CLI populates values via `set` before launching actors.
//! This is a stub: all values default to absent until the CLI wiring is done.

use std::collections::HashMap;

use parking_lot::RwLock;

pub struct EnvService {
    vars: RwLock<HashMap<String, String>>,
}

impl EnvService {
    #[must_use]
    pub fn new() -> Self {
        Self {
            vars: RwLock::new(HashMap::new()),
        }
    }

    pub fn set(&self, key: impl Into<String>, value: impl Into<String>) {
        self.vars.write().insert(key.into(), value.into());
    }

    #[must_use]
    pub fn get(&self, key: &str) -> Option<String> {
        self.vars.read().get(key).cloned()
    }
}

impl Default for EnvService {
    fn default() -> Self {
        Self::new()
    }
}
