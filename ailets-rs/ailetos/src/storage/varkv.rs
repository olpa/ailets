use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;

use super::buffer::Buffer;
use super::types::{KVBuffers, KVError, KVStat, OpenMode};
use crate::var_store::VarStore;

pub(super) const ENV_PREFIX: &str = "/env/";

/// `KVBuffers` implementation for the env-var namespace.
/// Receives paths already stripped of the `/env/` prefix by `RouterKV`.
pub struct VarKV {
    var_store: Arc<VarStore>,
}

impl VarKV {
    pub fn new(var_store: Arc<VarStore>) -> Self {
        Self { var_store }
    }

    fn open_env_read(&self, env_path: &str) -> Result<Buffer, KVError> {
        let (pid, key) = parse_env_path(env_path)?;
        let value = self
            .var_store
            .getenv(pid, key)
            .ok_or_else(|| KVError::NotFound(env_path.to_string()))?;
        let buf = Buffer::new();
        buf.append(value.as_bytes())?;
        Ok(buf)
    }

    fn listdir_env(&self, env_dir: &str) -> Result<Vec<String>, KVError> {
        let pid_str = env_dir.trim_end_matches('/');
        if pid_str.is_empty() || pid_str.contains('/') {
            return Err(KVError::Backend(
                "VarKV: listdir requires a concrete pid path like N/".to_string(),
            ));
        }
        let pid: u32 = pid_str
            .parse()
            .map_err(|_| KVError::Backend("VarKV: listdir requires a numeric pid".to_string()))?;
        let mut keys: HashSet<String> = self.var_store.keys(pid).iter().map(|k| k.to_string()).collect();
        for (k, _) in std::env::vars() {
            keys.insert(k);
        }
        Ok(keys
            .into_iter()
            .map(|k| format!("{ENV_PREFIX}{pid}/{k}"))
            .collect())
    }
}

fn parse_env_path(env_path: &str) -> Result<(u32, &str), KVError> {
    let slash = env_path
        .find('/')
        .ok_or_else(|| KVError::NotFound(env_path.to_string()))?;
    let pid: u32 = env_path[..slash]
        .parse()
        .map_err(|_| KVError::NotFound(env_path.to_string()))?;
    Ok((pid, &env_path[slash + 1..]))
}

#[async_trait]
impl KVBuffers for VarKV {
    async fn open(&self, path: &str, mode: OpenMode) -> Result<Buffer, KVError> {
        match mode {
            OpenMode::Read => self.open_env_read(path),
            OpenMode::Write | OpenMode::Append => {
                Err(KVError::Backend("VarKV: env vars are read-only".to_string()))
            }
        }
    }

    async fn stat(&self, _path: &str) -> Result<KVStat, KVError> {
        Err(KVError::Backend("VarKV: stat is not supported".to_string()))
    }

    async fn listdir(&self, dir_name: &str) -> Result<Vec<String>, KVError> {
        self.listdir_env(dir_name)
    }

    async fn destroy(&self) -> Result<(), KVError> {
        Ok(())
    }

    async fn flush_buffer(&self, _buffer: &Buffer) -> Result<(), KVError> {
        Ok(())
    }
}
