//! DAG Shell library — `DagShell` and `OutputSink`.
//!
//! Two dedicated tokio runtimes are owned by `DagShell`:
//!
//! - `ailetos_async_rt`: runs the ailetos executor exclusively.
//! - `cli_rt`: runs all CLI-side async work: notification watcher, join waits, sleeps.
//!
//! The CLI thread itself stays synchronous and drives async work via `block_on`.
//! TCL scripts are parsed and executed by a persistent `molt::Interp` stored in `tcl`.

pub(crate) mod dbg_actor;
pub(crate) mod dbg_control;
pub(crate) mod shell_input_actor;
pub(crate) mod shell_input_control;

mod commands;
mod output;
pub mod shell_ui;
mod tcl_interp;

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use ailetos::{Environment, Executor, ExecutorEvent, Handle, KVBuffers, MemKV};

// Re-exports
pub use output::{OutputSink, StdoutSink};

/// Outcome of a shell command: whether the REPL loop should continue or exit.
pub enum ShellControl {
    Continue,
    Exit,
}

fn make_env(kv: &Arc<MemKV>) -> Arc<Environment> {
    let env = Arc::new(Environment::new(Arc::clone(kv) as Arc<dyn KVBuffers>));
    {
        let mut reg = env.actor_registry.write();
        reg.register("cat", cat::execute);
        reg.register("dbg", dbg_actor::execute);
        reg.register("shell_input", shell_input_actor::execute);
        reg.register("query", query::execute);
        reg.register("messages_to_query", messages_to_query::execute);
        reg.register("messages_to_markdown", messages_to_markdown::execute);
        reg.register("gpt.response_to_messages", gpt::execute);
    }
    env
}

fn start_executor(
    ailetos_async_rt: &tokio::runtime::Handle,
    env: Arc<Environment>,
) -> (
    Executor,
    tokio::sync::mpsc::UnboundedReceiver<ExecutorEvent>,
) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<ExecutorEvent>();
    let executor = Executor::start(ailetos_async_rt, env, Some(tx));
    (executor, rx)
}

// ---------------------------------------------------------------------------
// NotificationWatcher
// ---------------------------------------------------------------------------

struct NotificationWatcher {
    cancel: CancellationToken,
    task: tokio::task::JoinHandle<()>,
}

impl NotificationWatcher {
    fn spawn(
        rt: &tokio::runtime::Handle,
        mut events_rx: tokio::sync::mpsc::UnboundedReceiver<ExecutorEvent>,
        env: Arc<Environment>,
        foreground_join: Arc<AtomicBool>,
        sink: Arc<dyn OutputSink>,
    ) -> Self {
        let cancel = CancellationToken::new();
        let task = rt.spawn({
            let cancel = cancel.clone();
            async move {
                loop {
                    tokio::select! {
                        () = cancel.cancelled() => break,
                        event = events_rx.recv() => match event {
                            Some(ExecutorEvent::NodeTerminated(h)) => {
                                if !foreground_join.load(std::sync::atomic::Ordering::Relaxed) {
                                    let name = {
                                        let dag = env.dag.read();
                                        dag.get_node(h)
                                            .map_or_else(|| format!("node#{}", h.id()), |n| format!("{}#{}", n.idname, h.id()))
                                    };
                                    sink.println(&format!("[{name}] done"));
                                }
                            }
                            None => break,
                        },
                    }
                }
            }
        });
        Self { cancel, task }
    }

    fn shutdown(&mut self, rt: &tokio::runtime::Runtime) {
        self.cancel.cancel();
        rt.block_on(&mut self.task).ok();
    }
}

// ---------------------------------------------------------------------------
// DagShell
// ---------------------------------------------------------------------------

pub struct DagShell {
    // TCL interpreter — taken out of self during execute() to avoid aliasing.
    // None only while execute() is on the call stack.
    pub(crate) tcl: Option<molt::Interp>,
    pub(crate) env: Arc<Environment>,
    pub(crate) kv: Arc<MemKV>,
    pub(crate) handles: Vec<Handle>,
    pub(crate) sink: Box<dyn OutputSink>,
    pub(crate) notification_sink: Arc<dyn OutputSink>,
    pub(crate) foreground_join: Arc<AtomicBool>,
    pub(crate) reader_tasks: tokio::task::JoinSet<()>,
    watcher: NotificationWatcher,
    // executor drops before ailetos_async_rt (declaration order = drop order).
    pub(crate) executor: Executor,
    pub(crate) ailetos_async_rt: tokio::runtime::Runtime,
    pub(crate) cli_rt: tokio::runtime::Runtime,
}

impl DagShell {
    #[must_use]
    pub fn new() -> Self {
        Self::new_with_sinks(Box::new(StdoutSink), Arc::new(StdoutSink))
    }

    #[must_use]
    pub fn new_with_sink(sink: Box<dyn OutputSink>) -> Self {
        Self::new_with_sinks(sink, Arc::new(StdoutSink))
    }

    /// Create a shell with separate sinks for synchronous command output and
    /// background notifications.
    ///
    /// # Panics
    /// Panics if the tokio runtime cannot be created.
    #[allow(clippy::expect_used)]
    pub fn new_with_sinks(
        command_sink: Box<dyn OutputSink>,
        notification_sink: Arc<dyn OutputSink>,
    ) -> Self {
        let ailetos_async_rt =
            tokio::runtime::Runtime::new().expect("failed to create ailetos runtime");
        Self::new_with_sinks_and_rt(command_sink, notification_sink, ailetos_async_rt)
    }

    /// Like `new_with_sinks` but accepts a pre-created runtime for ailetos.
    ///
    /// # Panics
    /// Panics if the CLI tokio runtime cannot be created.
    #[allow(clippy::expect_used)]
    pub fn new_with_sinks_and_rt(
        command_sink: Box<dyn OutputSink>,
        notification_sink: Arc<dyn OutputSink>,
        ailetos_async_rt: tokio::runtime::Runtime,
    ) -> Self {
        let cli_rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("failed to create cli runtime");

        let kv = Arc::new(MemKV::new());
        let env = make_env(&kv);
        let (executor, events_rx) = start_executor(ailetos_async_rt.handle(), Arc::clone(&env));

        let foreground_join = Arc::new(AtomicBool::new(false));

        let notification_sink_clone = Arc::clone(&notification_sink);
        let watcher = NotificationWatcher::spawn(
            cli_rt.handle(),
            events_rx,
            Arc::clone(&env),
            Arc::clone(&foreground_join),
            notification_sink,
        );

        Self {
            tcl: None,
            env,
            kv,
            handles: Vec::new(),
            sink: command_sink,
            notification_sink: notification_sink_clone,
            foreground_join,
            reader_tasks: tokio::task::JoinSet::new(),
            watcher,
            executor,
            ailetos_async_rt,
            cli_rt,
        }
    }

    pub(crate) fn parse_handle(&self, s: &str) -> Option<Handle> {
        s.parse::<i64>().ok().map(Handle::new)
    }

    /// Evaluate a TCL script.  Variables set in one call are visible in subsequent calls
    /// because the `molt::Interp` is persisted in `self.tcl` between invocations.
    ///
    /// # Errors
    /// Returns a TCL error message if script evaluation fails.
    pub fn execute(&mut self, script: &str) -> Result<ShellControl, String> {
        // Take the interpreter out of self so that command handlers can borrow
        // other fields of self through the thread-local without aliasing self.tcl.
        let mut tcl = self.tcl.take().unwrap_or_else(tcl_interp::make_interp);

        // Safety: see tcl_interp::get_shell safety comment.
        tcl_interp::set_shell(self as *mut DagShell);
        let result = tcl.eval(script);
        tcl_interp::clear_shell();

        self.tcl = Some(tcl);

        if tcl_interp::take_exit_requested() {
            return Ok(ShellControl::Exit);
        }

        match result {
            Ok(_) => Ok(ShellControl::Continue),
            Err(e) => Err(e.value().as_str().to_string()),
        }
    }

    fn prepare_exit(&mut self) {
        shell_input_control::close_all_shell_inputs();
        for &handle in &self.handles {
            self.env.suspension.resume(handle);
        }
        self.env.pipe_pool.close_all_leftover_writers();
        tracing::debug!(
            count = self.reader_tasks.len(),
            "prepare_exit: draining reader tasks"
        );
        let rt = &self.ailetos_async_rt;
        let tasks = &mut self.reader_tasks;
        rt.block_on(async { while tasks.join_next().await.is_some() {} });
        self.watcher.shutdown(&self.cli_rt);
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
    }
}
