//! Generic `LazyLock<Mutex<HashMap<Handle, T>>>` registry shared by actor control modules.

use std::collections::HashMap;
use std::sync::LazyLock;

use ailetos::Handle;
use parking_lot::Mutex;

pub(crate) struct ActorRegistry<T: Send + 'static>(LazyLock<Mutex<HashMap<Handle, T>>>);

// SAFETY: LazyLock<Mutex<_>> is Sync when T: Send.
unsafe impl<T: Send + 'static> Sync for ActorRegistry<T> {}

impl<T: Send + 'static> ActorRegistry<T> {
    pub(crate) const fn new() -> Self {
        Self(LazyLock::new(|| Mutex::new(HashMap::new())))
    }

    pub(crate) fn insert(&self, handle: Handle, value: T) {
        self.0.lock().insert(handle, value);
    }

    pub(crate) fn get_cloned(&self, handle: Handle) -> Option<T>
    where
        T: Clone,
    {
        self.0.lock().get(&handle).cloned()
    }
}
