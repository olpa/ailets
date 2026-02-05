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
pub mod memkv;
pub mod types;

pub use buffer::{Buffer, BufferError, BufferReadGuard};
pub use memkv::MemKV;
pub use types::{KVBuffers, KVError, OpenMode};
