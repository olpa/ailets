//! I/O module for ailets
//!
//! Contains storage abstractions and implementations.

pub mod kv;

pub use kv::{KVBuffer, KVBuffers, KVError, MemKV, OpenMode};
