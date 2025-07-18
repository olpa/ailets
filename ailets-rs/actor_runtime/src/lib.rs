use std::ffi::c_char;

mod actor_runtime;
mod dagops;

pub use actor_runtime::{aclose, aread, awrite, get_errno, open_read, open_write};
pub use dagops::{
    alias, alias_fd, detach_from_alias, instantiate_with_deps, open_write_pipe, value_node,
};

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

/// Look deep into an error and return the errno.
#[must_use]
#[allow(clippy::borrowed_box)]
pub fn extract_errno(e: &Box<dyn std::error::Error>) -> i32 {
    if let Some(io_err) = e.downcast_ref::<std::io::Error>() {
        if let Some(code) = io_err.raw_os_error() {
            return code;
        }
    }
    if let Some(scan_json::Error::RJiterError(rj_err)) = e.downcast_ref::<scan_json::Error>() {
        if let scan_json::rjiter::error::ErrorType::IoError(ref io_err) = rj_err.error_type {
            return io_err.raw_os_error().unwrap_or(-1);
        }
    }
    -1
}

// Annotate an io error
#[must_use]
#[allow(clippy::missing_panics_doc)]
pub fn annotate_error(
    e: Box<dyn std::error::Error>,
    annotation: &str,
) -> Box<dyn std::error::Error> {
    if let Some(io_err) = e.downcast_ref::<std::io::Error>() {
        let msg = format!("{annotation}: {io_err}");
        return Box::new(std::io::Error::new(io_err.kind(), msg));
    }
    if let Some(scan_json::Error::RJiterError(rj_err)) = e.downcast_ref::<scan_json::Error>() {
        if let scan_json::rjiter::error::ErrorType::IoError(ref io_err) = rj_err.error_type {
            let msg = format!("{annotation}: {io_err}");
            return Box::new(std::io::Error::new(io_err.kind(), msg));
        }
    }
    e
}
