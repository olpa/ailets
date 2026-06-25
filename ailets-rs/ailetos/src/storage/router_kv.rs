//! `KVBuffers` multiplexer: paths under `/var` are dispatched to `VarKV` (with the
//! `/var` prefix stripped); all other paths are forwarded to the inner backend unchanged.
//! `listdir` results from `VarKV` have the `/var` prefix re-added before returning.

use std::sync::Arc;

use async_trait::async_trait;

use super::buffer::Buffer;
use super::types::{KVBuffers, KVError, KVStat, OpenMode};
use super::varkv::VarKV;

const VAR_PREFIX: &str = "/var";

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
        if let Some(var_path) = path.strip_prefix(VAR_PREFIX) {
            return self.var_kv.open(var_path, mode).await;
        }
        self.inner.open(path, mode).await
    }

    async fn stat(&self, path: &str) -> Result<KVStat, KVError> {
        if let Some(var_path) = path.strip_prefix(VAR_PREFIX) {
            return self.var_kv.stat(var_path).await;
        }
        self.inner.stat(path).await
    }

    async fn listdir(&self, dir_name: &str) -> Result<Vec<String>, KVError> {
        if let Some(var_dir) = dir_name.strip_prefix(VAR_PREFIX) {
            return self.var_kv.listdir(var_dir).await.map(|entries| {
                entries.into_iter().map(|e| format!("{VAR_PREFIX}{e}")).collect()
            });
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
