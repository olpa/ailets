use crate::runtime_trait::ActorRuntime;

struct RuntimeReader<'a> {
    runtime: &'a dyn ActorRuntime,
    fd: isize,
}

impl std::io::Read for RuntimeReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.runtime
            .aread(self.fd, buf)
            .map_err(std::io::Error::from_raw_os_error)
    }
}

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
    let mut reader = RuntimeReader { runtime, fd };
    let mut buf = Vec::new();
    if let Err(e) = std::io::copy(&mut reader, &mut buf) {
        runtime.aclose(fd).ok();
        return Err(format!("read {path}: {e}"));
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
