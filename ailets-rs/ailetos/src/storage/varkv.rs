use std::collections::HashSet;
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

    fn open_env_read(&self, env_path: &str) -> Result<Buffer, KVError> {
        let (pid, key) = parse_env_path(env_path)?;
        let value = self
            .var_store
            .getenv(pid, key)
            .ok_or_else(|| KVError::NotFound(format!("/env/{env_path}")))?;
        let buf = Buffer::new();
        buf.append(value.as_bytes())?;
        Ok(buf)
    }

    fn listdir_env(&self, env_dir: &str) -> Result<Vec<String>, KVError> {
        let pid_str = env_dir.trim_end_matches('/');
        let pid: u32 = pid_str
            .parse()
            .map_err(|_| KVError::NotFound(format!("/env/{env_dir}")))?;
        let mut keys: HashSet<String> = self.var_store.keys(pid).into_iter().collect();
        for (k, _) in std::env::vars() {
            keys.insert(k);
        }
        Ok(keys
            .into_iter()
            .map(|k| format!("/env/{pid}/{k}"))
            .collect())
    }
}

fn parse_env_path(env_path: &str) -> Result<(u32, &str), KVError> {
    let slash = env_path
        .find('/')
        .ok_or_else(|| KVError::NotFound(format!("/env/{env_path}")))?;
    let pid: u32 = env_path[..slash]
        .parse()
        .map_err(|_| KVError::NotFound(format!("/env/{env_path}")))?;
    Ok((pid, &env_path[slash + 1..]))
}

#[async_trait]
impl KVBuffers for VarKV {
    async fn open(&self, path: &str, mode: OpenMode) -> Result<Buffer, KVError> {
        if let Some(env_path) = path.strip_prefix("/env/") {
            return match mode {
                OpenMode::Read => self.open_env_read(env_path),
                OpenMode::Write | OpenMode::Append => {
                    Err(KVError::Backend("env vars are read-only".to_string()))
                }
            };
        }
        self.inner.open(path, mode).await
    }

    async fn stat(&self, path: &str) -> Result<KVStat, KVError> {
        if path.starts_with("/env/") {
            return Err(KVError::NotFound(path.to_string()));
        }
        self.inner.stat(path).await
    }

    async fn listdir(&self, dir_name: &str) -> Result<Vec<String>, KVError> {
        if let Some(env_dir) = dir_name.strip_prefix("/env/") {
            return self.listdir_env(env_dir);
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
