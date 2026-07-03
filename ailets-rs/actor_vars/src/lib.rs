//! Read actor variables from `/var/{pid}/{key}`, written by the host runtime.

use actor_io::AReader;
use actor_runtime::ActorRuntime;
use tracing::warn;

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
    let mut reader = match AReader::new(runtime, &path) {
        Ok(reader) => reader,
        Err(embedded_io::ErrorKind::NotFound) => return Ok(None),
        Err(e) => {
            warn!("read_var: open {path}: {e:?}");
            return Err(format!("{e:?}"));
        }
    };
    let mut buf = Vec::new();
    if let Err(e) = std::io::Read::read_to_end(&mut reader, &mut buf) {
        warn!("read_var: read {path}: {e}");
        return Err(e.to_string());
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
pub fn list_var_keys(runtime: &dyn ActorRuntime) -> impl Iterator<Item = String> {
    let pid = runtime.node_handle();
    let dir = format!("/var/{pid}/");
    runtime
        .listdir(&dir)
        .unwrap_or_default()
        .into_iter()
        .filter_map(move |p| p.strip_prefix(&dir).map(str::to_owned))
}
