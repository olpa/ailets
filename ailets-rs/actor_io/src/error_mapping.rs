//! Error mapping utilities for converting between error types.
//!
//! This module provides functions to convert errno values to `embedded_io::ErrorKind`
//! and to convert error kinds to human-readable static strings.

/// Convert errno to `embedded_io::ErrorKind`
#[must_use]
#[allow(clippy::match_same_arms)] // We explicitly list common errno values for documentation
pub fn errno_to_error_kind(errno: i32) -> embedded_io::ErrorKind {
    match errno {
        1 | 13 => embedded_io::ErrorKind::PermissionDenied, // EPERM, EACCES
        2 => embedded_io::ErrorKind::NotFound,              // ENOENT
        9 | 22 => embedded_io::ErrorKind::InvalidInput,     // EBADF, EINVAL
        12 | 28 => embedded_io::ErrorKind::OutOfMemory,     // ENOMEM, ENOSPC (no space left)
        24 => embedded_io::ErrorKind::Unsupported,          // EMFILE (too many open files)
        32 => embedded_io::ErrorKind::BrokenPipe,           // EPIPE
        // EIO, EAGAIN/EWOULDBLOCK, ECONNRESET, ETIMEDOUT, ECONNREFUSED
        5 | 11 | 104 | 110 | 111 => embedded_io::ErrorKind::Other,
        _ => embedded_io::ErrorKind::Other,
    }
}

/// Convert `embedded_io::ErrorKind` to `std::io::Error`, preserving the error kind.
#[cfg(feature = "std")]
#[must_use]
pub fn embedded_io_to_std_error(kind: embedded_io::ErrorKind) -> std::io::Error {
    let std_kind = match kind {
        embedded_io::ErrorKind::NotFound => std::io::ErrorKind::NotFound,
        embedded_io::ErrorKind::PermissionDenied => std::io::ErrorKind::PermissionDenied,
        embedded_io::ErrorKind::ConnectionRefused => std::io::ErrorKind::ConnectionRefused,
        embedded_io::ErrorKind::ConnectionReset => std::io::ErrorKind::ConnectionReset,
        embedded_io::ErrorKind::ConnectionAborted => std::io::ErrorKind::ConnectionAborted,
        embedded_io::ErrorKind::NotConnected => std::io::ErrorKind::NotConnected,
        embedded_io::ErrorKind::AddrInUse => std::io::ErrorKind::AddrInUse,
        embedded_io::ErrorKind::AddrNotAvailable => std::io::ErrorKind::AddrNotAvailable,
        embedded_io::ErrorKind::BrokenPipe => std::io::ErrorKind::BrokenPipe,
        embedded_io::ErrorKind::AlreadyExists => std::io::ErrorKind::AlreadyExists,
        embedded_io::ErrorKind::InvalidInput => std::io::ErrorKind::InvalidInput,
        embedded_io::ErrorKind::InvalidData => std::io::ErrorKind::InvalidData,
        embedded_io::ErrorKind::TimedOut => std::io::ErrorKind::TimedOut,
        embedded_io::ErrorKind::Interrupted => std::io::ErrorKind::Interrupted,
        embedded_io::ErrorKind::Unsupported => std::io::ErrorKind::Unsupported,
        embedded_io::ErrorKind::OutOfMemory => std::io::ErrorKind::OutOfMemory,
        _ => std::io::ErrorKind::Other,
    };
    std::io::Error::new(std_kind, error_kind_to_str(kind))
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
