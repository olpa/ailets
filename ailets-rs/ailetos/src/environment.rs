//! Environment - high-level orchestration for the actor system
//!
//! `Environment` is the single handle for both the build phase and run phase.
//! All fields are `Arc`-wrapped, so `clone()` is a pure reference-count increment.
//! Wrap in `Arc<Environment>` for background or concurrent execution.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;

use crate::dag::{Dag, DependsOn, For, NodeKind, NodeState};
use crate::pipe::PipePool;

/// Type for actor functions
pub type ActorFn = fn(&dyn actor_runtime::ActorRuntime) -> Result<(), String>;
use crate::idgen::{Handle, IdGen};
use crate::suspension::SuspensionState;
use crate::{KVBuffers, KVError};

/// Registry mapping actor names to their implementation functions
#[derive(Clone)]
pub struct ActorRegistry {
    actors: HashMap<String, ActorFn>,
}

impl ActorRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            actors: HashMap::new(),
        }
    }

    /// Register an actor function
    pub fn register(&mut self, name: impl Into<String>, actor_fn: ActorFn) {
        self.actors.insert(name.into(), actor_fn);
    }

    /// Get an actor function by name
    #[must_use]
    pub fn get(&self, name: &str) -> Option<ActorFn> {
        self.actors.get(name).copied()
    }
}

impl Default for ActorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct Environment {
    pub dag: Arc<RwLock<Dag>>,
    pub idgen: Arc<IdGen>,
    pub kv: Arc<dyn KVBuffers>,
    pub pipe_pool: Arc<PipePool>,
    pub actor_registry: Arc<RwLock<ActorRegistry>>,
    pub suspension: Arc<SuspensionState>,
    pub(crate) attachment_config: Arc<RwLock<crate::attachments::AttachmentConfig>>,
}

impl Environment {
    /// Create a new environment
    pub fn new(kv: Arc<dyn KVBuffers>) -> Self {
        let idgen = Arc::new(IdGen::new());
        let dag = Arc::new(RwLock::new(Dag::new(Arc::clone(&idgen))));

        let pipe_pool = Arc::new(PipePool::new(Arc::clone(&kv)));
        Self {
            dag,
            idgen,
            kv,
            pipe_pool,
            actor_registry: Arc::new(RwLock::new(ActorRegistry::new())),
            suspension: Arc::new(SuspensionState::new()),
            attachment_config: Arc::new(RwLock::new(
                crate::attachments::AttachmentConfig::default(),
            )),
        }
    }

    /// Attach an actor's stdout to a custom writer (e.g. a terminal-aware sink, or host stdout).
    /// The writer is consumed the first time the actor's stdout is realized.
    pub fn attach_stdout_to(
        &self,
        actor_handle: Handle,
        sink: Box<dyn std::io::Write + Send + Sync>,
    ) {
        self.attachment_config.write().attach_to_sink(actor_handle, sink);
    }

    /// Add a value node - a node that outputs a constant value
    ///
    /// # Arguments
    /// * `data` - The bytes to write to the node's output
    /// * `explain` - Optional explanation of what this value represents
    ///
    /// # Errors
    /// Returns `KVError` if writing the data to KV storage fails
    pub async fn add_value_node(
        &self,
        data: Vec<u8>,
        explain: Option<String>,
    ) -> Result<Handle, KVError> {
        use crate::pipe::{pipe_path, write_completed_buffer};
        use actor_runtime::StdHandle;

        let handle = {
            let mut dag = self.dag.write();
            let handle = dag.add_node_with_explain("value".into(), NodeKind::Concrete, explain);

            // Value nodes are considered "built" at creation since their output is static
            dag.set_state(handle, NodeState::Terminated);
            handle
        };

        // Write data to KV storage immediately (spec://executor.md#immediate-values)
        let path = pipe_path(handle, StdHandle::Stdout as isize);
        write_completed_buffer(self.kv.as_ref(), &path, &data).await?;

        Ok(handle)
    }

    /// Add a regular node with dependencies
    ///
    /// # Arguments
    /// * `idname` - Name/type of the actor (e.g., "stdin", "cat")
    /// * `deps` - List of dependency node handles
    /// * `explain` - Optional explanation
    #[must_use]
    pub fn add_node(&self, idname: String, deps: &[Handle], explain: Option<String>) -> Handle {
        let mut dag = self.dag.write();
        let handle = dag.add_node_with_explain(idname, NodeKind::Concrete, explain);

        for &dep in deps {
            dag.add_dependency(For(handle), DependsOn(dep));
        }

        handle
    }

    /// Add an alias node
    #[must_use]
    pub fn add_alias(&self, alias_name: String, target: Handle) -> Handle {
        let mut dag = self.dag.write();
        let handle = dag.add_node(alias_name, NodeKind::Alias);
        dag.add_dependency(For(handle), DependsOn(target));
        handle
    }

    /// Resolve an alias node to its actual target node
    ///
    /// If the handle refers to an alias node, returns the target node.
    /// If the handle refers to a concrete node, returns the same handle.
    /// Recursively resolves nested aliases.
    #[must_use]
    pub fn resolve(&self, handle: Handle) -> Handle {
        let dag = self.dag.read();
        let Some(node) = dag.get_node(handle) else {
            return handle;
        };
        if !matches!(node.kind, crate::dag::NodeKind::Alias) {
            return handle;
        }
        // Alias node - get its dependency (should be exactly one)
        let target = dag.get_direct_dependencies(handle).next();
        drop(dag);
        // Recursively resolve in case the target is also an alias
        target.map_or(handle, |t| self.resolve(t))
    }
}
