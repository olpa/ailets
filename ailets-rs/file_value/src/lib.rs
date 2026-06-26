//! Actor: reads a file or stdin and writes raw bytes to stdout.
//!
//! Path is read from `/var/{pid}/path`.

use actor_io::AWriter;
use actor_runtime::var_access::read_var;
use actor_runtime::{ActorRuntime, StdHandle};

/// # Errors
/// Returns an error if I/O fails or the path var is missing.
pub fn execute(runtime: &dyn ActorRuntime) -> Result<(), String> {
    let path = read_var(runtime, "path")?
        .ok_or_else(|| "file_value: 'path' var not set".to_string())?;

    let mut src: Box<dyn std::io::Read> = if path == "-" {
        Box::new(std::io::stdin())
    } else {
        let f = std::fs::File::open(&path)
            .map_err(|e| format!("file_value: failed to open '{path}': {e}"))?;
        Box::new(f)
    };
    let mut writer = AWriter::new_from_std(runtime, StdHandle::Stdout);

    std::io::copy(&mut src, &mut writer)
        .map_err(|e| format!("file_value: copy error: {e}"))?;

    Ok(())
}
