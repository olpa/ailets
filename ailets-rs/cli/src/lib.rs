//! DAG Shell library - DagShell and OutputSink.
//!
//! Two dedicated tokio runtimes are owned by `DagShell`:
//! - `ailetos_async_rt`: runs the ailetos executor exclusively.
//! - `cli_rt`: runs all CLI-side async work: notification watcher, Ctrl+C handler, join waits, sleeps.
//! The CLI thread itself stays synchronous and drives async work via `block_on`.

pub(crate) mod dbg_actor;
pub(crate) mod dbg_control;
pub(crate) mod shell_input_actor;
pub(crate) mod shell_input_control;

mod commands;
mod output;
pub mod shell_ui;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use ailetos::{Environment, ExecutorEvent, Executor, Handle, KVBuffers, MemKV};

// Re-exports
pub use output::{OutputSink, StdoutSink};

// ---------------------------------------------------------------------------
// Internal types
// ---------------------------------------------------------------------------

pub(crate) struct JoinWaiter {
    pub target: Handle,
    pub ready_tx: tokio::sync::oneshot::Sender<()>,
    pub ctrlc_tx: tokio::sync::oneshot::Sender<()>,
}

fn make_env(kv: &Arc<MemKV>) -> Arc<Environment> {
    let env = Arc::new(Environment::new(Arc::clone(kv) as Arc<dyn KVBuffers>));
    {
        let mut reg = env.actor_registry.write();
        reg.register("cat", cat::execute);
        reg.register("dbg", dbg_actor::execute);
        reg.register("shell_input", shell_input_actor::execute);
    }
    env
}

fn start_executor(
    ailetos_async_rt: tokio::runtime::Handle,
    env: Arc<Environment>,
) -> (Executor, tokio::sync::mpsc::UnboundedReceiver<ExecutorEvent>) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<ExecutorEvent>();
    let executor = Executor::start(ailetos_async_rt, env, Some(tx));
    (executor, rx)
}

fn start_ctrlc_handler(
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

fn start_notification_watcher(
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

// ---------------------------------------------------------------------------
// DagShell
// ---------------------------------------------------------------------------

pub struct DagShell {
    pub(crate) env: Arc<Environment>,
    pub(crate) kv: Arc<MemKV>,
    pub(crate) handles: Vec<Handle>,
    pub(crate) vars: HashMap<String, Handle>,
    pub(crate) sink: Box<dyn OutputSink>,
    pub(crate) notification_sink: Arc<dyn OutputSink>,
    pub(crate) pending_join: Arc<Mutex<Option<JoinWaiter>>>,
    _watcher: tokio::task::JoinHandle<()>,
    // Global Ctrl+C handler task; kept alive until DagShell drops.
    _ctrlc_handler: tokio::task::JoinHandle<()>,
    // executor drops before ailetos_async_rt (declaration order = drop order).
    pub(crate) executor: Executor,
    pub(crate) ailetos_async_rt: tokio::runtime::Runtime,
    // CLI-side async runtime: join waits, sleeps. Independent from ailetos.
    pub(crate) cli_rt: tokio::runtime::Runtime,
}

impl DagShell {
    pub fn new() -> Self {
        Self::new_with_sinks(Box::new(StdoutSink), Arc::new(StdoutSink))
    }

    pub fn new_with_sink(sink: Box<dyn OutputSink>) -> Self {
        Self::new_with_sinks(sink, Arc::new(StdoutSink))
    }

    /// Create a shell with separate sinks for synchronous command output and
    /// background notifications (node terminations while at the prompt).
    pub fn new_with_sinks(
        command_sink: Box<dyn OutputSink>,
        notification_sink: Arc<dyn OutputSink>,
    ) -> Self {
        let ailetos_async_rt =
            tokio::runtime::Runtime::new().expect("failed to create ailetos runtime");
        Self::new_with_sinks_and_rt(command_sink, notification_sink, ailetos_async_rt)
    }

    /// Like `new_with_sinks` but accepts a pre-created runtime for ailetos.
    /// The caller must ensure this runtime is used exclusively for ailetos.
    pub fn new_with_sinks_and_rt(
        command_sink: Box<dyn OutputSink>,
        notification_sink: Arc<dyn OutputSink>,
        ailetos_async_rt: tokio::runtime::Runtime,
    ) -> Self {
        let cli_rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to create cli runtime");

        let kv = Arc::new(MemKV::new());
        let env = make_env(&kv);
        let (executor, events_rx) = start_executor(ailetos_async_rt.handle().clone(), Arc::clone(&env));

        let pending_join: Arc<Mutex<Option<JoinWaiter>>> = Arc::new(Mutex::new(None));

        let notification_sink_clone = Arc::clone(&notification_sink);
        let watcher = start_notification_watcher(
            cli_rt.handle(),
            events_rx,
            Arc::clone(&env),
            Arc::clone(&pending_join),
            notification_sink,
        );

        let ctrlc_handler = start_ctrlc_handler(cli_rt.handle(), Arc::clone(&pending_join));

        Self {
            env,
            kv,
            handles: Vec::new(),
            vars: HashMap::new(),
            sink: command_sink,
            notification_sink: notification_sink_clone,
            pending_join,
            _watcher: watcher,
            _ctrlc_handler: ctrlc_handler,
            executor,
            ailetos_async_rt,
            cli_rt,
        }
    }

    fn parse_handle(&self, s: &str) -> Option<Handle> {
        if let Some(var_name) = s.strip_prefix('$') {
            return self.vars.get(var_name).copied();
        }
        s.parse::<i64>().ok().map(Handle::new)
    }

    pub fn execute(&mut self, line: &str) -> Result<bool, String> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        let (cmd, rest) = match parts.split_first() {
            None => return Ok(true),
            Some((cmd, rest)) => (*cmd, rest),
        };

        match cmd {
            "quit" | "exit" | "q" => {
                self.prepare_exit();
                return Ok(false);
            }
            "help" | "?" => self.cmd_help(),
            "set" => self.cmd_set(rest)?,
            "node" => {
                self.cmd_node(rest)?;
            }
            "dep" => self.cmd_dep(rest)?,
            "deps" => self.cmd_deps(rest)?,
            "show" => self.cmd_show(rest)?,
            "run" => self.cmd_run(rest)?,
            "join" | "await" => self.cmd_join(rest)?,
            "follow" => self.cmd_follow(rest)?,
            "cat" => self.cmd_cat(rest)?,
            "status" => self.cmd_status(rest)?,
            "source" | "load" => self.cmd_source(rest)?,
            "suspend" => self.cmd_suspend(rest)?,
            "resume" => self.cmd_resume(rest)?,
            "wait" => self.cmd_wait(rest)?,
            "write" => self.cmd_write(rest)?,
            "close" => self.cmd_close(rest)?,
            "kill" => self.cmd_kill(rest)?,
            _ => {
                self.sink
                    .println(&format!("Unknown command: {cmd}. Type 'help' for usage."));
            }
        }

        Ok(true)
    }

    pub fn sleep(&self, duration: std::time::Duration) {
        self.cli_rt
            .block_on(async move { tokio::time::sleep(duration).await });
    }

    fn prepare_exit(&mut self) {
        shell_input_control::close_all_shell_inputs();
        for &handle in &self.handles {
            self.env.suspension.resume(handle);
        }
    }
}

impl Default for DagShell {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for DagShell {
    fn drop(&mut self) {
        self.prepare_exit();
        // executor and ailetos_async_rt drop in declaration order, closing the
        // event channel and causing the watcher task to exit.
    }
}
