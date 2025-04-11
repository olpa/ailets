use std::ffi::c_char;

mod actor_runtime;
#[cfg(feature = "dagops")]
mod dagops;

pub use actor_runtime::{aclose, aread, awrite, open_read, open_write};
#[cfg(feature = "dagops")]
pub use dagops::{DagOps, DagOpsTrait};

/// Standard handles for I/O streams.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StdHandle {
    Stdin = 0,
    Stdout = 1,
    Log = 2,
    Env = 3,
    Metrics = 4,
    Trace = 5,
}

/// Convert an error to a heap-allocated C-string.
///
/// This function is useful for returning errors to the host runtime.
#[must_use]
#[allow(clippy::missing_panics_doc)]
pub fn err_to_heap_c_string(err: &str) -> *const c_char {
    #[allow(clippy::unwrap_used)]
    let err = Box::leak(Box::new(std::ffi::CString::new(err).unwrap()));
    err.as_ptr()
}
