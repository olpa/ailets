//! Executor - actor execution for the actor system
//!
//! This module handles:
//! - Topological ordering of DAG nodes
//! - Readiness checking for node spawning
//! - The spawn loop that runs actors
//! - Top-level execution of a DAG run

use std::collections::HashSet;
use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::actor_syscall::ActorLifecycleEvent;
use crate::attachments::AttachmentManager;
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
    infra: &ExecutorInfra,
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

/// A job submitted to the executor, pairing a target node with its stop conditions.
struct JobItem {
    target: Handle,
    stop_conditions: StopConditions,
}

/// Spawn loop for `Executor`: waits on both the job channel and state-change
/// notifications simultaneously, exiting only when the channel is closed and
/// no actors are running. Joins all actor tasks before returning.
async fn run_spawn_loop_jobs(
    env: &Arc<Environment>,
    infra: &ExecutorInfra,
    job_rx: &mut mpsc::UnboundedReceiver<JobItem>,
) {
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
                    Some(item) => {
                        let dag = env.dag.read();
                        pending.extend(
                            TopologicalOrderIter::with_stop_conditions(
                                &dag, item.target, item.stop_conditions,
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

    for task in actor_tasks {
        if let Err(e) = task.await {
            warn!(error = %e, "actor task failed");
        }
    }
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
/// 1. Join actor tasks (done by `run_spawn_loop_jobs` before calling `shutdown`) —
///    `BlockingActorRuntime::drop` sends lifecycle events and blocks on
///    `lifecycle_handler` replies, so `lifecycle_handler` must still be running.
/// 2. `io_bridge.shutdown()` — flush I/O channels.
/// 3. `attachment_manager.shutdown()` — wait for attachment tasks.
/// 4. Drop `lifecycle_tx` — signals `lifecycle_handler` to exit.
/// 5. Drop `io_bridge` — releases the last Arc so `lifecycle_handler` can finish.
/// 6. Join `lifecycle_handler`.
struct ExecutorInfra {
    spawn_wakeup: Arc<tokio::sync::Notify>,
    io_bridge: Arc<IoBridge>,
    lifecycle_tx: mpsc::UnboundedSender<ActorLifecycleEvent>,
    lifecycle_handler: tokio::task::JoinHandle<()>,
    attachment_manager: Arc<AttachmentManager>,
}

impl ExecutorInfra {
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

    async fn shutdown(self) {
        let Self {
            spawn_wakeup: _,
            io_bridge,
            lifecycle_tx,
            lifecycle_handler,
            attachment_manager,
        } = self;

        if let Err(e) = io_bridge.shutdown().await {
            warn!(error = %e, "io_bridge shutdown error");
        }
        if let Err(e) = attachment_manager.shutdown().await {
            warn!(error = %e, "attachment manager shutdown error");
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
    /// Kept to expose io_bridge() for callers that need direct I/O access.
    /// todo: remove once kill_actor() is properly implemented (fix-kill-command.md)
    infra: Arc<ExecutorInfra>,
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
    pub fn start(
        env: Arc<Environment>,
        events_tx: Option<mpsc::UnboundedSender<ExecutorEvent>>,
    ) -> Self {
        let (job_tx, mut job_rx) = mpsc::unbounded_channel::<JobItem>();
        let infra = Arc::new(ExecutorInfra::new(&env, events_tx));
        let infra_task = Arc::clone(&infra);

        let executor_task = tokio::spawn(async move {
            run_spawn_loop_jobs(&env, &infra_task, &mut job_rx).await;
            // By the time run_spawn_loop_jobs returns, Executor::shutdown() has already
            // dropped its infra Arc, so infra_task is the sole owner here.
            let infra = Arc::try_unwrap(infra_task)
                .unwrap_or_else(|_| panic!("executor: expected sole infra ownership at shutdown"));
            infra.shutdown().await;
        });

        Self {
            job_tx,
            executor_task,
            infra,
        }
    }

    /// Submit a job (target node) to the executor.
    ///
    /// Returns immediately without blocking — the job runs asynchronously.
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
            .send(JobItem { target, stop_conditions })
            .map_err(|e| mpsc::error::SendError(e.0.target))
    }

    /// Return a reference to the I/O bridge for this executor run.
    ///
    /// # WARNING
    /// This is a temporary escape hatch for CLI code that needs direct I/O bridge
    /// access. It will be removed once `kill_actor()` is properly implemented.
    /// See: `doc/in_progress/fix-kill-command.md`
    pub fn io_bridge(&self) -> Arc<IoBridge> {
        Arc::clone(&self.infra.io_bridge)
    }

    /// Kill a running actor with the specified exit code.
    ///
    /// # NOTE
    /// Not yet implemented. See `doc/in_progress/fix-kill-command.md`.
    pub async fn kill_actor(&self, _handle: Handle, _exit_code: i32) -> Result<(), String> {
        // todo: implement as part of fix-kill-command.md
        Err("kill_actor not yet implemented".to_string())
    }

    /// Wait for all submitted jobs to complete and clean up executor resources.
    ///
    /// Closes the job submission channel, waits for all in-flight work to finish,
    /// then tears down internal infrastructure in the correct order.
    pub async fn shutdown(self) {
        let Self {
            job_tx,
            executor_task,
            infra,
        } = self;
        drop(job_tx); // close the channel → signals executor loop to exit
        drop(infra); // release our Arc → executor task becomes sole owner
        if let Err(e) = executor_task.await {
            warn!(error = %e, "executor task panicked");
        }
    }
}

