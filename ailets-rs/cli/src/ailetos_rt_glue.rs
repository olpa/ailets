//! Async task factories and executor startup for the CLI.
//!
//! Provides functions to start the ailetos executor, the notification watcher,
//! and the Ctrl+C handler. Callers decide which runtime each task runs on.

use std::sync::{Arc, Mutex};

use ailetos::{Environment, Executor, ExecutorEvent, Handle, KVBuffers, MemKV};

use crate::output::OutputSink;

// ---------------------------------------------------------------------------
// Internal types
// ---------------------------------------------------------------------------

/// State set by `join_handle` so the watcher task knows what to signal.
pub struct JoinWaiter {
    pub target: Handle,
    pub ready_tx: tokio::sync::oneshot::Sender<()>,
    pub ctrlc_tx: tokio::sync::oneshot::Sender<()>,
}

// ---------------------------------------------------------------------------
// Executor startup helpers
// ---------------------------------------------------------------------------

pub fn start_executor_with_bridge(
    ailetos_async_rt: tokio::runtime::Handle,
    env: Arc<Environment>,
) -> (Executor, tokio::sync::mpsc::UnboundedReceiver<ExecutorEvent>) {
    let (tokio_tx, tokio_rx) = tokio::sync::mpsc::unbounded_channel::<ExecutorEvent>();
    let executor = Executor::start(ailetos_async_rt.clone(), env, Some(tokio_tx));
    (executor, tokio_rx)
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

/// Spawn the global Ctrl+C handler task.
///
/// When Ctrl+C is received, notifies the pending join via its ctrlc_tx channel.
pub fn start_ctrlc_handler(
    rt: &tokio::runtime::Handle,
    pending_join: Arc<Mutex<Option<JoinWaiter>>>,
) -> tokio::task::JoinHandle<()> {
    rt.spawn(async move {
        loop {
            if tokio::signal::ctrl_c().await.is_err() {
                break;
            }
            let mut pending = pending_join.lock().unwrap();
            if let Some(waiter) = pending.take() {
                let _ = waiter.ctrlc_tx.send(());
            }
        }
    })
}

/// Spawn the notification watcher task.
///
/// On each event:
/// - if `pending_join` targets this handle → signal the waiter
/// - otherwise → print a notification via `notification_sink`
pub fn start_notification_watcher(
    rt: &tokio::runtime::Handle,
    mut events_rx: tokio::sync::mpsc::UnboundedReceiver<ExecutorEvent>,
    env: Arc<Environment>,
    pending_join: Arc<Mutex<Option<JoinWaiter>>>,
    notification_sink: Arc<dyn OutputSink>,
) -> tokio::task::JoinHandle<()> {
    rt.spawn(async move {
        while let Some(ExecutorEvent::NodeTerminated(h)) = events_rx.recv().await {
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
    })
}
