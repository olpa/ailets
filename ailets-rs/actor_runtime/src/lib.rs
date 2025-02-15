mod actor_runtime;
#[cfg(feature = "mocked")]
pub mod mocked_actor_runtime;

pub use actor_runtime::{aclose, aread, awrite, n_of_streams, open_read, open_write};
