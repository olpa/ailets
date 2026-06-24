//! Environment - high-level orchestration for the actor system
//!
//! `Environment` is the single handle for both the build phase and run phase.
//! All fields are `Arc`-wrapped, so `clone()` is a pure reference-count increment.
//! Wrap in `Arc<Environment>` for background or concurrent execution.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;

use crate::dag::{Dag, DependsOn, For, NodeKind, NodeState};
use crate::storage::varkv::VarKV;
use crate::var_store::VarStore;
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
    pub env_service: Arc<VarStore>,
}

impl Environment {
    /// Create a new environment
    pub fn new(kv: Arc<dyn KVBuffers>) -> Self {
        let idgen = Arc::new(IdGen::new());
        let dag = Arc::new(RwLock::new(Dag::new(Arc::clone(&idgen))));

        let var_store = Arc::new(VarStore::new());
        let kv_with_vars = Arc::new(VarKV::new(kv, Arc::clone(&var_store))) as Arc<dyn KVBuffers>;
        let pipe_pool = Arc::new(PipePool::new(Arc::clone(&kv_with_vars)));
        Self {
            dag,
            idgen,
            kv: kv_with_vars,
            pipe_pool,
            actor_registry: Arc::new(RwLock::new(ActorRegistry::new())),
            suspension: Arc::new(SuspensionState::new()),
            env_service: var_store,
        }
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

    /// Add an alias node pointing to a single target.
    ///
    /// Calling this multiple times with the same alias name adds targets to
    /// the same alias node.
    #[must_use]
    pub fn add_alias(&self, alias_name: String, target: Handle) -> Handle {
        self.add_aliases(alias_name, &[target])
    }

    /// Add an alias node pointing to one or more targets.
    ///
    /// If an alias node with the same name already exists, the targets are
    /// added to it and its handle is returned.
    #[must_use]
    pub fn add_aliases(&self, alias_name: String, targets: &[Handle]) -> Handle {
        let mut dag = self.dag.write();
        let existing = dag
            .nodes()
            .find(|n| n.kind == NodeKind::Alias && n.idname == alias_name)
            .map(|n| n.pid);
        let handle = existing.unwrap_or_else(|| dag.add_node(alias_name, NodeKind::Alias));
        for &target in targets {
            dag.add_dependency(For(handle), DependsOn(target));
        }
        handle
    }

    /// Resolve a handle to all concrete nodes it refers to.
    ///
    /// For a concrete node, returns `[handle]`.
    /// For an alias, recursively expands to all reachable concrete nodes.
    #[must_use]
    pub fn resolve_all(&self, handle: Handle) -> Vec<Handle> {
        let dag = self.dag.read();
        match dag.get_node(handle).map(|n| &n.kind) {
            Some(NodeKind::Alias) => dag.resolve_dependencies(handle).collect(),
            Some(NodeKind::Concrete) | None => vec![handle],
        }
    }
}
