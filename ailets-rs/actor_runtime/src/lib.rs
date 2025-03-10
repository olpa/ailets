use std::os::raw::c_char;

mod actor_runtime;
#[cfg(feature = "dagops")]
mod dagops;

pub use actor_runtime::{aclose, aread, awrite, n_of_streams, open_read, open_write};
#[cfg(feature = "dagops")]
pub use dagops::{DagOps, DagOpsTrait};

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
