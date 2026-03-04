pub mod attachments;
pub mod dag;
pub mod environment;
pub mod idgen;
pub mod io;
pub mod merge_reader;
pub mod notification_queue;
pub mod pipe;
pub mod pipepool;
pub mod scheduler;
pub mod stub_actor_runtime;
pub mod system_runtime;

// Re-export DAG types for convenience
pub use dag::{Dag, DependsOn, For, Node, NodeKind, NodeState, OwnedDependencyIterator};

// Re-export idgen types for convenience
pub use idgen::{Handle, HandleType, IdGen, IntCanBeHandle};

// Re-export Buffer type for convenience
pub use io::{Buffer, BufferError, BufferReadGuard};

// Re-export KV types for convenience
#[cfg(feature = "sqlitekv")]
pub use io::{CoordinatorError, FlushCoordinator, FlushFn, SqliteKV};
pub use io::{KVBuffers, KVError, MemKV, OpenMode};

// Re-export PipePool and PipeAccess for convenience
pub use pipepool::{PipeAccess, PipePool};

// Re-export system runtime types for convenience
pub use system_runtime::{
    AttachmentConfig, Channel, ChannelHandle, FdTable, IoEvent, IoFuture, IoRequest,
    SendableBuffer, StdHandles, SystemRuntime,
};

// Re-export attachment functions
pub use attachments::{attach_to_stderr, attach_to_stdout};

// Re-export blocking actor runtime
pub use stub_actor_runtime::BlockingActorRuntime;

// Re-export environment types
pub use environment::{ActorFn, ActorRegistry, Environment, ValueNodeData};

// Re-export scheduler
pub use scheduler::Scheduler;
