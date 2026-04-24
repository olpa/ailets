/// POSIX errno constants used by the actor runtime error propagation chain.

/// Broken pipe: a reader receives this when the writer closed with any error.
pub const EPIPE: i32 = 32;

/// Owner died: set on actor output files when the actor terminates abnormally.
pub const EOWNERDEAD: i32 = 130;

/// Operation cancelled: set on nodes whose dependency failed and can never run.
pub const ECANCELED: i32 = 125;
