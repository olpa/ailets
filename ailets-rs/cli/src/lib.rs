//! DAG Shell library — `DagShell` and `OutputSink`.
//!
//! Two dedicated tokio runtimes are owned by `DagShell`:
//!
//! - `ailetos_async_rt`: runs the ailetos executor exclusively.
//! - `cli_rt`: runs all CLI-side async work: notification watcher, join waits, sleeps.
//!
//! The CLI thread itself stays synchronous and drives async work via `block_on`.
//! TCL scripts are parsed and executed by a `molt::Interp` owned by the caller and
//! passed into `DagShell::execute`.  Create one with `make_interp()`.

pub(crate) mod dbg_actor;
pub(crate) mod dbg_control;
pub(crate) mod shell_input_actor;
pub(crate) mod shell_input_control;

mod commands;
mod output;
pub mod prompt_nodes;
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
                                // warning: there is a theoretical race here. The executor
                                // sends NodeTerminated to this channel BEFORE signalling the
                                // join receiver, so events are already queued when the main
                                // thread's block_on returns. However, this watcher task runs
                                // on a separate worker thread and may not be scheduled until
                                // after the main thread clears foreground_join — meaning a
                                // notification could slip through. In practice this is rare
                                // and considered not worth fixing.
                                if !foreground_join.load(std::sync::atomic::Ordering::Acquire) {
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
    pub(crate) env: Arc<Environment>,
    pub(crate) kv: Arc<MemKV>,
    pub(crate) sink: Box<dyn OutputSink>,
    pub(crate) notification_sink: Arc<dyn OutputSink>,
    pub(crate) foreground_join: Arc<AtomicBool>,
    // Tracked reader tasks (follow / run --bg). Drained in prepare_exit so the
    // last bytes written by an actor are never silently dropped on fast exit.
    pub(crate) reader_tasks: tokio::task::JoinSet<()>,
    watcher: NotificationWatcher,
    // executor drops before ailetos_async_rt (declaration order = drop order).
    pub(crate) executor: Executor,
    pub(crate) ailetos_async_rt: tokio::runtime::Runtime,
    // CLI-side async runtime: join waits, sleeps. Independent from ailetos.
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
    /// background notifications (node terminations while at the prompt).
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
    /// The caller must ensure this runtime is used exclusively for ailetos.
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
            env,
            kv,
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
        if let Ok(id) = s.parse::<i64>() {
            return Some(Handle::new(id));
        }
        let dag = self.env.dag.read();
        if let Some((name, n_str)) = s.rsplit_once('.') {
            let id: i64 = n_str.parse().ok()?;
            let handle = Handle::new(id);
            dag.get_node(handle)
                .filter(|n| n.idname == name)
                .map(|_| handle)
        } else {
            let mut matches = dag.nodes().filter(|n| n.idname == s);
            let first = matches.next()?;
            matches.next().is_none().then_some(first.pid)
        }
    }

    /// Evaluate a TCL script.  Variables set in one call are visible in subsequent calls
    /// because the caller owns and reuses the `molt::Interp` across invocations.
    ///
    /// # Errors
    /// Returns a TCL error message if script evaluation fails.
    pub fn execute(
        &mut self,
        interp: &mut molt::Interp,
        ctx: molt::types::ContextID,
        script: &str,
    ) -> Result<ShellControl, String> {
        // Why a raw pointer: molt's command handlers have a fixed signature
        // (interp, ctx, argv) with no room for extra parameters.  The only way
        // to reach DagShell from inside a handler is through the ShellContext
        // stored in the interpreter.  We can't store a Rust reference there
        // because `self` is already mutably borrowed for the duration of this
        // function, and Rust would refuse a second &mut borrow of the same
        // value.  A raw pointer sidesteps the borrow checker; the pointer is
        // valid for the entire eval() call because `self` is borrowed for at
        // least that long, and no handler touches `interp` (the other borrow),
        // so there is no actual aliasing.
        interp.context::<tcl_interp::ShellContext>(ctx).shell =
            std::ptr::from_mut::<DagShell>(self);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| interp.eval(script)));
        interp.context::<tcl_interp::ShellContext>(ctx).shell = std::ptr::null_mut();
        let result = match result {
            Ok(r) => r,
            Err(payload) => std::panic::resume_unwind(payload),
        };

        let shell_ctx = interp.context::<tcl_interp::ShellContext>(ctx);
        let exit_requested = shell_ctx.exit_requested;
        shell_ctx.exit_requested = false;

        if exit_requested {
            return Ok(ShellControl::Exit);
        }

        match result {
            Ok(_) => Ok(ShellControl::Continue),
            Err(e) => Err(e.value().as_str().to_string()),
        }
    }

    /// Register prompt items as DAG nodes, creating a stdin actor if needed.
    ///
    /// Returns `true` if stdin was consumed by any `PromptArg::Stdin` item.
    ///
    /// # Errors
    /// Returns an error if a file cannot be read or its type cannot be determined.
    pub fn register_prompt_inputs(
        &mut self,
        items: &[shell_ui::PromptArg],
    ) -> Result<bool, String> {
        let needs_stdin = items.iter().any(|a| matches!(a, shell_ui::PromptArg::Stdin));
        let stdin_handle = if needs_stdin {
            let handle = self.env.add_node("shell_input".to_string(), &[], None);
            shell_input_control::register_shell_input_actor(handle);
            Some(handle)
        } else {
            None
        };
        prompt_nodes::register_prompt_inputs(
            &self.env,
            self.ailetos_async_rt.handle(),
            items,
            stdin_handle,
        )
    }

    fn prepare_exit(&mut self) {
        shell_input_control::close_all_shell_inputs();
        let handles: Vec<Handle> = self.env.dag.read().nodes().map(|n| n.pid).collect();
        for handle in handles {
            self.env.suspension.resume(handle);
        }
        // Unblock reader tasks waiting on pipes that will never be realized
        // (e.g. a node whose dep failed without ever opening stdout).
        // Must happen before draining reader_tasks to avoid deadlock.
        self.env.pipe_pool.close_all_leftover_writers();
        tracing::debug!(
            count = self.reader_tasks.len(),
            "prepare_exit: draining reader tasks"
        );
        let rt = &self.ailetos_async_rt;
        let tasks = &mut self.reader_tasks;
        rt.block_on(async { while tasks.join_next().await.is_some() {} });
        self.watcher.shutdown(&self.cli_rt);
        // executor and ailetos_async_rt drop in declaration order, closing the
        // event channel and causing the watcher task to exit.
    }
}

pub use tcl_interp::make_interp;

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
