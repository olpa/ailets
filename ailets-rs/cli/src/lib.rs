//! DAG Shell library - `DagShell` and `OutputSink`.
//!
//! Two dedicated tokio runtimes are owned by `DagShell`:
//!
//! - `ailetos_async_rt`: runs the ailetos executor exclusively.
//! - `cli_rt`: runs all CLI-side async work: notification watcher, join waits, sleeps.
//!
//! The CLI thread itself stays synchronous and drives async work via `block_on`.

pub(crate) mod dbg_actor;
pub(crate) mod dbg_control;
pub(crate) mod query_actor;
pub(crate) mod shell_input_actor;
pub(crate) mod shell_input_control;

mod commands;
mod output;
pub mod shell_ui;

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use ailetos::{Environment, Executor, ExecutorEvent, Handle, KVBuffers, MemKV};

// Re-exports
pub use output::{OutputSink, StdoutSink};
use shell_ui::find_heredoc_marker;

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
        reg.register("query", query_actor::execute);
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
    pub(crate) env: Arc<Environment>,
    pub(crate) kv: Arc<MemKV>,
    pub(crate) handles: Vec<Handle>,
    pub(crate) vars: HashMap<String, Handle>,
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
            handles: Vec::new(),
            vars: HashMap::new(),
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

    fn parse_handle(&self, s: &str) -> Option<Handle> {
        if let Some(var_name) = s.strip_prefix('$') {
            return self.vars.get(var_name).copied();
        }
        s.parse::<i64>().ok().map(Handle::new)
    }

    /// # Errors
    /// Returns an error string if the command fails.
    pub fn execute(&mut self, input: &str) -> Result<ShellControl, String> {
        let mut lines = input.lines().peekable();
        let line = match lines.next() {
            None => return Ok(ShellControl::Continue),
            Some(l) => l.trim(),
        };
        let mut parts: Vec<&str> = line.split_whitespace().collect();
        let body;
        if let Some((idx, delim)) = find_heredoc_marker(&parts) {
            let mut collected = String::new();
            let mut closed = false;
            for body_line in lines.by_ref() {
                if body_line.trim() == delim {
                    closed = true;
                    break;
                }
                if !collected.is_empty() {
                    collected.push('\n');
                }
                collected.push_str(body_line);
            }
            if !closed {
                return Err(format!("heredoc <<{delim} has no closing line"));
            }
            body = collected;
            #[allow(clippy::indexing_slicing)]
            // idx comes from find_heredoc_marker which scans parts
            {
                parts[idx] = body.as_str();
            }
        }
        self.execute_parts(&parts)
    }

    pub(crate) fn execute_lines<'a>(
        &mut self,
        lines: impl Iterator<Item = &'a str>,
    ) -> Result<ShellControl, String> {
        let mut lines = lines.peekable();
        while let Some(line) = lines.next() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts: Vec<&str> = line.split_whitespace().collect();
            let body;
            if let Some((idx, delim)) = find_heredoc_marker(&parts) {
                let mut collected = String::new();
                let mut closed = false;
                for body_line in lines.by_ref() {
                    if body_line.trim() == delim {
                        closed = true;
                        break;
                    }
                    if !collected.is_empty() {
                        collected.push('\n');
                    }
                    collected.push_str(body_line);
                }
                if !closed {
                    self.sink
                        .println(&format!("Error: heredoc <<{delim} has no closing line"));
                    continue;
                }
                body = collected;
                #[allow(clippy::indexing_slicing)]
                // idx comes from find_heredoc_marker which scans parts
                {
                    parts[idx] = body.as_str();
                }
            }
            match self.execute_parts(&parts) {
                Ok(ShellControl::Continue) => {}
                Ok(ShellControl::Exit) => return Ok(ShellControl::Exit),
                Err(e) => return Err(e),
            }
        }
        Ok(ShellControl::Continue)
    }

    /// Like `execute`, but takes already-tokenized arguments. Used by
    /// `cmd_source` to run commands whose arguments were assembled from a
    /// heredoc body (which may contain whitespace `split_whitespace` would
    /// otherwise break apart).
    ///
    /// # Errors
    /// Returns an error string if the command fails.
    pub(crate) fn execute_parts(&mut self, parts: &[&str]) -> Result<ShellControl, String> {
        let (cmd, rest) = match parts.split_first() {
            None => return Ok(ShellControl::Continue),
            Some((cmd, rest)) => (*cmd, rest),
        };

        match cmd {
            "quit" | "exit" | "q" => {
                return Ok(ShellControl::Exit);
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
            "status" => self.cmd_status(rest),
            "source" | "load" => return self.cmd_source(rest),
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

        Ok(ShellControl::Continue)
    }

    fn prepare_exit(&mut self) {
        shell_input_control::close_all_shell_inputs();
        for &handle in &self.handles {
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
