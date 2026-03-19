//! Pipe infrastructure for inter-actor communication
//!
//! This module provides the pipe primitives and management for data flow between actors:
//! - `reader`: Reader side of the pipe
//! - `writer`: Writer side of the pipe
//! - `rw_shared`: Shared state between Reader and Writer
//! - `pool`: Manages output pipes for actors (PipePool)
//! - `merge`: Sequential reader over multiple dependency inputs (MergeReader)

mod merge;
mod pool;
mod reader;
mod rw_shared;
mod writer;

pub use merge::MergeReader;
pub use pool::{LatentState, LatentWriter, PipePool};
pub use reader::Reader;
pub use rw_shared::ReaderSharedData;
pub use writer::Writer;
