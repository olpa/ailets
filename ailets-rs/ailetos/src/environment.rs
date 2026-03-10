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
use crate::scheduler::Scheduler;
use crate::{BlockingActorRuntime, IoRequest, KVBuffers, SystemRuntime};

/// Value node data - bytes to write to the node's output pipe
#[derive(Debug, Clone)]
pub struct ValueNodeData {
    pub data: Vec<u8>,
}

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
    pub dag: Dag,
    pub idgen: Arc<IdGen>,
    pub kv: Arc<K>,
    pub actor_registry: ActorRegistry,
    /// Value data for value nodes (keyed by node handle)
    value_nodes: HashMap<Handle, ValueNodeData>,
    /// Pending stream attachments to be registered when run() is called
    pending_attachments: Vec<(Handle, actor_runtime::StdHandle, crate::system_runtime::AttachmentConfig)>,
}

impl<K: KVBuffers> Environment<K> {
    /// Create a new environment
    pub fn new(kv: Arc<K>) -> Self {
        let idgen = Arc::new(IdGen::new());
        let dag = Dag::new(Arc::clone(&idgen));

        Self {
            dag,
            idgen,
            kv,
            actor_registry: ActorRegistry::new(),
            value_nodes: HashMap::new(),
            pending_attachments: Vec::new(),
        }
    }

    /// Attach actor's stdout to host stdout
    ///
    /// The attachment will be spawned when the actor first writes to stdout.
    pub fn attach_stdout(&mut self, node_handle: Handle) {
        self.pending_attachments.push((
            node_handle,
            actor_runtime::StdHandle::Stdout,
            crate::system_runtime::AttachmentConfig::Stdout,
        ));
    }

    /// Attach actor's stderr (Log handle) to host stderr
    ///
    /// The attachment will be spawned when the actor first writes to the Log handle.
    pub fn attach_stderr(&mut self, node_handle: Handle) {
        self.pending_attachments.push((
            node_handle,
            actor_runtime::StdHandle::Log,
            crate::system_runtime::AttachmentConfig::Stderr,
        ));
    }

    /// Attach all actors' stderr to host stderr
    ///
    /// Note: Call this AFTER adding all nodes to the DAG, as it only affects
    /// nodes that have been added at the time this method is called.
    ///
    /// Alternatively, call `attach_stderr()` for each individual node as needed.
    pub fn attach_all_stderr(&mut self) {
        // Collect node handles by iterating through the possible range
        // This is a simple implementation; alternatively we could add a method to DAG
        let mut handles = Vec::new();

        // Get ID generator's current state to know the range of possible handles
        // We'll just try to get all nodes that might exist
        for i in 0..1000 {  // Reasonable upper bound
            let handle = Handle::new(i);
            if let Some(node) = self.dag.get_node(handle) {
                // Only attach to concrete actor nodes and value nodes, not alias nodes
                // Alias nodes don't run as actors and don't produce output
                if !matches!(node.kind, crate::dag::NodeKind::Alias) {
                    handles.push(handle);
                }
            }
        }

        for handle in handles {
            self.attach_stderr(handle);
        }
    }

    /// Add a value node - a node that outputs a constant value
    ///
    /// # Arguments
    /// * `data` - The bytes to write to the node's output
    /// * `explain` - Optional explanation of what this value represents
    ///
    /// # Returns
    /// The handle to the created node
    pub fn add_value_node(&mut self, data: Vec<u8>, explain: Option<String>) -> Handle {
        let handle = self
            .dag
            .add_node_with_explain("value".into(), NodeKind::Concrete, explain);

        // Value nodes are considered "built" at creation since their output is static
        self.dag.set_state(handle, NodeState::Terminated);

        self.value_nodes.insert(handle, ValueNodeData { data });

        handle
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
        let handle = self
            .dag
            .add_node_with_explain(idname, NodeKind::Concrete, explain);

        for &dep in deps {
            self.dag.add_dependency(For(handle), DependsOn(dep));
        }

        handle
    }

    /// Add an alias node
    pub fn add_alias(&mut self, alias_name: String, target: Handle) -> Handle {
        let handle = self.dag.add_node(alias_name, NodeKind::Alias);
        self.dag.add_dependency(For(handle), DependsOn(target));
        handle
    }

    /// Resolve an alias node to its actual target node
    ///
    /// If the handle refers to an alias node, returns the target node.
    /// If the handle refers to a concrete node, returns the same handle.
    /// Recursively resolves nested aliases.
    #[must_use]
    pub fn resolve(&self, handle: Handle) -> Handle {
        if let Some(node) = self.dag.get_node(handle) {
            if matches!(node.kind, crate::dag::NodeKind::Alias) {
                // Alias node - get its dependency (should be exactly one)
                let mut deps = self.dag.get_direct_dependencies(handle);
                if let Some(target) = deps.next() {
                    // Recursively resolve in case the target is also an alias
                    return self.resolve(target);
                }
            }
        }
        // Not an alias, or no dependency found - return as-is
        handle
    }

    /// Get a node by handle
    #[must_use]
    pub fn get_node(&self, handle: Handle) -> Option<&crate::dag::Node> {
        self.dag.get_node(handle)
    }

    /// Check if a node is a value node
    #[must_use]
    pub fn is_value_node(&self, handle: Handle) -> bool {
        self.value_nodes.contains_key(&handle)
    }

    /// Get value data for a value node
    #[must_use]
    pub fn get_value_data(&self, handle: Handle) -> Option<&[u8]> {
        self.value_nodes.get(&handle).map(|v| v.data.as_slice())
    }

    /// Spawn a task for a value node
    fn spawn_value_node_task(
        node_handle: Handle,
        idname: String,
        value_data: ValueNodeData,
        runtime: BlockingActorRuntime,
    ) -> tokio::task::JoinHandle<()> {
        use actor_io::{AReader, AWriter};
        use actor_runtime::StdHandle;
        use embedded_io::Write;

        tokio::task::spawn_blocking(move || {
            debug!(node = ?node_handle, name = %idname, "value node task starting");

            runtime.request_std_handles_setup();

            let mut areader = AReader::new_from_std(&runtime, StdHandle::Stdin);
            let mut awriter = AWriter::new_from_std(&runtime, StdHandle::Stdout);

            // Write the value data
            let result = awriter
                .write_all(&value_data.data)
                .map_err(|e| format!("Failed to write value: {e:?}"))
                .and_then(|()| {
                    awriter
                        .close()
                        .map_err(|e| format!("Failed to close writer: {e:?}"))
                })
                .and_then(|()| {
                    areader
                        .close()
                        .map_err(|e| format!("Failed to close reader: {e:?}"))
                });

            match result {
                Ok(()) => debug!(node = ?node_handle, name = %idname, "value node completed"),
                Err(e) => {
                    warn!(node = ?node_handle, name = %idname, error = %e, "value node error");
                }
            }

            runtime.close_all_handles();
            debug!(node = ?node_handle, name = %idname, "value node done");
        })
    }

    /// Spawn a task for a regular actor node
    fn spawn_actor_node_task(
        node_handle: Handle,
        idname: String,
        actor_fn: ActorFn,
        runtime: BlockingActorRuntime,
    ) -> tokio::task::JoinHandle<()> {
        use actor_io::{AReader, AWriter};
        use actor_runtime::StdHandle;

        tokio::task::spawn_blocking(move || {
            debug!(node = ?node_handle, name = %idname, "task starting");

            runtime.request_std_handles_setup();

            let areader = AReader::new_from_std(&runtime, StdHandle::Stdin);
            let awriter = AWriter::new_from_std(&runtime, StdHandle::Stdout);

            let result = actor_fn(areader, awriter);

            match result {
                Ok(()) => debug!(node = ?node_handle, name = %idname, "task completed"),
                Err(e) => {
                    warn!(node = ?node_handle, name = %idname, error = %e, "task error");
                }
            }

            runtime.close_all_handles();
            debug!(node = ?node_handle, name = %idname, "task done");
        })
    }

    /// Spawn actor tasks for all nodes in the system
    fn spawn_actor_tasks(
        dag: &Arc<RwLock<Dag>>,
        target: Handle,
        system_tx: &mpsc::UnboundedSender<IoRequest>,
        actor_registry: &ActorRegistry,
        value_nodes: &HashMap<Handle, ValueNodeData>,
    ) -> Vec<tokio::task::JoinHandle<()>> {
        let dag_guard = dag.read();
        let scheduler = Scheduler::new(&dag_guard, target);
        let mut tasks = Vec::new();

        for node_handle in scheduler.iter() {
            let Some(node) = dag_guard.get_node(node_handle) else {
                warn!(node = ?node_handle, "node not found in DAG, skipping");
                continue;
            };
            let idname = node.idname.clone();
            debug!(node = ?node_handle, name = %idname, "spawning actor task");

            let runtime = BlockingActorRuntime::new(node_handle, system_tx.clone());

            // Check if this is a value node
            let task = if let Some(value_data) = value_nodes.get(&node_handle).cloned() {
                Some(Self::spawn_value_node_task(
                    node_handle,
                    idname.clone(),
                    value_data,
                    runtime,
                ))
            } else {
                actor_registry.get(&idname).map(|actor_fn| {
                    Self::spawn_actor_node_task(node_handle, idname.clone(), actor_fn, runtime)
                })
            };

            if let Some(task) = task {
                tasks.push(task);
            } else if !value_nodes.contains_key(&node_handle) {
                warn!(node = ?node_handle, name = %idname, "actor not registered, skipping");
            }
        }

        tasks
    }

    /// Run the system: spawn system runtime and actor tasks, wait for completion
    pub async fn run(self, target: Handle)
    where
        K: 'static,
    {
        // Wrap DAG in Arc<RwLock> for sharing with mutable state access
        let dag = Arc::new(RwLock::new(self.dag));

        // Create system runtime
        let mut system_runtime = SystemRuntime::new(Arc::clone(&dag), Arc::clone(&self.kv), self.idgen);

        // Register pending attachments
        for (node_handle, std_handle, config) in self.pending_attachments {
            system_runtime.register_attachment(node_handle, std_handle, config);
        }

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
            &dag,
            target,
            &system_tx,
            &self.actor_registry,
            &self.value_nodes,
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
