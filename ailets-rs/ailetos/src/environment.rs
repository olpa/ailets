//! Environment - high-level orchestration for the actor system
//!
//! This module provides two structs with distinct responsibilities:
//!
//! - `Environment` — build phase. Owns all mutable state. Use `&mut self` methods
//!   to construct the DAG, register actors, and configure attachments. Nothing here
//!   is safe to share across threads.
//!
//! - `RunHandle` — run phase. Created from `Environment::make_run_handle()`. All
//!   fields are either `Arc`-wrapped or value-snapshotted, so it is cheap to wrap
//!   in `Arc` for background or concurrent execution. The `dag` and `suspension`
//!   fields are shared with the originating `Environment`, allowing the build-phase
//!   owner to observe and control running actors.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use tokio::sync::mpsc;
use tracing::{debug, error, warn};

use crate::dag::{Dag, DependsOn, For, NodeKind, NodeState};
use crate::idgen::{Handle, IdGen};
use crate::scheduler::{Scheduler, StopConditions};
use crate::suspension::SuspensionState;
use crate::pipe::PipePool;
use crate::{BlockingActorRuntime, IoRequest, KVBuffers, KVError, ShutdownHandle, SystemRuntime};

/// Type for actor functions
pub type ActorFn = fn(BlockingActorRuntime) -> Result<(), String>;

/// Decide whether a node is ready to be spawned.
///
/// Decision table (iterate concrete deps, first decisive result wins):
///
/// | Dep state              | Has output (pipe realized) | Decision       |
/// |------------------------|----------------------------|----------------|
/// | NotStarted             | —                          | don't start    |
/// | Suspended              | —                          | don't start    |
/// | Running / Terminating  | yes                        | start          |
/// | Running / Terminating  | no                         | don't start    |
/// | Terminated             | yes                        | start          |
/// | Terminated             | no                         | skip (neutral) |
/// | (all deps exhausted)   | —                          | start          |
///
/// "Has output" is checked optimistically: a dep has output if its stdout
/// pipe is realized (writer exists in the pool), regardless of byte count.
pub fn is_ready_to_spawn<K: KVBuffers>(
    node_handle: Handle,
    dag: &Dag,
    pipe_pool: &PipePool<K>,
    suspension: &SuspensionState,
) -> bool {
    use actor_runtime::StdHandle;

    for dep in dag.resolve_dependencies(node_handle) {
        let Some(dep_node) = dag.get_node(dep) else {
            continue;
        };

        if suspension.is_suspended(dep) {
            return false;
        }

        let has_output = pipe_pool
            .get_already_realized_writer((dep, StdHandle::Stdout))
            .is_some();

        match dep_node.state {
            NodeState::NotStarted => return false,
            NodeState::Running | NodeState::Terminating => {
                return has_output;
            }
            NodeState::Terminated => {
                if has_output {
                    return true;
                }
                // neutral: no output from this dep, continue to next
            }
        }
    }

    true
}

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

/// Build-phase owner of the actor system.
///
/// Construct the DAG and configure the system here, then call
/// `make_run_handle()` to obtain a `RunHandle` for execution.
pub struct Environment<K: KVBuffers> {
    pub dag: Arc<RwLock<Dag>>,
    pub idgen: Arc<IdGen>,
    pub kv: Arc<K>,
    pub actor_registry: ActorRegistry,
    pub suspension: Arc<SuspensionState>,
    attachment_config: crate::attachments::AttachmentConfig,
}

impl<K: KVBuffers> Environment<K> {
    /// Create a new environment
    pub fn new(kv: Arc<K>) -> Self {
        let idgen = Arc::new(IdGen::new());
        let dag = Arc::new(RwLock::new(Dag::new(Arc::clone(&idgen))));

        Self {
            dag,
            idgen,
            kv,
            actor_registry: ActorRegistry::new(),
            suspension: Arc::new(SuspensionState::new()),
            attachment_config: crate::attachments::AttachmentConfig::default(),
        }
    }

    /// Attach a specific actor's stdout to host stdout
    ///
    /// The actor's stdout will be automatically attached to host stdout
    /// when it writes for the first time.
    ///
    /// Note: Actor stderr (Log handle), metrics, and tracing are always attached to
    /// host stderr for all actors.
    ///
    /// # Arguments
    /// * `actor_handle` - The handle of the actor whose stdout should be attached
    pub fn attach_stdout(&mut self, actor_handle: Handle) {
        self.attachment_config.attach_stdout(actor_handle);
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
        &mut self,
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
        let path = pipe_path(handle, StdHandle::Stdout);
        write_completed_buffer(self.kv.as_ref(), &path, &data).await?;

        Ok(handle)
    }

    /// Add a regular node with dependencies
    ///
    /// # Arguments
    /// * `idname` - Name/type of the actor (e.g., "stdin", "cat")
    /// * `deps` - List of dependency node handles
    /// * `explain` - Optional explanation
    pub fn add_node(&mut self, idname: String, deps: &[Handle], explain: Option<String>) -> Handle {
        let mut dag = self.dag.write();
        let handle = dag.add_node_with_explain(idname, NodeKind::Concrete, explain);

        for &dep in deps {
            dag.add_dependency(For(handle), DependsOn(dep));
        }

        handle
    }

    /// Add an alias node
    pub fn add_alias(&mut self, alias_name: String, target: Handle) -> Handle {
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

    /// Create a `RunHandle` from this environment.
    ///
    /// Clones the `Arc`-based shared fields and snapshots `attachment_config`
    /// and `actor_registry` at this point in time. The returned handle can be
    /// wrapped in `Arc` for concurrent or background execution.
    ///
    /// Actors registered or attachments configured after this call will not be
    /// visible to the returned handle.
    pub fn make_run_handle(&self) -> RunHandle<K> {
        RunHandle {
            dag: Arc::clone(&self.dag),
            kv: Arc::clone(&self.kv),
            idgen: Arc::clone(&self.idgen),
            attachment_config: self.attachment_config.clone(),
            actor_registry: self.actor_registry.clone(),
            suspension: Arc::clone(&self.suspension),
        }
    }

    /// Convenience: run the environment directly without managing a `RunHandle`.
    ///
    /// For background execution, use `make_run_handle()` and wrap the result in
    /// `Arc` instead.
    pub async fn run(&self, target: Handle, stop_conditions: StopConditions)
    where
        K: 'static,
    {
        self.make_run_handle().run(target, stop_conditions).await;
    }
}

/// Run-phase handle for an actor system.
///
/// Obtained from `Environment::make_run_handle()`. All fields are either
/// `Arc`-wrapped or snapshotted values, so this struct is cheap to wrap in
/// `Arc` for background or concurrent execution.
///
/// `dag` and `suspension` are shared with the originating `Environment`,
/// allowing the build-phase owner to observe and resume running actors.
pub struct RunHandle<K: KVBuffers> {
    pub dag: Arc<RwLock<Dag>>,
    pub suspension: Arc<SuspensionState>,
    kv: Arc<K>,
    idgen: Arc<IdGen>,
    attachment_config: crate::attachments::AttachmentConfig,
    actor_registry: ActorRegistry,
}

impl<K: KVBuffers> RunHandle<K> {
    /// Spawn a task for an actor node
    fn spawn_actor_task(
        node_handle: Handle,
        idname: String,
        actor_fn: ActorFn,
        actor_runtime: BlockingActorRuntime,
        shutdown: ShutdownHandle,
    ) -> tokio::task::JoinHandle<()> {
        tokio::task::spawn_blocking(move || {
            debug!(node = ?node_handle, name = %idname, "task starting");

            actor_runtime.register_std_fds();

            let result = actor_fn(actor_runtime);

            match result {
                Ok(()) => {
                    debug!(node = ?node_handle, name = %idname, "task completed");
                }
                Err(e) => {
                    warn!(node = ?node_handle, name = %idname, error = %e, "task error");
                }
            }

            debug!(node = ?node_handle, name = %idname, "task done, shutdown via Drop");
            drop(shutdown);
        })
    }

    /// Spawn actor tasks for nodes whose dependencies are already running or terminated.
    ///
    /// Implements spec://executor.md#on-demand-spawn: actor spawning is deferred until
    /// input is available. A node is ready when all its concrete dependencies are in
    /// Running, Terminating, or Terminated state (not NotStarted).
    ///
    /// Returns the newly spawned tasks. Call in a loop until the result is empty.
    fn spawn_ready_actor_tasks(
        dag: &Arc<RwLock<Dag>>,
        target: Handle,
        stop_conditions: &StopConditions,
        system_tx: &mpsc::UnboundedSender<IoRequest>,
        actor_registry: &ActorRegistry,
        suspension: &Arc<SuspensionState>,
    ) -> Vec<tokio::task::JoinHandle<()>> {
        let nodes_to_spawn: Vec<(Handle, String)> = {
            let dag_guard = dag.read();
            let scheduler =
                Scheduler::with_stop_conditions(&dag_guard, target, stop_conditions.clone());
            scheduler
                .iter()
                .filter_map(|node_handle| {
                    let node = dag_guard.get_node(node_handle)?;
                    if node.state != NodeState::NotStarted {
                        return None;
                    }
                    let all_deps_ready = dag_guard
                        .resolve_dependencies(node_handle)
                        .all(|dep| {
                            dag_guard
                                .get_node(dep)
                                .map_or(false, |n| n.state != NodeState::NotStarted)
                        });
                    if all_deps_ready {
                        Some((node_handle, node.idname.clone()))
                    } else {
                        None
                    }
                })
                .collect()
        };

        let mut tasks = Vec::new();

        for (node_handle, idname) in nodes_to_spawn {
            debug!(node = ?node_handle, name = %idname, "spawning actor task");

            dag.write().set_state(node_handle, NodeState::Running);

            let (actor_runtime, shutdown) =
                BlockingActorRuntime::new(node_handle, system_tx.clone(), Arc::clone(suspension));

            if let Some(actor_fn) = actor_registry.get(&idname) {
                let task = Self::spawn_actor_task(node_handle, idname, actor_fn, actor_runtime, shutdown);
                tasks.push(task);
            } else {
                warn!(node = ?node_handle, name = %idname, "actor not registered, skipping");
            }
        }

        tasks
    }

    /// Run the system: spawn system runtime and actor tasks, wait for completion
    pub async fn run(&self, target: Handle, stop_conditions: StopConditions)
    where
        K: 'static,
    {
        let pipe_pool = Arc::new(PipePool::new(Arc::clone(&self.kv)));

        // Create system runtime with a snapshot of the attachment configuration
        let system_runtime = SystemRuntime::new(
            Arc::clone(&self.dag),
            Arc::clone(&self.kv),
            Arc::clone(&self.idgen),
            self.attachment_config.clone(),
            Arc::clone(&pipe_pool),
        );

        // Get sender before moving system_runtime
        let Some(system_tx) = system_runtime.get_system_tx() else {
            error!("Failed to get system_tx - system runtime already started");
            return;
        };

        // Spawn SystemRuntime task
        let system_task = tokio::spawn(async move {
            system_runtime.run().await;
        });

        // Spawn actor tasks on-demand: loop until no new nodes become ready.
        // Each pass sets newly spawned nodes to Running, unlocking the next tier.
        let mut actor_tasks = Vec::new();
        loop {
            let new_tasks = Self::spawn_ready_actor_tasks(
                &self.dag,
                target,
                &stop_conditions,
                &system_tx,
                &self.actor_registry,
                &self.suspension,
            );
            if new_tasks.is_empty() {
                break;
            }
            actor_tasks.extend(new_tasks);
        }

        // Drop our sender so the channel can close when all actors finish
        drop(system_tx);

        // Wait for system runtime
        if let Err(e) = system_task.await {
            warn!(error = %e, "SystemRuntime task failed");
        }

        // Wait for all actor tasks
        for task in actor_tasks {
            if let Err(e) = task.await {
                warn!(error = %e, "actor task failed");
            }
        }
    }
}
