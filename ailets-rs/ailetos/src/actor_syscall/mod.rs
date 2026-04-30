//! Actor syscall layer — the boundary between actor user space and system internals.
//!
//! Actors are synchronous: they call blocking functions (`aread`, `awrite`, `aclose`)
//! and expect results before continuing. The system is async: pipes, storage, and
//! lifecycle management all run on a Tokio runtime. This module bridges the two.
//!
//! The design mirrors the Unix syscall interface:
//! - Actors (user space) call into this layer the way a process calls into the kernel.
//! - The layer translates blocking calls into async operations and returns results.
//! - System internals (`pipe`, `dag`, `storage`) are never touched directly by actors.
//!
//! # Components
//!
//! **Actor side** — runs on the actor's blocking thread:
//! - [`stub_actor_runtime`] — implements `ActorRuntime`; translates each blocking call
//!   into an [`IoRequest`], sends it to `IoBridge`, and blocks on the oneshot reply.
//! - [`fd_table`] — per-actor POSIX fd → [`ChannelHandle`] map; owned by `BlockingActorRuntime`.
//!
//! **Bridge** — runs on the Tokio runtime:
//! - [`io_bridge`] — async event loop; receives [`IoRequest`]s, dispatches to pipes and
//!   storage, and sends replies. See its module doc for the full architecture, including
//!   why handlers use `Box::pin(async move { ... })` and how `FuturesUnordered` works.
//!
//! **Supporting types:**
//! - [`sendable_buffer`] — wraps a raw pointer to an actor's stack buffer so it can
//!   cross the thread boundary safely; valid only while the actor is blocked on the reply.
//! - [`lifecycle_event`] — signals sent from `IoBridge` to the executor when an actor's
//!   I/O teardown progresses; the executor replies to synchronise DAG state updates.
//!
//! # Data flow (read example)
//!
//! ```text
//! actor thread                      Tokio runtime
//! ────────────────────────────────  ──────────────────────────────
//! aread(fd)
//!   → IoRequest::Read ──────────→  IoBridge::run (tokio::select!)
//!   blocking_recv() …              handle_read() → pending_ops
//!                                  MergeReader::read().await
//!   ← (bytes, errno) ←──────────  oneshot reply
//! returns to actor
//! ```

pub mod lifecycle_event;
pub mod fd_table;
pub mod io_bridge;
pub mod sendable_buffer;
pub mod stub_actor_runtime;

pub use lifecycle_event::ActorLifecycleEvent;
pub use fd_table::{FdEntry, FdTable};
pub use io_bridge::{ChannelHandle, IoBridge, IoRequest};
pub use sendable_buffer::SendableBuffer;
pub use stub_actor_runtime::{BlockingActorRuntime, ShutdownHandle};
