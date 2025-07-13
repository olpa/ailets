pub mod rc_writer;
pub mod vfs;

pub use rc_writer::RcWriter;
pub use vfs::{add_file, clear_mocks, get_file, IO_INTERRUPT, WANT_ERROR};
#[cfg(feature = "dagops")]
pub use vfs::{dag_value_node, open_write_value_node};
