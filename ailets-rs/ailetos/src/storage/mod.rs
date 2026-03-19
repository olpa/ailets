//! I/O module for ailets
//!
//! Contains storage abstractions and implementations.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────┐
//! │  Pipe (coordination layer)          │
//! │  - notification queue               │
//! │  - async readers waiting for writers│
//! └─────────────────────────────────────┘
//!          ▲
//!          │ uses Buffer for storage
//!          ▼
//! ┌─────────────────────────────────────┐
//! │  Buffer (shared storage)            │
//! │  - Arc<Mutex<Vec<u8>>>              │
//! │  - append() for writing             │
//! │  - lock() for reading               │
//! └─────────────────────────────────────┘
//!          ▲
//!          │ created/managed by
//!          ▼
//! ┌─────────────────────────────────────┐
//! │  KV (storage registry)              │
//! │  - names buffers by path            │
//! │  - multiple backends                │
//! └─────────────────────────────────────┘
//!      ▲         ▲           ▲
//!      │         │           │
//!   MemKV    SQLiteKV    DynamoKV
//! ```

pub mod buffer;
#[cfg(feature = "sqlitekv")]
pub mod flush_coordinator;
pub mod memkv;
#[cfg(feature = "sqlitekv")]
pub mod sqlitekv;
pub mod types;

pub use buffer::{Buffer, BufferError, BufferReadGuard};
#[cfg(feature = "sqlitekv")]
pub use flush_coordinator::{CoordinatorError, FlushCoordinator, FlushFn};
pub use memkv::MemKV;
#[cfg(feature = "sqlitekv")]
pub use sqlitekv::SqliteKV;
pub use types::{KVBuffers, KVError, OpenMode};
