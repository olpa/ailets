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
use tracing::{debug, warn};

use crate::actor_syscall::ActorLifecycleEvent;
use crate::attachments::AttachmentManager;
use crate::dag::{Dag, NodeKind, NodeState};
use crate::environment::{ActorFn, Environment};
use crate::errno::EOWNERDEAD;
use crate::idgen::Handle;
use crate::pipe::PipePool;
use crate::{BlockingActorRuntime, IoBridge};

/// Events emitted by the executor to report progress to the caller.
#[derive(Debug, Clone)]
pub enum ExecutorEvent {
    /// A node has finished executing (successfully or not).
    NodeTerminated(Handle),
}

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
/// | Terminated, `exit_code` != 0   | —          | don't start     |
/// | Terminated, `exit_code` == 0   | yes        | start           |
/// | Terminated, `exit_code` == 0   | no         | skip (neutral)  |
/// | (all deps exhausted)           | —          | start           |
///
/// "Has output" is checked optimistically: a dep has output if its stdout
/// pipe is realized (writer exists in the pool), regardless of byte count.
///
/// Note: Suspension state does not affect spawn readiness. If a dependency
/// has produced output, downstream actors can start consuming it regardless
/// of whether the dependency is suspended.
pub fn is_ready_to_spawn(node_handle: Handle, dag: &Dag, pipe_pool: &PipePool) -> bool {
    use actor_runtime::StdHandle;

    for dep in dag.resolve_dependencies(node_handle) {
        let Some(dep_node) = dag.get_node(dep) else {
            continue;
        };

        match dep_node.state {
            NodeState::NotStarted => return false,
            NodeState::Running | NodeState::Terminating => {
                return pipe_pool
                    .get_already_realized_writer((dep, StdHandle::Stdout as isize))
                    .is_some();
            }
            NodeState::Terminated => {
                if dep_node.exit_code != 0 {
                    return false;
                }
                if pipe_pool
                    .get_already_realized_writer((dep, StdHandle::Stdout as isize))
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
/// When false and no nodes were spawned this iteration, no further I/O events
/// can occur, so remaining pending nodes will never become ready.
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
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let blocking_result = tokio::task::spawn_blocking(move || {
            debug!(node = ?node_handle, name = %idname, "task starting");

            let mut runtime = actor_runtime;
            runtime.register_std_fds();

            let result = actor_fn(&runtime);

            match result {
                Ok(()) => {
                    debug!(node = ?node_handle, name = %idname, "task completed");
                }
                Err(e) => {
                    warn!(node = ?node_handle, name = %idname, error = %e, "task error");
                    runtime.latch_errno(EOWNERDEAD);
                }
            }
            runtime
        })
        .await;

        match blocking_result {
            Ok(mut runtime) => {
                if let Err(e) = runtime.shutdown().await {
                    warn!(node = ?node_handle, error = %e, "actor shutdown error");
                }
            }
            Err(e) => {
                warn!(node = ?node_handle, error = %e, "actor blocking task panicked");
            }
        }
    })
}

/// Run the system: spawn system runtime and actor tasks, wait for completion
pub async fn run(env: Arc<Environment>, target: Handle, stop_conditions: StopConditions) {
    let infra = Executor::new(&env, None);

    let pending: HashSet<Handle> = {
        let dag_guard = env.dag.read();
        TopologicalOrderIter::with_stop_conditions(&dag_guard, target, stop_conditions)
            .filter(|&n| {
                dag_guard
                    .get_node(n)
                    .is_some_and(|node| node.state == NodeState::NotStarted)
            })
            .collect()
    };

    let actor_tasks = run_spawn_loop(&env, &infra, pending).await;
    infra.shutdown(actor_tasks).await;
}

/// Run the system consuming jobs from a `JobQueue`.
///
/// Exits when the channel closes (all `JobSender` clones dropped) and all
/// in-progress work finishes. Supports both finite and infinite execution
/// depending on the lifetime of the senders.
pub async fn run_jobs(
    env: Arc<Environment>,
    mut jobs: JobQueue,
    stop_conditions: StopConditions,
    events_tx: mpsc::UnboundedSender<ExecutorEvent>,
) {
    let infra = Executor::new(&env, Some(events_tx));
    let actor_tasks =
        run_spawn_loop_jobs(&env, &infra, &mut jobs.rx, &stop_conditions).await;
    infra.shutdown(actor_tasks).await;
}

/// Receives actor lifecycle events from the IO bridge, updates DAG state,
/// replies to unblock the IO bridge, and fires notify so the spawn loop can react.
/// Emits `ExecutorEvent::NodeTerminated` to `events_tx` on each termination.
fn spawn_lifecycle_event_task(
    dag: Arc<parking_lot::RwLock<Dag>>,
    notify: Arc<tokio::sync::Notify>,
    mut rx: mpsc::UnboundedReceiver<ActorLifecycleEvent>,
    events_tx: Option<mpsc::UnboundedSender<ExecutorEvent>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            match event {
                ActorLifecycleEvent::Terminating { node_handle, reply } => {
                    let prior = {
                        let mut dag = dag.write();
                        let prior = dag
                            .get_node(node_handle)
                            .map_or(NodeState::Terminating, |n| n.state);
                        match prior {
                            NodeState::Terminating | NodeState::Terminated => {}
                            NodeState::Running => {
                                dag.set_state(node_handle, NodeState::Terminating);
                            }
                            NodeState::NotStarted => {
                                warn!(node = ?node_handle, "actor shutdown received but node was never started");
                                dag.set_state(node_handle, NodeState::Terminating);
                            }
                        }
                        prior
                    };
                    if reply.send(prior).is_err() {
                        warn!(node = ?node_handle, "actor_done: Terminating reply receiver dropped");
                    }
                    notify.notify_one();
                }
                ActorLifecycleEvent::Terminated {
                    node_handle,
                    exit_code,
                    reply,
                } => {
                    let prior = {
                        let mut dag = dag.write();
                        let prior = dag
                            .get_node(node_handle)
                            .map_or(NodeState::Terminated, |n| n.state);
                        if prior != NodeState::Terminating {
                            warn!(node = ?node_handle, ?prior, "actor Terminated but prior state was not Terminating");
                        }
                        dag.set_state(node_handle, NodeState::Terminated);
                        dag.set_exit_code(node_handle, exit_code);
                        prior
                    };
                    if reply.send(prior).is_err() {
                        warn!(node = ?node_handle, "actor_done: Terminated reply receiver dropped");
                    }
                    if let Some(ref tx) = events_tx {
                        if tx.send(ExecutorEvent::NodeTerminated(node_handle)).is_err() {
                            warn!(node = ?node_handle, "executor events receiver dropped");
                        }
                    }
                    notify.notify_one();
                }
            }
        }
    })
}

/// Spawn one batch of ready nodes from `pending`.
///
/// Returns `(remaining_pending, new_actor_tasks, had_ready)` where `had_ready`
/// is true when at least one node was found ready this pass — used by
/// `run_spawn_loop` to detect quiescence.
fn spawn_ready_actors(
    pending: HashSet<Handle>,
    env: &Arc<Environment>,
    infra: &Executor,
) -> (HashSet<Handle>, Vec<tokio::task::JoinHandle<()>>, bool) {
    let to_spawn: Vec<(Handle, String)> = {
        let dag_guard = env.dag.read();
        pending
            .iter()
            .filter(|&&n| is_ready_to_spawn(n, &dag_guard, &env.pipe_pool))
            .filter_map(|&n| dag_guard.get_node(n).map(|node| (n, node.idname.clone())))
            .collect()
    };

    let had_ready = !to_spawn.is_empty();
    let mut remaining = pending;
    let mut actor_tasks = Vec::new();

    for (node_handle, idname) in &to_spawn {
        remaining.remove(node_handle);

        let Some(actor_fn) = env.actor_registry.read().get(idname) else {
            warn!(node = ?node_handle, name = %idname, "actor not registered, skipping");
            // Mark as terminated so dependents are not blocked.
            // No lifecycle events needed since the actor was never started.
            {
                let mut dag = env.dag.write();
                dag.set_exit_code(*node_handle, crate::errno::ENOENT);
                dag.set_state(*node_handle, NodeState::Terminated);
            }
            infra.spawn_wakeup.notify_one();
            continue;
        };

        {
            let mut dag = env.dag.write();
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

        let actor_runtime = BlockingActorRuntime::new(
            *node_handle,
            Arc::clone(&infra.io_bridge),
            Arc::clone(&env.suspension),
            infra.lifecycle_tx.clone(),
        );
        actor_tasks.push(spawn_actor_task(*node_handle, idname.clone(), actor_fn, actor_runtime));
    }

    (remaining, actor_tasks, had_ready)
}

/// Spawn loop for `run_with_tx`: runs until all nodes in `pending` are done or quiescent.
async fn run_spawn_loop(
    env: &Arc<Environment>,
    infra: &Executor,
    mut pending: HashSet<Handle>,
) -> Vec<tokio::task::JoinHandle<()>> {
    let mut actor_tasks = Vec::new();

    loop {
        let (new_pending, new_tasks, had_ready) = spawn_ready_actors(pending, env, infra);
        pending = new_pending;
        actor_tasks.extend(new_tasks);

        if pending.is_empty() {
            break;
        }

        // Quiescence: nothing was ready to spawn AND no actor is running.
        // Remaining nodes are blocked by a failed dependency; they stay
        // NotStarted and are eligible for an incremental re-run.
        if !had_ready && !has_active_actors(&env.dag.read()) {
            debug!("quiescent: pending nodes remain but no actors are active — stopping");
            break;
        }

        infra.spawn_wakeup.notified().await;
    }

    actor_tasks
}

/// Spawn loop for `run_jobs`: waits on both the job channel and state-change
/// notifications simultaneously, exiting only when the channel is closed and
/// no actors are running.
async fn run_spawn_loop_jobs(
    env: &Arc<Environment>,
    infra: &Executor,
    job_rx: &mut mpsc::UnboundedReceiver<Handle>,
    stop_conditions: &StopConditions,
) -> Vec<tokio::task::JoinHandle<()>> {
    let mut actor_tasks = Vec::new();
    let mut pending: HashSet<Handle> = HashSet::new();
    let mut channel_closed = false;

    loop {
        let (new_pending, new_tasks, _) = spawn_ready_actors(pending, env, infra);
        pending = new_pending;
        actor_tasks.extend(new_tasks);

        if channel_closed && !has_active_actors(&env.dag.read()) {
            break;
        }

        tokio::select! {
            result = job_rx.recv(), if !channel_closed => {
                match result {
                    Some(target) => {
                        let dag = env.dag.read();
                        pending.extend(
                            TopologicalOrderIter::with_stop_conditions(
                                &dag, target, stop_conditions.clone(),
                            )
                            .filter(|&n| {
                                dag.get_node(n)
                                    .is_some_and(|node| node.state == NodeState::NotStarted)
                            }),
                        );
                    }
                    None => { channel_closed = true; }
                }
            }
            _ = infra.spawn_wakeup.notified() => {}
        }
    }

    actor_tasks
}

/// Shared infrastructure for an executor run.
///
/// # Fields
///
/// - `spawn_wakeup`: Signals the spawn loop when DAG state changes occur. The
///   lifecycle handler calls `notify_one()` after updating node states, causing
///   the spawn loop to wake up and check for newly ready nodes.
///
/// - `io_bridge`: Handles all I/O operations for actors (stdin/stdout/stderr/files).
///   Cloned into each `BlockingActorRuntime` so actors can perform I/O. Also
///   passed to `attachment_manager` for coordinating attachment I/O.
///
/// - `lifecycle_tx`: Channel sender for actor lifecycle events (Terminating/Terminated).
///   Cloned into each `BlockingActorRuntime`; when an actor shuts down, it sends
///   events via this channel to notify the executor of state transitions.
///
/// - `lifecycle_handler`: Background task that receives lifecycle events from
///   `lifecycle_tx`, updates the DAG state accordingly, and triggers `spawn_wakeup`.
///   Also emits `ExecutorEvent::NodeTerminated` to external listeners if configured.
///
/// - `attachment_manager`: Manages attachment I/O tasks (e.g., network connections,
///   file attachments). Provides attachment handles to actors via the io_bridge.
///
/// # Teardown order is critical:
///
/// 1. Join actor tasks — `BlockingActorRuntime::drop` sends lifecycle events
///    and blocks on `lifecycle_handler` replies, so `lifecycle_handler` must still
///    be running at this point.
/// 2. `io_bridge.shutdown()` — flush I/O channels.
/// 3. `attachment_manager.shutdown()` — wait for attachment tasks.
/// 4. Drop `lifecycle_tx` — signals `lifecycle_handler` to exit.
/// 5. Drop `io_bridge` — releases the last Arc so `lifecycle_handler` can finish.
/// 6. Join `lifecycle_handler`.
struct Executor {
    spawn_wakeup: Arc<tokio::sync::Notify>,
    io_bridge: Arc<IoBridge>,
    lifecycle_tx: mpsc::UnboundedSender<ActorLifecycleEvent>,
    lifecycle_handler: tokio::task::JoinHandle<()>,
    attachment_manager: Arc<AttachmentManager>,
}

impl Executor {
    fn new(
        env: &Arc<Environment>,
        events_tx: Option<mpsc::UnboundedSender<ExecutorEvent>>,
    ) -> Self {
        let spawn_wakeup = Arc::new(tokio::sync::Notify::new());
        let (lifecycle_tx, lifecycle_rx) = mpsc::unbounded_channel::<ActorLifecycleEvent>();
        let attachment_manager =
            Arc::new(AttachmentManager::new(env.attachment_config.read().clone()));
        let io_bridge = Arc::new(IoBridge::new(
            Arc::clone(env),
            Arc::clone(&attachment_manager),
            Arc::clone(&spawn_wakeup),
        ));
        let lifecycle_handler = spawn_lifecycle_event_task(
            Arc::clone(&env.dag),
            Arc::clone(&spawn_wakeup),
            lifecycle_rx,
            events_tx,
        );
        Self {
            spawn_wakeup,
            io_bridge,
            lifecycle_tx,
            lifecycle_handler,
            attachment_manager,
        }
    }

    async fn shutdown(self, actor_tasks: Vec<tokio::task::JoinHandle<()>>) {
        let Self {
            spawn_wakeup: _,
            io_bridge,
            lifecycle_tx,
            lifecycle_handler,
            attachment_manager,
        } = self;

        for task in actor_tasks {
            if let Err(e) = task.await {
                warn!(error = %e, "actor task failed");
            }
        }
        if let Err(e) = io_bridge.shutdown().await {
            warn!(error = %e, "io_bridge shutdown error");
        }
        attachment_manager.shutdown().await;
        drop(lifecycle_tx);
        drop(io_bridge);
        if let Err(e) = lifecycle_handler.await {
            warn!(error = %e, "lifecycle handler task failed");
        }
    }
}

/// Like `run`, but sends the `IoBridge` back via `tx_out` once ready.
///
/// # WARNING: This function exists to support a broken implementation
///
/// The CLI's `kill` command requires direct access to `IoBridge` to call
/// `cleanup_actor_io()`. However, this is fundamentally broken:
/// - It only cleans up I/O channels, it doesn't actually stop the actor task
/// - It doesn't update DAG state or send lifecycle events
/// - It breaks encapsulation by exposing internal infrastructure
///
/// This function should be removed once the kill command is properly implemented.
/// See: `doc/in_progress/fix-kill-command.md` for the planned fix.
pub async fn run_with_tx(
    env: Arc<Environment>,
    target: Handle,
    stop_conditions: StopConditions,
    tx_out: Option<oneshot::Sender<Arc<IoBridge>>>,
) {
    let infra = Executor::new(&env, None);

    if let Some(sender) = tx_out {
        sender.send(Arc::clone(&infra.io_bridge)).ok();
    }

    let pending: HashSet<Handle> = {
        let dag_guard = env.dag.read();
        TopologicalOrderIter::with_stop_conditions(&dag_guard, target, stop_conditions)
            .filter(|&n| {
                dag_guard
                    .get_node(n)
                    .is_some_and(|node| node.state == NodeState::NotStarted)
            })
            .collect()
    };

    let actor_tasks = run_spawn_loop(&env, &infra, pending).await;
    infra.shutdown(actor_tasks).await;
}

/// Handle for submitting new jobs to a running executor.
///
/// Cloneable and `Send + Sync` — distribute copies to any system component
/// that needs to submit work. The channel closes when all clones are dropped,
/// which signals the executor to exit after finishing current work.
#[derive(Clone)]
pub struct JobSender {
    tx: mpsc::UnboundedSender<Handle>,
}

impl JobSender {
    pub fn submit(&self, target: Handle) -> Result<(), mpsc::error::SendError<Handle>> {
        self.tx.send(target)
    }
}

/// Receiving end of the job channel, consumed by `run_jobs`.
pub struct JobQueue {
    pub rx: mpsc::UnboundedReceiver<Handle>,
}

/// Create a linked (`JobSender`, `JobQueue`) pair.
pub fn job_queue() -> (JobSender, JobQueue) {
    let (tx, rx) = mpsc::unbounded_channel();
    (JobSender { tx }, JobQueue { rx })
}

/// Iterator that yields DAG nodes in topological order (dependencies before dependents).
///
/// On first `next()`, computes the full order into `result`. Then yields nodes
/// one by one via `result_index`. The `stopped` flag allows early termination.
///
/// The topological ordering is no longer relied upon by the spawn loop (which uses
/// `is_ready_to_spawn` to enforce dependency order dynamically). This iterator is
/// still used for two things: DAG traversal (discovering all nodes reachable from a
/// target) and honouring `StopConditions` (`one_step`, `stop_before`, `stop_after`).
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
