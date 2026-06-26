use crate::runtime_trait::ActorRuntime;

/// Read a single actor variable from `/var/{pid}/{key}`.
///
/// Returns `Ok(None)` when the variable is absent or empty.
///
/// # Errors
/// Returns an error if the stream is opened but reading fails.
pub fn read_var(runtime: &dyn ActorRuntime, key: &str) -> Result<Option<String>, String> {
    let pid = runtime.node_handle();
    let path = format!("/var/{pid}/{key}");
    let fd = match runtime.open_read(&path) {
        Ok(fd) => fd,
        Err(_) => return Ok(None),
    };
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        match runtime.aread(fd, &mut chunk) {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(e) => {
                runtime.aclose(fd).ok();
                return Err(format!("read {path}: errno {e}"));
            }
        }
    }
    runtime
        .aclose(fd)
        .map_err(|e| format!("close {path}: errno {e}"))?;
    Ok(String::from_utf8(buf).ok().filter(|s| !s.is_empty()))
}

/// List all variable keys registered for this actor under `/var/{pid}/`.
///
/// Returns an empty vec if `listdir` is not supported (e.g. native runtimes).
#[must_use]
pub fn list_var_keys(runtime: &dyn ActorRuntime) -> Vec<String> {
    let pid = runtime.node_handle();
    let dir = format!("/var/{pid}/");
    runtime
        .listdir(&dir)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|p| p.strip_prefix(&dir).map(str::to_owned))
        .collect()
}
