//! Glue code between synchronous CLI and async ailetos runtime.
//!
//! This module bridges the synchronous CLI (main thread, rustyline REPL) with
//! the asynchronous ailetos executor running on a dedicated tokio runtime.
//! It provides:
//! - Event bridging (tokio → sync channels)
//! - Background threads (Ctrl+C handler, notification watcher)
//! - Executor startup and environment setup

use std::sync::{Arc, Mutex};

use ailetos::{Environment, Executor, ExecutorEvent, Handle, KVBuffers, MemKV};

use crate::output::OutputSink;

// ---------------------------------------------------------------------------
// Internal types
// ---------------------------------------------------------------------------

/// State set by `join_handle` so the watcher thread knows what to signal.
pub struct JoinWaiter {
    pub target: Handle,
    pub ready_tx: std::sync::mpsc::SyncSender<()>,
    pub ctrlc_tx: std::sync::mpsc::Sender<()>,
}

/// Sent to the watcher thread when the executor is replaced (on `reset`).
pub struct WatcherUpdate {
    pub events_rx: std::sync::mpsc::Receiver<ExecutorEvent>,
    pub env: Arc<Environment>,
}

// ---------------------------------------------------------------------------
// Executor startup helpers
// ---------------------------------------------------------------------------

pub fn start_executor_with_bridge(
    rt: &tokio::runtime::Runtime,
    env: Arc<Environment>,
) -> (Executor, std::sync::mpsc::Receiver<ExecutorEvent>) {
    let (tokio_tx, mut tokio_rx) = tokio::sync::mpsc::unbounded_channel::<ExecutorEvent>();
    let (sync_tx, sync_rx) = std::sync::mpsc::channel::<ExecutorEvent>();

    let executor = {
        let _guard = rt.enter();
        Executor::start(env, Some(tokio_tx))
    };

    rt.spawn(async move {
        while let Some(event) = tokio_rx.recv().await {
            if sync_tx.send(event).is_err() {
                break;
            }
        }
    });

    (executor, sync_rx)
}

pub fn make_env(kv: &Arc<MemKV>) -> Arc<Environment> {
    let env = Arc::new(Environment::new(Arc::clone(kv) as Arc<dyn KVBuffers>));
    {
        let mut reg = env.actor_registry.write();
        reg.register("cat", cat::execute);
        reg.register("dbg", crate::dbg_actor::execute);
        reg.register("shell_input", crate::shell_input_actor::execute);
    }
    env
}

/// Spawn the global Ctrl+C handler thread.
///
/// Listens for Ctrl+C once at process startup. When Ctrl+C is received,
/// checks if there's a pending join and notifies it via its ctrlc_tx channel.
/// This avoids spawning per-join threads and ensures POSIX-compliant signal handling.
pub fn start_ctrlc_handler(
    pending_join: Arc<Mutex<Option<JoinWaiter>>>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let Ok(rt) = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        else {
            return;
        };
        rt.block_on(async {
            loop {
                if tokio::signal::ctrl_c().await.is_err() {
                    break;
                }
                // Ctrl+C received; notify the current waiter if one exists
                let mut pending = pending_join.lock().unwrap();
                if let Some(waiter) = pending.take() {
                    let _ = waiter.ctrlc_tx.send(());
                }
            }
        });
    })
}

/// Spawn the watcher thread.
///
/// The watcher owns `events_rx` for the current executor. On each event:
/// - if `pending_join` targets this handle → signal the waiter
/// - otherwise → print a notification via `notification_sink`
///
/// When the executor is replaced (`reset`), `DagShell` sends a `WatcherUpdate`
/// so the watcher switches to the new receiver. When `update_rx` closes (on
/// `DagShell` drop), the watcher exits.
pub fn start_notification_watcher(
    initial: WatcherUpdate,
    update_rx: std::sync::mpsc::Receiver<WatcherUpdate>,
    pending_join: Arc<Mutex<Option<JoinWaiter>>>,
    notification_sink: Arc<dyn OutputSink>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut env = initial.env;
        let mut events_rx = initial.events_rx;

        loop {
            match update_rx.try_recv() {
                Ok(upd) => {
                    env = upd.env;
                    events_rx = upd.events_rx;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
            }

            match events_rx.recv_timeout(std::time::Duration::from_millis(50)) {
                Ok(ExecutorEvent::NodeTerminated(h)) => {
                    let mut pending = pending_join.lock().unwrap();
                    if pending.as_ref().map(|j| j.target == h).unwrap_or(false) {
                        if let Some(waiter) = pending.take() {
                            let _ = waiter.ready_tx.send(());
                        }
                    } else if pending.is_none() {
                        let name = {
                            let dag = env.dag.read();
                            dag.get_node(h)
                                .map(|n| format!("{}#{}", n.idname, h.id()))
                                .unwrap_or_else(|| format!("node#{}", h.id()))
                        };
                        notification_sink.println(&format!("[{name}] done"));
                    }
                    // else: foreground join active but not our target — suppress
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    // Old executor done; wait for the next executor (or drop).
                    match update_rx.recv() {
                        Ok(upd) => {
                            env = upd.env;
                            events_rx = upd.events_rx;
                        }
                        Err(_) => break,
                    }
                }
            }
        }
    })
}
