//! Executor - actor execution for the actor system
//!
//! This module handles:
//! - Topological ordering of DAG nodes
//! - Readiness checking for node spawning
//! - The spawn loop that runs actors
//! - Top-level execution of a DAG run

use std::collections::HashSet;
use std::sync::Arc;

use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error, warn};

use crate::dag::{Dag, NodeKind, NodeState};
use crate::environment::{ActorFn, RunHandle};
use crate::idgen::Handle;
use crate::pipe::PipePool;
use crate::system_runtime::IoRequest;
use crate::{BlockingActorRuntime, KVBuffers, ShutdownHandle, SystemRuntime};

/// Conditions for stopping DAG iteration
#[derive(Debug, Clone, Default)]
pub struct StopConditions {
    /// Execute only the first ready node, then stop
    pub one_step: bool,
    /// Stop before executing this node
    pub stop_before: Option<Handle>,
    /// Stop after executing this node
    pub stop_after: Option<Handle>,
}

/// Decide whether a node is ready to be spawned.
///
/// Decision table (iterate concrete deps, first decisive result wins):
///
/// | Dep state                      | Has output | Decision        |
/// |--------------------------------|------------|-----------------|
/// | `NotStarted`                   | —          | don't start     |
/// | Running / Terminating          | yes        | start           |
/// | Running / Terminating          | no         | don't start     |
/// | Terminated, exit_code != 0     | —          | don't start     |
/// | Terminated, exit_code == 0     | yes        | start           |
/// | Terminated, exit_code == 0     | no         | skip (neutral)  |
/// | (all deps exhausted)           | —          | start           |
///
/// "Has output" is checked optimistically: a dep has output if its stdout
/// pipe is realized (writer exists in the pool), regardless of byte count.
///
/// Note: Suspension state does not affect spawn readiness. If a dependency
/// has produced output, downstream actors can start consuming it regardless
/// of whether the dependency is suspended.
pub fn is_ready_to_spawn<K: KVBuffers>(
    node_handle: Handle,
    dag: &Dag,
    pipe_pool: &PipePool<K>,
) -> bool {
    use actor_runtime::StdHandle;

    for dep in dag.resolve_dependencies(node_handle) {
        let Some(dep_node) = dag.get_node(dep) else {
            continue;
        };

        match dep_node.state {
            NodeState::NotStarted => return false,
            NodeState::Running | NodeState::Terminating => {
                return pipe_pool
                    .get_already_realized_writer((dep, StdHandle::Stdout))
                    .is_some();
            }
            NodeState::Terminated => {
                if dep_node.exit_code != 0 {
                    return false;
                }
                if pipe_pool
                    .get_already_realized_writer((dep, StdHandle::Stdout))
                    .is_some()
                {
                    return true;
                }
                // neutral: clean termination with no output, continue to next dep
            }
        }
    }

    true
}

/// True if any node is Running or Terminating (i.e. an actor task is still alive).
/// Used by the spawn loop to decide whether `spawn_notify` can ever fire again.
fn has_active_actors(dag: &Dag) -> bool {
    dag.nodes().any(|n| match n.state {
        NodeState::Running | NodeState::Terminating => true,
        NodeState::NotStarted | NodeState::Terminated => false,
    })
}

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

        let result = actor_fn(&actor_runtime);

        match result {
            Ok(()) => {
                debug!(node = ?node_handle, name = %idname, "task completed");
            }
            Err(e) => {
                warn!(node = ?node_handle, name = %idname, error = %e, "task error");
                shutdown.mark_failed();
            }
        }

        debug!(node = ?node_handle, name = %idname, "task done, shutdown via Drop");
        drop(shutdown);
    })
}

/// Run the system: spawn system runtime and actor tasks, wait for completion
pub async fn run<K: KVBuffers + 'static>(
    run_handle: &RunHandle<K>,
    target: Handle,
    stop_conditions: StopConditions,
) {
    run_with_tx(run_handle, target, stop_conditions, None).await;
}

/// Like `run`, but sends `system_tx` back via `tx_out` once the system runtime is ready.
pub async fn run_with_tx<K: KVBuffers + 'static>(
    run_handle: &RunHandle<K>,
    target: Handle,
    stop_conditions: StopConditions,
    tx_out: Option<oneshot::Sender<mpsc::UnboundedSender<IoRequest>>>,
) {
    let pipe_pool = Arc::new(PipePool::new(Arc::clone(&run_handle.kv)));

    let system_runtime = SystemRuntime::new(
        Arc::clone(&run_handle.dag),
        Arc::clone(&run_handle.kv),
        Arc::clone(&run_handle.idgen),
        run_handle.attachment_config.clone(),
        Arc::clone(&pipe_pool),
    );

    let spawn_notify = system_runtime.get_spawn_notify();

    let Some(system_tx) = system_runtime.get_system_tx() else {
        error!("Failed to get system_tx - system runtime already started");
        return;
    };

    if let Some(sender) = tx_out {
        sender.send(system_tx.clone()).ok();
    }

    let system_task = tokio::spawn(async move {
        system_runtime.run().await;
    });

    // Build initial pending list: NotStarted nodes in topological order
    let mut pending: Vec<Handle> = {
        let dag_guard = run_handle.dag.read();
        TopologicalOrderIter::with_stop_conditions(&dag_guard, target, stop_conditions)
            .filter(|&n| {
                dag_guard
                    .get_node(n)
                    .is_some_and(|node| node.state == NodeState::NotStarted)
            })
            .collect()
    };

    let mut actor_tasks = Vec::new();

    loop {
        let to_spawn: Vec<(Handle, String)> = {
            let dag_guard = run_handle.dag.read();
            pending
                .iter()
                .filter(|&&n| is_ready_to_spawn(n, &dag_guard, &pipe_pool))
                .filter_map(|&n| dag_guard.get_node(n).map(|node| (n, node.idname.clone())))
                .collect()
        };

        for (node_handle, idname) in &to_spawn {
            pending.retain(|&h| h != *node_handle);

            let Some(actor_fn) = run_handle.actor_registry.get(idname) else {
                warn!(node = ?node_handle, name = %idname, "actor not registered, skipping");
                // Terminate the node explicitly so dependents are not blocked.
                let _ = system_tx.send(IoRequest::ActorShutdown {
                    node_handle: *node_handle,
                    exit_code: 0,
                });
                continue;
            };

            {
                let mut dag = run_handle.dag.write();
                // Re-check state under the write lock: an actor task running
                // concurrently may have already advanced this node past NotStarted.
                if dag
                    .get_node(*node_handle)
                    .is_none_or(|n| n.state != NodeState::NotStarted)
                {
                    continue;
                }
                dag.set_state(*node_handle, NodeState::Running);
            }
            debug!(node = ?node_handle, name = %idname, "spawning actor task");

            let (actor_runtime, shutdown) = BlockingActorRuntime::new(
                *node_handle,
                system_tx.clone(),
                Arc::clone(&run_handle.suspension),
            );

            actor_tasks.push(spawn_actor_task(
                *node_handle,
                idname.clone(),
                actor_fn,
                actor_runtime,
                shutdown,
            ));
        }

        if pending.is_empty() {
            break;
        }

        // Quiescence check: nothing changed this iteration (no node was ready
        // to spawn) AND no actor is Running or Terminating.
        //
        // When both hold, spawn_notify can never fire again — nothing changed,
        // and nothing will change. Remaining pending nodes are blocked by a
        // failed dependency and will never run in this execution. Break instead
        // of waiting forever; those nodes stay NotStarted and are eligible for
        // an incremental re-run once the failure is resolved.
        if to_spawn.is_empty() && !has_active_actors(&run_handle.dag.read()) {
            debug!("quiescent: pending nodes remain but no actors are active — stopping");
            break;
        }

        // Something is running or was just spawned. Wait for a state change:
        // either an actor terminates (possibly unblocking a dependent) or a
        // pipe is realized (making a dep's output available).
        spawn_notify.notified().await;
    }

    drop(system_tx);

    if let Err(e) = system_task.await {
        warn!(error = %e, "SystemRuntime task failed");
    }

    for task in actor_tasks {
        if let Err(e) = task.await {
            warn!(error = %e, "actor task failed");
        }
    }
}

/// Iterator that yields DAG nodes in topological order (dependencies before dependents).
///
/// On first `next()`, computes the full order into `result`. Then yields nodes
/// one by one via `result_index`. The `stopped` flag allows early termination.
pub struct TopologicalOrderIter<'a> {
    dag: &'a Dag,
    // (node, deps_pushed): when false, push deps then re-push with true;
    // when true, emit the node (post-order ensures deps come first).
    stack: Vec<(Handle, bool)>,
    visited: HashSet<Handle>,
    result: Vec<Handle>,
    result_index: usize,
    stopped: bool,
    stop_conditions: StopConditions,
}

impl<'a> TopologicalOrderIter<'a> {
    #[must_use]
    pub fn new(dag: &'a Dag, target: Handle) -> Self {
        Self::with_stop_conditions(dag, target, StopConditions::default())
    }

    #[must_use]
    pub fn with_stop_conditions(
        dag: &'a Dag,
        target: Handle,
        stop_conditions: StopConditions,
    ) -> Self {
        Self {
            dag,
            stack: vec![(target, false)],
            visited: HashSet::new(),
            result: Vec::new(),
            result_index: 0,
            stopped: false,
            stop_conditions,
        }
    }

    /// Build the full topological order using post-order DFS.
    /// Only concrete nodes are included; aliases are traversed but not yielded.
    fn build_order(&mut self) {
        while let Some((node, deps_pushed)) = self.stack.pop() {
            if deps_pushed {
                if let Some(node_info) = self.dag.get_node(node) {
                    if node_info.kind == NodeKind::Concrete {
                        self.result.push(node);
                    }
                }
                continue;
            }

            if !self.visited.insert(node) {
                continue;
            }

            // Re-push to emit after all deps are processed
            self.stack.push((node, true));

            for dep in self.dag.resolve_dependencies(node) {
                if !self.visited.contains(&dep) {
                    self.stack.push((dep, false));
                }
            }
        }
    }
}

impl Iterator for TopologicalOrderIter<'_> {
    type Item = Handle;

    fn next(&mut self) -> Option<Self::Item> {
        if !self.stack.is_empty() {
            self.build_order();
        }

        if self.stopped {
            return None;
        }

        let node = *self.result.get(self.result_index)?;
        self.result_index += 1;

        if self.stop_conditions.stop_before == Some(node) {
            self.stopped = true;
            return None;
        }

        if self.stop_conditions.one_step || self.stop_conditions.stop_after == Some(node) {
            self.stopped = true;
        }

        Some(node)
    }
}
