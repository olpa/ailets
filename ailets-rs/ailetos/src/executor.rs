//! Executor - DAG-based actor scheduling and execution
//!
//! # Architecture
//!
//! The executor orchestrates concurrent actor execution based on a DAG of dependencies.
//! It implements **on-demand spawning** (actors start only when their inputs are available)
//! and **maximum concurrency** (runs as many actors in parallel as dependencies allow).
//!
//! ## Core Components
//!
//! ### Executor
//!
//! The main entry point for running actors. Created via [`Executor::start()`], it runs
//! indefinitely in the background, processing jobs submitted via [`Executor::submit()`].
//! Shutdown via [`Executor::shutdown()`] closes the job channel, waits for all work to
//! complete, and cleans up resources.
//!
//! ### `ExecutorInfra`
//!
//! Internal infrastructure shared across all spawned actors (I/O bridge, lifecycle
//! handlers, wakeup notifications). See the struct documentation for details.
//!
//! ### Actor Tasks
//!
//! Each actor runs in a two-layer task structure via [`spawn_actor_task()`]:
//!
//! 1. An async tokio task (`tokio::spawn`) for lifecycle management
//! 2. A blocking task (`tokio::task::spawn_blocking`) that runs the actor function
//!
//! Actors use synchronous I/O, so they must run in blocking tasks. The outer async
//! task provides a `BlockingActorRuntime` for actor syscalls, handles errors by
//! latching errno, and calls shutdown after completion.
//!
//! Actor tasks are tracked in a `JoinSet` for incremental cleanup as they complete.
//!
//! ## Execution Flow
//!
//! ### Job Submission
//!
//! Jobs (target nodes) are submitted via [`Executor::submit()`], which sends them through
//! an mpsc channel to the executor's main loop. Each job specifies:
//!
//! - **target**: The DAG node to execute
//! - **`stop_conditions`**: Controls which dependencies to include (`one_step`, `stop_before`, etc.)
//!
//! The executor expands each target into a set of `NotStarted` nodes using topological
//! ordering, then attempts to spawn them based on readiness.
//!
//! ### Main Loop (`run_spawn_loop_jobs`)
//!
//! The executor's core loop simultaneously:
//!
//! 1. **Receives new jobs** from the job channel (via `select!` on `job_rx.recv()`)
//! 2. **Reacts to state changes** via `executor_wakeup.notified()` when:
//!    - An actor terminates (changes spawn readiness for dependents)
//!    - A pipe is realized (first write unblocks downstream readers)
//!    - A writer closes (may trigger cleanup or error propagation)
//! 3. **Joins completed actor tasks** incrementally to maintain bounded memory
//!
//! On each iteration, the loop calls [`spawn_ready_actors()`] to check which pending
//! nodes are now ready to spawn based on dependency state and output availability
//! (see [`is_ready_to_spawn()`] for the complete readiness algorithm).
//!
//! The lifecycle handler ([`lifecycle_event_task()`]) receives events from actors,
//! updates the DAG state, and wakes the executor. Only the `Terminated` transition
//! triggers a wakeup (Terminating doesn't change spawn readiness).
//!
//! ## Communication Channels
//!
//! - Job submission (User → Executor):
//!   Jobs sent via mpsc channel; closes when all handles dropped
//! - Lifecycle events (Actors → Executor):
//!   Two-phase protocol with reply channels for synchronization
//! - Executor events (Executor → External observers):
//!   `NodeTerminated(Handle)` fired when actors finish
//! - I/O requests (Actors → `IoBridge` → Pipe/Attachment tasks):
//!   Per-fd commands with oneshot replies

use std::collections::HashSet;
use std::sync::Arc;

use parking_lot::Mutex;

use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinSet;
use tracing::{debug, warn};

use crate::actor_syscall::ActorLifecycleEvent;
use crate::dag::{Dag, NodeState};
use crate::environment::{ActorFn, Environment};
use crate::errno::EOWNERDEAD;
use crate::idgen::Handle;
use crate::pipe::PipePool;
use crate::traversal::{StopConditions, TopologicalOrderIter};
use crate::{BlockingActorRuntime, IoBridge};

/// Events emitted by the executor to report progress to the caller.
#[derive(Debug, Clone)]
pub enum ExecutorEvent {
    /// A node has finished executing (successfully or not).
    NodeTerminated(Handle),
}

pub enum SpawnReadiness {
    Ready,
    Waiting,
    FailedDependency(Handle),
}

/// Decide whether a node is ready to be spawned.
///
/// Decision table (iterate concrete deps, first decisive result wins):
///
/// | Dep state                      | Has output | Decision              |
/// |--------------------------------|------------|-----------------------|
/// | `NotStarted`                   | —          | `Waiting`             |
/// | Running / Terminating          | yes        | `Ready`               |
/// | Running / Terminating          | no         | `Waiting`             |
/// | Terminated, `exit_code` != 0   | —          | `FailedDependency`    |
/// | Terminated, `exit_code` == 0   | yes        | `Ready`               |
/// | Terminated, `exit_code` == 0   | no         | skip (neutral)        |
/// | (all deps exhausted)           | —          | `Ready`               |
///
/// "Has output" is checked optimistically: a dep has output if its stdout
/// pipe is realized (writer exists in the pool), regardless of byte count.
///
/// Note: Suspension state does not affect spawn readiness. If a dependency
/// has produced output, downstream actors can start consuming it regardless
/// of whether the dependency is suspended.
pub fn is_ready_to_spawn(node_handle: Handle, dag: &Dag, pipe_pool: &PipePool) -> SpawnReadiness {
    use actor_runtime::StdHandle;

    for dep in dag.resolve_dependencies(node_handle) {
        let Some(dep_node) = dag.get_node(dep) else {
            continue;
        };

        match dep_node.state {
            NodeState::NotStarted => return SpawnReadiness::Waiting,
            NodeState::Running | NodeState::Terminating => {
                return if pipe_pool
                    .get_already_realized_writer((dep, StdHandle::Stdout as isize))
                    .is_some()
                {
                    SpawnReadiness::Ready
                } else {
                    SpawnReadiness::Waiting
                };
            }
            NodeState::Terminated => {
                if dep_node.exit_code != 0 {
                    return SpawnReadiness::FailedDependency(dep);
                }
                if pipe_pool
                    .get_already_realized_writer((dep, StdHandle::Stdout as isize))
                    .is_some()
                {
                    return SpawnReadiness::Ready;
                }
                // neutral: clean termination with no output, continue to next dep
            }
        }
    }

    SpawnReadiness::Ready
}

/// Spawn a task for an actor node
fn spawn_actor_task(
    async_runtime: &tokio::runtime::Handle,
    node_handle: Handle,
    idname: String,
    actor_fn: ActorFn,
    actor_runtime: BlockingActorRuntime,
) -> tokio::task::JoinHandle<()> {
    let async_runtime = async_runtime.clone();
    async_runtime.spawn({
        let async_runtime = async_runtime.clone();
        async move {
            let blocking_result = async_runtime
                .spawn_blocking(move || {
                    debug!(node = ?node_handle, name = %idname, "task starting");

                    let mut actor_runtime = actor_runtime;
                    actor_runtime.register_std_fds();

                    let result = actor_fn(&actor_runtime);

                    match result {
                        Ok(()) => {
                            debug!(node = ?node_handle, name = %idname, "task completed");
                        }
                        Err(e) => {
                            warn!(node = ?node_handle, name = %idname, error = %e, "task error");
                            actor_runtime.latch_errno(EOWNERDEAD);
                        }
                    }
                    actor_runtime
                })
                .await;

            match blocking_result {
                Ok(mut actor_runtime) => {
                    if let Err(e) = actor_runtime.shutdown().await {
                        warn!(node = ?node_handle, error = %e, "actor shutdown error");
                    }
                }
                Err(e) => {
                    warn!(node = ?node_handle, error = %e, "actor blocking task panicked");
                }
            }
        }
    })
}

/// Receives actor lifecycle events from the IO bridge, updates DAG state,
/// replies to unblock the IO bridge, and wakes the executor so it can react.
/// Emits `ExecutorEvent::NodeTerminated` to `events_tx` on each termination.
async fn lifecycle_event_task(
    dag: Arc<parking_lot::RwLock<Dag>>,
    executor_wakeup: Arc<tokio::sync::watch::Sender<()>>,
    mut rx: mpsc::UnboundedReceiver<ActorLifecycleEvent>,
    events_tx: Option<mpsc::UnboundedSender<ExecutorEvent>>,
    join_waiters: Arc<Mutex<Vec<(Handle, oneshot::Sender<Result<(), String>>)>>>,
) {
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
                // Don't wake executor: is_ready_to_spawn treats Terminating the same
                // as Running, so no spawn decisions change until Terminated.
                // Terminated follows shortly after Terminating, once cleanup completes.
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
                let result = if exit_code == 0 { Ok(()) } else { Err(format!("exit code {exit_code}")) };
                let to_notify: Vec<_> = join_waiters
                    .lock()
                    .extract_if(.., |(h, _)| *h == node_handle)
                    .collect();
                for (h, tx) in to_notify {
                    if tx.send(result.clone()).is_err() {
                        warn!(node = ?h, "join: waiter receiver dropped");
                    }
                }
                if executor_wakeup.send(()).is_err() {
                    warn!(node = ?node_handle, "executor: wakeup after node terminated failed, no receivers");
                }
            }
        }
    }
}

enum SpawnOutcome {
    Ok(HashSet<Handle>),
    FailedNodeDependency(Handle, HashSet<Handle>),
}

/// Classify and spawn one batch of ready nodes from `pending`.
///
/// Spawns actor tasks directly into `actor_tasks` `JoinSet`. The whole batch is
/// always classified and all `Ready` nodes are spawned — a `FailedDependency`
/// seen for one node must not starve a `Ready` sibling in the same batch.
///
/// Returns `Ok(remaining)` with the nodes that are still pending (not spawned),
/// or `FailedNodeDependency(dep, remaining)` when at least one pending node is
/// blocked on a failed dependency `dep`, alongside the nodes still pending
/// after spawning everything that was `Ready`.
fn spawn_ready_actors(
    pending: &HashSet<Handle>,
    env: &Arc<Environment>,
    infra: &ExecutorInfra,
    actor_tasks: &mut JoinSet<()>,
) -> SpawnOutcome {
    let mut to_spawn: Vec<Handle> = Vec::new();
    let mut failed_dependency: Option<Handle> = None;
    {
        let dag_guard = env.dag.read();
        for &n in pending {
            match is_ready_to_spawn(n, &dag_guard, &env.pipe_pool) {
                SpawnReadiness::Ready => to_spawn.push(n),
                SpawnReadiness::Waiting => {}
                SpawnReadiness::FailedDependency(dep) => {
                    failed_dependency.get_or_insert(dep);
                }
            }
        }
    }

    let mut remaining: HashSet<Handle> = pending.clone();

    for node_handle in to_spawn {
        remaining.remove(&node_handle);

        let dag = env.dag.read();
        let Some(node) = dag.get_node(node_handle) else { continue };
        let Some(actor_fn) = env.actor_registry.read().get(&node.idname) else {
            let idname = &node.idname;
            warn!(node = ?node_handle, name = %idname, "actor not registered, skipping");
            drop(dag);
            // Mark as terminated so dependents are not blocked.
            // No lifecycle events needed since the actor was never started.
            {
                let mut dag = env.dag.write();
                dag.set_exit_code(node_handle, crate::errno::ENOENT);
                dag.set_state(node_handle, NodeState::Terminated);
            }
            if infra.executor_wakeup.send(()).is_err() {
                let idname = env.dag.read().get_node(node_handle)
                    .map_or("<unknown>".into(), |n| n.idname.clone());
                warn!(node = ?node_handle, name = %idname, "executor: wakeup after unregistered actor failed, no receivers");
            }
            continue;
        };
        let idname = node.idname.clone(); // spawn_actor_task takes ownership of the name
        drop(dag);

        {
            let mut dag = env.dag.write();
            // Re-check state under the write lock: an actor task running
            // concurrently may have already advanced this node past NotStarted.
            if dag.get_node(node_handle).is_none_or(|n| n.state != NodeState::NotStarted) {
                continue;
            }
            dag.set_state(node_handle, NodeState::Running);
        }

        debug!(node = ?node_handle, name = %idname, "spawning actor task");

        let actor_runtime = BlockingActorRuntime::new(
            node_handle,
            Arc::clone(&infra.io_bridge),
            Arc::clone(&env.suspension),
            infra.lifecycle_tx.clone(),
        );
        let task_handle = spawn_actor_task(
            &infra.async_runtime,
            node_handle,
            idname.clone(),
            actor_fn,
            actor_runtime,
        );

        // Spawn into JoinSet with error handling wrapper.
        // We handle panics here (rather than in run_spawn_loop_jobs) to preserve
        // node_handle context for debugging. JoinError doesn't contain the handle,
        // so moving this to the loop would lose critical diagnostic information.
        actor_tasks.spawn_on(
            async move {
                if let Err(e) = task_handle.await {
                    warn!(error = %e, node = ?node_handle, "actor task panicked");
                }
            },
            &infra.async_runtime,
        );
    }

    match failed_dependency {
        Some(dep) => SpawnOutcome::FailedNodeDependency(dep, remaining),
        None => SpawnOutcome::Ok(remaining),
    }
}

fn remove_blocked_from_pending(
    failed_dep: Handle,
    pending: &mut HashSet<Handle>,
    dag: &Dag,
    join_waiters: &Mutex<Vec<(Handle, oneshot::Sender<Result<(), String>>)>>,
) {
    use std::collections::VecDeque;

    let mut blocked: HashSet<Handle> = HashSet::from([failed_dep]);
    let mut queue: VecDeque<Handle> = VecDeque::from([failed_dep]);

    while let Some(current) = queue.pop_front() {
        let newly_blocked: Vec<Handle> = pending
            .iter()
            .filter(|&&n| dag.resolve_dependencies(n).any(|dep| dep == current))
            .copied()
            .collect();
        for n in newly_blocked {
            pending.remove(&n);
            if blocked.insert(n) {
                queue.push_back(n);
            }
            let to_notify: Vec<_> = join_waiters
                .lock()
                .extract_if(.., |(h, _)| *h == n)
                .collect();
            for (h, tx) in to_notify {
                if tx.send(Err(format!("blocked by failed dependency {failed_dep:?}"))).is_err() {
                    warn!(node = ?h, "join: waiter receiver dropped");
                }
            }
        }
    }
}

/// A job submitted to the executor, pairing a target node with its stop conditions.
struct JobItem {
    target: Handle,
    stop_conditions: StopConditions,
}

/// Spawn loop for `Executor`: waits on job submissions, state-change notifications,
/// and actor task completions simultaneously. Exits when the job channel is closed
/// and all spawned actor tasks have completed. Actor tasks are joined incrementally
/// as they complete to maintain bounded memory usage.
async fn run_spawn_loop_jobs(
    env: &Arc<Environment>,
    infra: &ExecutorInfra,
    job_rx: &mut mpsc::UnboundedReceiver<JobItem>,
    shared_pending: &Mutex<HashSet<Handle>>,
    join_waiters: &Mutex<Vec<(Handle, oneshot::Sender<Result<(), String>>)>>,
) {
    let mut actor_tasks = JoinSet::new();
    let mut channel_closed = false;
    let mut wakeup_rx = infra.executor_wakeup.subscribe();

    loop {
        {
            let mut pending_locked = shared_pending.lock();
            match spawn_ready_actors(&*pending_locked, env, infra, &mut actor_tasks) {
                SpawnOutcome::Ok(remaining) => *pending_locked = remaining,
                SpawnOutcome::FailedNodeDependency(failed_dep, remaining) => {
                    *pending_locked = remaining;
                    let dag = env.dag.read();
                    remove_blocked_from_pending(failed_dep, &mut pending_locked, &dag, join_waiters);
                }
            }
        }

        if channel_closed && actor_tasks.is_empty() {
            break;
        }

        tokio::select! {
            // Receive new job submissions from the executor's job channel.
            // Guard ensures we only listen while the channel is open.
            result = job_rx.recv(), if !channel_closed => {
                match result {
                    Some(item) => {
                        let dag = env.dag.read();
                        let one_step = item.stop_conditions.one_step;
                        // Strip one_step from the iterator conditions: the traversal must
                        // reach all nodes so the NotStarted filter below can skip already-
                        // terminated ones. The one_step limit is applied via .take(1) after
                        // filtering, so a second `run --one-step` advances past done nodes.
                        let topo_conditions = StopConditions { one_step: false, ..item.stop_conditions };
                        let not_started = TopologicalOrderIter::with_stop_conditions(
                            &dag, item.target, topo_conditions,
                        )
                        .filter(|&n| {
                            dag.get_node(n)
                                .is_some_and(|node| node.state == NodeState::NotStarted)
                        });
                        if one_step {
                            shared_pending.lock().extend(not_started.take(1));
                        } else {
                            shared_pending.lock().extend(not_started);
                        }
                    }
                    None => { channel_closed = true; }
                }
            }
            // Wake up when DAG state changes (actor termination, pipe realization, writer close).
            // Coalesces multiple rapid wakeups: watch only delivers the latest value, so
            // concurrent signals don't queue up. We re-check readiness regardless of how
            // many wakeups arrived.
            // Err means the Sender was dropped (executor shutting down); treat as wakeup.
            _ = wakeup_rx.changed() => {}

            // Join completed actor tasks to reclaim memory and maintain bounded growth.
            // This branch completes whenever any task finishes, removing it from the set.
            // No guard needed: when empty, join_next() returns None and doesn't match.
            // Errors are handled in spawn_ready_actors wrapper to preserve node_handle context.
            Some(_) = actor_tasks.join_next() => {
                // Task completed and removed from the set - nothing to do here.
            }
        }
    }

    // Drain any remaining waiters: nodes that were never scheduled or otherwise
    // never reached a terminal state before the executor shut down.
    for (h, tx) in join_waiters.lock().drain(..) {
        if tx.send(Err("executor shut down".into())).is_err() {
            warn!(node = ?h, "join: waiter receiver dropped at shutdown");
        }
    }
}

/// Shared infrastructure for an executor run.
///
/// # Fields
///
/// - `executor_wakeup`: Signals the executor when DAG state changes occur. The
///   lifecycle handler calls `send(())` after updating node states, causing
///   the executor to wake up and check for newly ready nodes. Multiple rapid
///   signals are coalesced — the executor only needs to re-check, not count events.
///
/// - `io_bridge`: Handles all I/O operations for actors (stdin/stdout/stderr/files).
///   Cloned into each `BlockingActorRuntime` so actors can perform I/O.
///
/// - `lifecycle_tx`: Channel sender for actor lifecycle events (Terminating/Terminated).
///   Cloned into each `BlockingActorRuntime`; when an actor shuts down, it sends
///   events via this channel to notify the executor of state transitions.
///
/// - `lifecycle_handler`: Background task that receives lifecycle events from
///   `lifecycle_tx`, updates the DAG state accordingly, and triggers `executor_wakeup`.
///   Also emits `ExecutorEvent::NodeTerminated` to external listeners if configured.
///
/// # Teardown order is critical:
///
/// 1. Join actor tasks (done by `run_spawn_loop_jobs` before calling `shutdown`) —
///    `BlockingActorRuntime::drop` sends lifecycle events and blocks on
///    `lifecycle_handler` replies, so `lifecycle_handler` must still be running.
/// 2. `io_bridge.shutdown()` — flush I/O channels.
/// 3. Drop `lifecycle_tx` — signals `lifecycle_handler` to exit.
/// 4. Drop `io_bridge` — releases the last Arc so `lifecycle_handler` can finish.
/// 5. Join `lifecycle_handler`.
struct ExecutorInfra {
    async_runtime: tokio::runtime::Handle,
    executor_wakeup: Arc<tokio::sync::watch::Sender<()>>,
    io_bridge: Arc<IoBridge>,
    lifecycle_tx: mpsc::UnboundedSender<ActorLifecycleEvent>,
    lifecycle_handler: tokio::task::JoinHandle<()>,
}

impl ExecutorInfra {
    fn new(
        async_runtime: tokio::runtime::Handle,
        env: &Arc<Environment>,
        events_tx: Option<mpsc::UnboundedSender<ExecutorEvent>>,
        join_waiters: Arc<Mutex<Vec<(Handle, oneshot::Sender<Result<(), String>>)>>>,
    ) -> Self {
        let (wakeup_tx, _) = tokio::sync::watch::channel(());
        let executor_wakeup = Arc::new(wakeup_tx);
        let (lifecycle_tx, lifecycle_rx) = mpsc::unbounded_channel::<ActorLifecycleEvent>();
        let io_bridge = Arc::new(IoBridge::new(
            async_runtime.clone(),
            Arc::clone(env),
            Arc::clone(&executor_wakeup),
        ));
        let lifecycle_handler = async_runtime.spawn(lifecycle_event_task(
            Arc::clone(&env.dag),
            Arc::clone(&executor_wakeup),
            lifecycle_rx,
            events_tx,
            join_waiters,
        ));
        Self {
            async_runtime,
            executor_wakeup,
            io_bridge,
            lifecycle_tx,
            lifecycle_handler,
        }
    }

    async fn shutdown(self) {
        let Self {
            async_runtime: _,
            executor_wakeup: _,
            io_bridge,
            lifecycle_tx,
            lifecycle_handler,
        } = self;

        if let Err(e) = io_bridge.shutdown().await {
            warn!(error = %e, "io_bridge shutdown error");
        }
        drop(lifecycle_tx);
        drop(io_bridge);
        if let Err(e) = lifecycle_handler.await {
            warn!(error = %e, "lifecycle handler task failed");
        }
    }
}

/// Handle for interacting with a running executor.
///
/// Created by [`Executor::start()`], consumed by [`Executor::shutdown()`].
/// Use [`Executor::submit()`] to queue jobs while the executor is running.
pub struct Executor {
    job_tx: mpsc::UnboundedSender<JobItem>,
    executor_task: tokio::task::JoinHandle<()>,
    dag: Arc<parking_lot::RwLock<Dag>>,
    pending: Arc<Mutex<HashSet<Handle>>>,
    join_waiters: Arc<Mutex<Vec<(Handle, oneshot::Sender<Result<(), String>>)>>>,
}

impl Executor {
    /// Start a new executor for the given environment.
    ///
    /// # Parameters
    /// - `env`: The actor environment (DAG, pipe pool, etc.)
    /// - `events_tx`: Optional channel for receiving `ExecutorEvent` notifications
    ///
    /// # Returns
    /// An `Executor` handle for submitting jobs and controlling execution.
    #[must_use]
    pub fn start(
        async_runtime: &tokio::runtime::Handle,
        env: Arc<Environment>,
        events_tx: Option<mpsc::UnboundedSender<ExecutorEvent>>,
    ) -> Self {
        let (job_tx, mut job_rx) = mpsc::unbounded_channel::<JobItem>();
        let dag = Arc::clone(&env.dag);
        let pending = Arc::new(Mutex::new(HashSet::new()));
        let pending_clone = Arc::clone(&pending);
        let join_waiters: Arc<Mutex<Vec<(Handle, oneshot::Sender<Result<(), String>>)>>> =
            Arc::new(Mutex::new(Vec::new()));
        let join_waiters_for_lifecycle = Arc::clone(&join_waiters);
        let join_waiters_for_loop = Arc::clone(&join_waiters);
        let infra = ExecutorInfra::new(async_runtime.clone(), &env, events_tx, join_waiters_for_lifecycle);

        let executor_task = async_runtime.spawn(async move {
            run_spawn_loop_jobs(&env, &infra, &mut job_rx, &pending_clone, &join_waiters_for_loop).await;
            infra.shutdown().await;
        });

        Self {
            job_tx,
            executor_task,
            dag,
            pending,
            join_waiters,
        }
    }

    /// Return a snapshot of the nodes currently scheduled for execution.
    ///
    /// The snapshot is a point-in-time copy: it may be stale by the time
    /// the caller uses it, but is consistent and requires no held locks.
    #[must_use]
    pub fn snapshot_pending(&self) -> HashSet<Handle> {
        self.pending.lock().clone()
    }

    /// Wait asynchronously for a node to reach a terminal state.
    ///
    /// Returns `Ok(())` when the node terminates successfully, or `Err` if the
    /// node is removed from pending because a dependency failed.
    #[must_use]
    pub fn join(&self, node: Handle) -> oneshot::Receiver<Result<(), String>> {
        let (tx, rx) = oneshot::channel();

        let dag = self.dag.read();
        let Some(n) = dag.get_node(node) else {
            let _ = tx.send(Err(format!("node {node:?} not found in DAG")));
            return rx;
        };

        match n.state {
            NodeState::Terminated => {
                let result = if n.exit_code == 0 { Ok(()) } else { Err(format!("exit code {}", n.exit_code)) };
                let _ = tx.send(result);
            }
            NodeState::NotStarted | NodeState::Running | NodeState::Terminating => {
                self.join_waiters.lock().push((node, tx));
            }
        }

        rx
    }

    /// Submit a job (target node) to the executor.
    ///
    /// Returns immediately without blocking — the job runs asynchronously.
    ///
    /// # Errors
    /// Returns `Err` if the executor has already shut down.
    ///
    /// `stop_conditions` controls which nodes in the target's dependency graph
    /// are included in this job's execution.
    pub fn submit(
        &self,
        target: Handle,
        stop_conditions: StopConditions,
    ) -> Result<(), mpsc::error::SendError<Handle>> {
        self.job_tx
            .send(JobItem {
                target,
                stop_conditions,
            })
            .map_err(|e| mpsc::error::SendError(e.0.target))
    }

    /// Wait for all submitted jobs to complete and clean up executor resources.
    ///
    /// Closes the job submission channel, waits for all in-flight work to finish,
    /// then tears down internal infrastructure in the correct order.
    pub async fn shutdown(self) {
        drop(self.job_tx); // close the channel → signals executor loop to exit
        if let Err(e) = self.executor_task.await {
            warn!(error = %e, "executor task panicked");
        }
    }
}
