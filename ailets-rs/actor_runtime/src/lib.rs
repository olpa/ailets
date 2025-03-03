mod actor_runtime;
#[cfg(feature = "dagops")]
mod dagops;

pub use actor_runtime::{aclose, aread, awrite, n_of_streams, open_read, open_write};
#[cfg(feature = "dagops")]
pub use dagops::{DagOps, DagOpsTrait};
