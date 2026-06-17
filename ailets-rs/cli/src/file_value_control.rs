//! Configuration registry for `file_value` actors.
//!
//! Stores per-node configuration: the source path (`"-"` for stdin), user
//! attributes (e.g. `type`, `content_type`), and shared KV + IdGen handles
//! needed for image storage.

use std::collections::HashMap;
use std::sync::LazyLock;

use ailetos::{Handle, IdGen, KVBuffers};
use parking_lot::Mutex;
use std::sync::Arc;

pub struct FileValueConfig {
    pub path: String,
    pub attrs: Vec<(String, String)>,
    pub kv: Arc<dyn KVBuffers>,
    pub idgen: Arc<IdGen>,
    pub async_runtime: tokio::runtime::Handle,
}

static REGISTRY: LazyLock<Mutex<HashMap<Handle, FileValueConfig>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Register a `file_value` node with its configuration.
pub fn register(
    handle: Handle,
    path: String,
    attrs: Vec<(String, String)>,
    kv: Arc<dyn KVBuffers>,
    idgen: Arc<IdGen>,
    async_runtime: tokio::runtime::Handle,
) {
    REGISTRY.lock().insert(
        handle,
        FileValueConfig { path, attrs, kv, idgen, async_runtime },
    );
}

/// Take the configuration for a `file_value` node. Returns `None` if not registered.
pub fn take(handle: Handle) -> Option<FileValueConfig> {
    REGISTRY.lock().remove(&handle)
}
