use std::sync::Arc;

use async_trait::async_trait;

use super::buffer::Buffer;
use super::types::{KVBuffers, KVError, KVStat, OpenMode};
use crate::var_store::VarStore;

/// Read-only `KVBuffers` backend that exposes `VarStore` variables as paths of the form `/{pid}/{key}`.
/// Resolution follows the three-tier order defined by `VarStore::getenv`: per-actor, global, OS environment.
pub struct VarKV {
    var_store: Arc<VarStore>,
}

impl VarKV {
    pub fn new(var_store: Arc<VarStore>) -> Self {
        Self { var_store }
    }

    fn open_impl(&self, path: &str) -> Result<Buffer, KVError> {
        let (pid, key) = parse_path(path)?;
        let value = self
            .var_store
            .getenv(pid, key)
            .ok_or_else(|| KVError::NotFound(path.to_string()))?;
        let buf = Buffer::new();
        buf.append(value.as_bytes())?;
        Ok(buf)
    }

    fn listdir_impl(&self, dir: &str) -> Result<Vec<String>, KVError> {
        let pid_str = dir.trim_matches('/');
        if pid_str.is_empty() || pid_str.contains('/') {
            return Err(KVError::Backend(
                "VarKV: listdir requires a concrete pid path like N/".to_string(),
            ));
        }
        let pid: i64 = pid_str
            .parse()
            .map_err(|_| KVError::Backend("VarKV: listdir requires a numeric pid".to_string()))?;
        Ok(self
            .var_store
            .keysenv(pid)
            .into_iter()
            .map(|k| format!("/{pid}/{k}"))
            .collect())
    }
}

fn parse_path(path: &str) -> Result<(i64, &str), KVError> {
    let rest = path
        .strip_prefix('/')
        .ok_or_else(|| KVError::NotFound(path.to_string()))?;
    let slash = rest
        .find('/')
        .ok_or_else(|| KVError::NotFound(path.to_string()))?;
    let pid: i64 = rest[..slash]
        .parse()
        .map_err(|_| KVError::NotFound(path.to_string()))?;
    Ok((pid, &rest[slash + 1..]))
}

#[async_trait]
impl KVBuffers for VarKV {
    async fn open(&self, path: &str, mode: OpenMode) -> Result<Buffer, KVError> {
        match mode {
            OpenMode::Read => self.open_impl(path),
            OpenMode::Write | OpenMode::Append => Err(KVError::Backend(
                "VarKV: env vars are read-only".to_string(),
            )),
        }
    }

    async fn stat(&self, _path: &str) -> Result<KVStat, KVError> {
        Err(KVError::Backend("VarKV: stat is not supported".to_string()))
    }

    async fn listdir(&self, dir_name: &str) -> Result<Vec<String>, KVError> {
        self.listdir_impl(dir_name)
    }

    async fn destroy(&self) -> Result<(), KVError> {
        Ok(())
    }

    async fn flush_buffer(&self, _buffer: &Buffer) -> Result<(), KVError> {
        Ok(())
    }
}
