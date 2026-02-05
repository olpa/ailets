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
//! │  - Buffer trait (NO kv imports)     │
//! └─────────────────────────────────────┘
//!          ▲
//!          │ integration layer (future)
//!          │ implements Buffer for shared storage
//!          ▼
//! ┌─────────────────────────────────────┐
//! │  KV (storage layer)                 │
//! │  - simple async storage operations  │
//! │  - returns Arc<Mutex<Vec<u8>>>      │
//! │  - multiple backends                │
//! └─────────────────────────────────────┘
//!      ▲         ▲           ▲
//!      │         │           │
//!   MemKV    SQLiteKV    DynamoKV
//! ```

pub mod memkv;
pub mod types;

pub use memkv::MemKV;
pub use types::{KVBuffer, KVBuffers, KVError, OpenMode};
