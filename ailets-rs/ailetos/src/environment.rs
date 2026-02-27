//! Environment - high-level orchestration for the actor system
//!
//! This module provides the Environment struct, which is the main entry point
//! for building and running actor systems. It manages:
//! - DAG construction with value nodes
//! - System runtime orchestration
//! - Actor spawning and execution

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::dag::{Dag, DependsOn, For, NodeKind};
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
    pub fn get(&self, name: &str) -> ActorFn {
        self.actors
            .get(name)
            .copied()
            .unwrap_or_else(|| panic!("Actor '{}' not registered", name))
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
    pub kv: K,
    pub actor_registry: ActorRegistry,
    /// Value data for value nodes (keyed by node handle)
    value_nodes: HashMap<Handle, ValueNodeData>,
}

impl<K: KVBuffers> Environment<K> {
    /// Create a new environment
    pub fn new(kv: K) -> Self {
        let idgen = Arc::new(IdGen::new());
        let dag = Dag::new(Arc::clone(&idgen));

        Self {
            dag,
            idgen,
            kv,
            actor_registry: ActorRegistry::new(),
            value_nodes: HashMap::new(),
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
    pub fn add_node(
        &mut self,
        idname: String,
        deps: &[Handle],
        explain: Option<String>,
    ) -> Handle {
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

    /// Get a node by handle
    pub fn get_node(&self, handle: Handle) -> Option<&crate::dag::Node> {
        self.dag.get_node(handle)
    }

    /// Check if a node is a value node
    pub fn is_value_node(&self, handle: Handle) -> bool {
        self.value_nodes.contains_key(&handle)
    }

    /// Get value data for a value node
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
                .map_err(|e| format!("Failed to write value: {:?}", e))
                .and_then(|_| {
                    awriter
                        .close()
                        .map_err(|e| format!("Failed to close writer: {:?}", e))
                })
                .and_then(|_| {
                    areader
                        .close()
                        .map_err(|e| format!("Failed to close reader: {:?}", e))
                });

            match result {
                Ok(()) => debug!(node = ?node_handle, name = %idname, "value node completed"),
                Err(e) => warn!(node = ?node_handle, name = %idname, error = %e, "value node error"),
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
                    warn!(node = ?node_handle, name = %idname, error = %e, "task error")
                }
            }

            runtime.close_all_handles();
            debug!(node = ?node_handle, name = %idname, "task done");
        })
    }

    /// Spawn actor tasks for all nodes in the system
    fn spawn_actor_tasks(
        dag: &Arc<Dag>,
        target: Handle,
        system_tx: mpsc::UnboundedSender<IoRequest>,
        actor_registry: &ActorRegistry,
        value_nodes: &HashMap<Handle, ValueNodeData>,
    ) -> Vec<tokio::task::JoinHandle<()>> {
        let scheduler = Scheduler::new(dag, target);
        let mut tasks = Vec::new();

        for node_handle in scheduler.iter() {
            let node = dag.get_node(node_handle).expect("node exists");
            let idname = node.idname.clone();
            debug!(node = ?node_handle, name = %idname, "spawning actor task");

            let runtime = BlockingActorRuntime::new(node_handle, system_tx.clone());

            // Check if this is a value node
            let task = if let Some(value_data) = value_nodes.get(&node_handle).cloned() {
                Self::spawn_value_node_task(node_handle, idname, value_data, runtime)
            } else {
                let actor_fn = actor_registry.get(&idname);
                Self::spawn_actor_node_task(node_handle, idname, actor_fn, runtime)
            };

            tasks.push(task);
        }

        tasks
    }

    /// Run the system: spawn system runtime and actor tasks, wait for completion
    pub async fn run(self, target: Handle)
    where
        K: 'static,
    {
        // Wrap DAG in Arc for sharing
        let dag = Arc::new(self.dag);

        // Create system runtime
        let system_runtime = SystemRuntime::new(Arc::clone(&dag), self.kv, self.idgen);

        // Get sender before moving system_runtime
        let system_tx = system_runtime.get_system_tx();

        // Spawn SystemRuntime task
        let system_task = tokio::spawn(async move {
            system_runtime.run().await;
        });

        // Spawn actor tasks
        let actor_tasks =
            Self::spawn_actor_tasks(&dag, target, system_tx, &self.actor_registry, &self.value_nodes);

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
