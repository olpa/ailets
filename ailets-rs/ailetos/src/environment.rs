//! Environment - high-level orchestration for the actor system
//!
//! This module provides the Environment struct, which is the main entry point
//! for building and running actor systems. It manages:
//! - DAG construction with value nodes
//! - System runtime orchestration
//! - Actor spawning and execution

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use tokio::sync::mpsc;
use tracing::{debug, error, warn};

use crate::dag::{Dag, DependsOn, For, NodeKind, NodeState};
use crate::idgen::{Handle, IdGen};
use crate::scheduler::{Scheduler, StopConditions};
use crate::suspension::SuspensionState;
use crate::{BlockingActorRuntime, IoRequest, KVBuffers, KVError, ShutdownHandle, SystemRuntime};

/// Type for actor functions
pub type ActorFn = fn(actor_io::AReader, actor_io::AWriter) -> Result<(), String>;

/// Registry mapping actor names to their implementation functions
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

/// Environment for building and running actor systems
pub struct Environment<K: KVBuffers> {
    pub dag: Arc<RwLock<Dag>>,
    pub idgen: Arc<IdGen>,
    pub kv: Arc<K>,
    pub actor_registry: ActorRegistry,
    pub suspension: Arc<SuspensionState>,
    /// Attachment configuration
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
    /// # Returns
    /// The handle to the created node
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
    ///
    /// # Returns
    /// The handle to the created node
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

    /// Spawn a task for an actor node
    fn spawn_actor_task(
        node_handle: Handle,
        idname: String,
        actor_fn: ActorFn,
        actor_runtime: BlockingActorRuntime,
        shutdown: ShutdownHandle,
    ) -> tokio::task::JoinHandle<()> {
        use actor_io::{AReader, AWriter};
        use actor_runtime::StdHandle;

        tokio::task::spawn_blocking(move || {
            debug!(node = ?actor_runtime.node_handle(), name = %idname, "task starting");

            actor_runtime.register_std_fds();

            let areader = AReader::new_from_std(&actor_runtime, StdHandle::Stdin);
            let awriter = AWriter::new_from_std(&actor_runtime, StdHandle::Stdout);

            let result = actor_fn(areader, awriter);

            match result {
                Ok(()) => {
                    debug!(node = ?actor_runtime.node_handle(), name = %idname, "task completed");
                }
                Err(e) => {
                    warn!(node = ?actor_runtime.node_handle(), name = %idname, error = %e, "task error");
                }
            }

            debug!(node = ?actor_runtime.node_handle(), name = %idname, "task done, shutdown via Drop");
            drop(shutdown);
        })
    }

    /// Spawn actor tasks for all nodes in the system
    fn spawn_actor_tasks(
        dag: &Arc<RwLock<Dag>>,
        target: Handle,
        stop_conditions: &StopConditions,
        system_tx: &mpsc::UnboundedSender<IoRequest>,
        actor_registry: &ActorRegistry,
        suspension: &Arc<SuspensionState>,
    ) -> Vec<tokio::task::JoinHandle<()>> {
        let dag_guard = dag.read();
        let scheduler =
            Scheduler::with_stop_conditions(&dag_guard, target, stop_conditions.clone());
        let mut tasks = Vec::new();

        for node_handle in scheduler.iter() {
            let Some(node) = dag_guard.get_node(node_handle) else {
                warn!(node = ?node_handle, "node not found in DAG, skipping");
                continue;
            };
            let idname = node.idname.clone();
            debug!(node = ?node_handle, name = %idname, "spawning actor task");

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
    pub async fn run(&mut self, target: Handle, stop_conditions: StopConditions)
    where
        K: 'static,
    {
        // Create system runtime with attachment configuration
        let system_runtime = SystemRuntime::new(
            Arc::clone(&self.dag),
            Arc::clone(&self.kv),
            Arc::clone(&self.idgen),
            self.attachment_config.clone(),
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

        // Spawn actor tasks
        let actor_tasks = Self::spawn_actor_tasks(
            &self.dag,
            target,
            &stop_conditions,
            &system_tx,
            &self.actor_registry,
            &self.suspension,
        );

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
