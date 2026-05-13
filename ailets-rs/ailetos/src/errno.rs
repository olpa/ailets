//! POSIX errno constants used by the actor runtime error propagation chain.

/// No such file, directory, or entity.
pub const ENOENT: i32 = 2;

/// I/O error: returned when an infrastructure operation (e.g. flush) fails.
pub const EIO: i32 = 5;

/// Bad file descriptor: returned when a channel or fd is not found.
pub const EBADF: i32 = 9;

/// Too many open files: returned when the fd table overflows.
pub const EMFILE: i32 = 24;

/// No space left on device: returned when a buffer is full.
pub const ENOSPC: i32 = 28;

/// Broken pipe: a reader receives this when the writer closed with any error.
pub const EPIPE: i32 = 32;

/// Function not implemented: returned for syscalls that are not yet supported.
pub const ENOSYS: i32 = 38;

/// Owner died: set on actor output files when the actor terminates abnormally.
pub const EOWNERDEAD: i32 = 130;

/// Operation cancelled: set on nodes whose dependency failed and can never run.
pub const ECANCELED: i32 = 125;
