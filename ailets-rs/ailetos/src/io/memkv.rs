//! In-memory implementation of KVBuffers

use super::types::{KVBuffer, KVBuffers, KVError, OpenMode};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;

/// In-memory implementation of KVBuffers
///
/// Simple hash map based storage, useful for testing and single-process use.
pub struct MemKV {
    buffers: Mutex<HashMap<String, KVBuffer>>,
}

impl MemKV {
    /// Create a new empty MemKV store
    #[must_use]
    pub fn new() -> Self {
        Self {
            buffers: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for MemKV {
    fn default() -> Self {
        Self::new()
    }
}

impl KVBuffers for MemKV {
    async fn open(&self, path: &str, mode: OpenMode) -> Result<KVBuffer, KVError> {
        let mut buffers = self.buffers.lock();

        match mode {
            OpenMode::Read => buffers
                .get(path)
                .cloned()
                .ok_or_else(|| KVError::NotFound(path.to_string())),
            OpenMode::Write => {
                let buffer = Arc::new(Mutex::new(Vec::new()));
                buffers.insert(path.to_string(), Arc::clone(&buffer));
                Ok(buffer)
            }
            OpenMode::Append => {
                if let Some(buffer) = buffers.get(path) {
                    Ok(Arc::clone(buffer))
                } else {
                    let buffer = Arc::new(Mutex::new(Vec::new()));
                    buffers.insert(path.to_string(), Arc::clone(&buffer));
                    Ok(buffer)
                }
            }
        }
    }

    async fn flush(&self, _path: &str) -> Result<(), KVError> {
        // No-op for in-memory storage
        Ok(())
    }

    async fn listdir(&self, dir_name: &str) -> Result<Vec<String>, KVError> {
        let prefix = if dir_name.ends_with('/') {
            dir_name.to_string()
        } else {
            format!("{dir_name}/")
        };

        let buffers = self.buffers.lock();
        let mut paths: Vec<String> = buffers
            .keys()
            .filter(|path| path.starts_with(&prefix))
            .cloned()
            .collect();
        paths.sort();
        Ok(paths)
    }

    async fn destroy(&self) -> Result<(), KVError> {
        let mut buffers = self.buffers.lock();
        buffers.clear();
        Ok(())
    }
}
