use crate::runtime_trait::ActorRuntime;
use tracing::warn;

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

// `read_var` is not expected to fail in practice, so callers don't check its error. If it does
// fail, developers need to notice — hence `warn!`. Normally a library function wouldn't log, but
// here it compensates for callers that propagate errors without inspecting them.
/// Read a single actor variable from `/var/{pid}/{key}`.
///
/// Returns `Ok(None)` when the variable is absent (`ENOENT`) or empty.
///
/// # Errors
/// Returns an error if opening fails for any reason other than `ENOENT`, or if reading fails.
pub fn read_var(runtime: &dyn ActorRuntime, key: &str) -> Result<Option<String>, String> {
    let pid = runtime.node_handle();
    let path = format!("/var/{pid}/{key}");
    let fd = match runtime.open_read(&path) {
        Ok(fd) => fd,
        Err(2) => return Ok(None), // ENOENT — variable absent
        Err(e) => {
            warn!("read_var: open {path}: errno {e}");
            return Err(format!("errno {e}"));
        }
    };
    let mut reader = RuntimeReader { runtime, fd };
    let mut buf = Vec::new();
    if let Err(e) = std::io::copy(&mut reader, &mut buf) {
        runtime.aclose(fd).ok();
        warn!("read_var: read {path}: {e}");
        return Err(e.to_string());
    }
    if let Err(e) = runtime.aclose(fd) {
        warn!("read_var: close {path}: errno {e}");
        return Err(format!("errno {e}"));
    }
    if buf.is_empty() {
        return Ok(None);
    }
    String::from_utf8(buf).map(Some).map_err(|e| {
        warn!("read_var: decode {path}: {e}");
        e.to_string()
    })
}

/// List all variable keys registered for this actor under `/var/{pid}/`.
///
/// Returns an empty iterator if `listdir` is not supported (e.g. native runtimes).
#[must_use]
pub fn list_var_keys(runtime: &dyn ActorRuntime) -> impl Iterator<Item = String> {
    let pid = runtime.node_handle();
    let dir = format!("/var/{pid}/");
    runtime
        .listdir(&dir)
        .unwrap_or_default()
        .into_iter()
        .filter_map(move |p| p.strip_prefix(&dir).map(str::to_owned))
}
