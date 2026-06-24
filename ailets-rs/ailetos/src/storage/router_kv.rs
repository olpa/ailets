use std::sync::Arc;

use async_trait::async_trait;

use super::buffer::Buffer;
use super::types::{KVBuffers, KVError, KVStat, OpenMode};
use super::varkv::{VarKV, ENV_PREFIX};

/// Routes KV paths to the appropriate backend.
/// Strips the path prefix before dispatching so each backend sees only its own namespace.
pub struct RouterKV {
    inner: Arc<dyn KVBuffers>,
    var_kv: VarKV,
}

impl RouterKV {
    pub fn new(inner: Arc<dyn KVBuffers>, var_kv: VarKV) -> Self {
        Self { inner, var_kv }
    }
}

#[async_trait]
impl KVBuffers for RouterKV {
    async fn open(&self, path: &str, mode: OpenMode) -> Result<Buffer, KVError> {
        if let Some(env_path) = path.strip_prefix(ENV_PREFIX) {
            return self.var_kv.open(env_path, mode).await;
        }
        self.inner.open(path, mode).await
    }

    async fn stat(&self, path: &str) -> Result<KVStat, KVError> {
        if let Some(env_path) = path.strip_prefix(ENV_PREFIX) {
            return self.var_kv.stat(env_path).await;
        }
        self.inner.stat(path).await
    }

    async fn listdir(&self, dir_name: &str) -> Result<Vec<String>, KVError> {
        if let Some(env_dir) = dir_name.strip_prefix(ENV_PREFIX) {
            return self.var_kv.listdir(env_dir).await;
        }
        self.inner.listdir(dir_name).await
    }

    async fn destroy(&self) -> Result<(), KVError> {
        self.inner.destroy().await
    }

    async fn flush_buffer(&self, buffer: &Buffer) -> Result<(), KVError> {
        self.inner.flush_buffer(buffer).await
    }
}
