use std::ffi::c_char;

mod actor_runtime;
mod dagops;
mod ffi_runtime;
mod runtime_trait;

pub use actor_runtime::{aclose, aread, awrite, get_errno, open_read, open_write};
pub use dagops::{
    alias, alias_fd, detach_from_alias, instantiate_with_deps, open_write_pipe, value_node,
};
pub use ffi_runtime::FfiActorRuntime;
pub use runtime_trait::ActorRuntime;

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
pub fn err_to_heap_c_string(code: i32, message: &str) -> *const c_char {
    let error_json = serde_json::json!({
        "code": code,
        "message": message
    });
    let error_str = error_json.to_string();
    #[allow(clippy::unwrap_used)]
    let err = Box::leak(Box::new(std::ffi::CString::new(error_str).unwrap()));
    err.as_ptr()
}
