use std::sync::Arc;

use async_trait::async_trait;

use super::buffer::Buffer;
use super::types::{KVBuffers, KVError, KVStat, OpenMode};
use crate::var_store::VarStore;

pub struct VarKV {
    inner: Arc<dyn KVBuffers>,
    var_store: Arc<VarStore>,
}

impl VarKV {
    pub fn new(inner: Arc<dyn KVBuffers>, var_store: Arc<VarStore>) -> Self {
        Self { inner, var_store }
    }
}

#[async_trait]
impl KVBuffers for VarKV {
    async fn open(&self, path: &str, mode: OpenMode) -> Result<Buffer, KVError> {
        self.inner.open(path, mode).await
    }

    async fn stat(&self, path: &str) -> Result<KVStat, KVError> {
        self.inner.stat(path).await
    }

    async fn listdir(&self, dir_name: &str) -> Result<Vec<String>, KVError> {
        self.inner.listdir(dir_name).await
    }

    async fn destroy(&self) -> Result<(), KVError> {
        self.inner.destroy().await
    }

    async fn flush_buffer(&self, buffer: &Buffer) -> Result<(), KVError> {
        self.inner.flush_buffer(buffer).await
    }
}
