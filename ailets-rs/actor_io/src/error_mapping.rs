//! Error mapping utilities for converting between error types.
//!
//! This module provides functions to convert errno values to `embedded_io::ErrorKind`
//! and to convert error kinds to human-readable static strings.

use core::ffi::c_int;

/// Convert errno to `embedded_io::ErrorKind`
#[must_use]
#[allow(clippy::match_same_arms)] // We explicitly list common errno values for documentation
pub fn errno_to_error_kind(errno: c_int) -> embedded_io::ErrorKind {
    match errno {
        1 | 13 => embedded_io::ErrorKind::PermissionDenied, // EPERM, EACCES
        2 => embedded_io::ErrorKind::NotFound,              // ENOENT
        9 | 22 => embedded_io::ErrorKind::InvalidInput,     // EBADF, EINVAL
        12 | 28 => embedded_io::ErrorKind::OutOfMemory,     // ENOMEM, ENOSPC (no space left)
        24 => embedded_io::ErrorKind::Unsupported,          // EMFILE (too many open files)
        // EIO, EAGAIN/EWOULDBLOCK, EPIPE, ECONNRESET, ETIMEDOUT, ECONNREFUSED
        5 | 11 | 32 | 104 | 110 | 111 => embedded_io::ErrorKind::Other,
        _ => embedded_io::ErrorKind::Other,
    }
}

/// Convert error kind to a static string description
#[must_use]
pub fn error_kind_to_str(kind: embedded_io::ErrorKind) -> &'static str {
    match kind {
        embedded_io::ErrorKind::NotFound => "not found",
        embedded_io::ErrorKind::PermissionDenied => "permission denied",
        embedded_io::ErrorKind::ConnectionRefused => "connection refused",
        embedded_io::ErrorKind::ConnectionReset => "connection reset",
        embedded_io::ErrorKind::ConnectionAborted => "connection aborted",
        embedded_io::ErrorKind::NotConnected => "not connected",
        embedded_io::ErrorKind::AddrInUse => "address in use",
        embedded_io::ErrorKind::AddrNotAvailable => "address not available",
        embedded_io::ErrorKind::BrokenPipe => "broken pipe",
        embedded_io::ErrorKind::AlreadyExists => "already exists",
        embedded_io::ErrorKind::InvalidInput => "invalid input",
        embedded_io::ErrorKind::InvalidData => "invalid data",
        embedded_io::ErrorKind::TimedOut => "timed out",
        embedded_io::ErrorKind::Interrupted => "interrupted",
        embedded_io::ErrorKind::Unsupported => "unsupported",
        embedded_io::ErrorKind::OutOfMemory => "out of memory",
        embedded_io::ErrorKind::Other => "other error",
        _ => "unknown error",
    }
}
