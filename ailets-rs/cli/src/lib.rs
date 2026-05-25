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

mod ailetos_rt_glue;
mod commands;
mod output;
pub mod shell_ui;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use ailetos::{Environment, Executor, Handle, MemKV};

// Re-exports
pub use output::{OutputSink, StdoutSink};

pub(crate) use ailetos_rt_glue::{
    make_env, start_ctrlc_handler, start_executor_with_bridge, start_notification_watcher,
    JoinWaiter,
};

// ---------------------------------------------------------------------------
// DagShell
// ---------------------------------------------------------------------------

pub struct DagShell {
    env: Arc<Environment>,
    kv: Arc<MemKV>,
    handles: Vec<Handle>,
    vars: HashMap<String, Handle>,
    sink: Box<dyn OutputSink>,
    notification_sink: Arc<dyn OutputSink>,
    pending_join: Arc<Mutex<Option<JoinWaiter>>>,
    _watcher: tokio::task::JoinHandle<()>,
    // Global Ctrl+C handler task; kept alive until DagShell drops.
    _ctrlc_handler: tokio::task::JoinHandle<()>,
    // executor drops before ailetos_async_rt (declaration order = drop order).
    executor: Executor,
    ailetos_async_rt: tokio::runtime::Runtime,
    // CLI-side async runtime: join waits, sleeps. Independent from ailetos.
    cli_rt: tokio::runtime::Runtime,
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
        let (executor, events_rx) = start_executor_with_bridge(ailetos_async_rt.handle().clone(), Arc::clone(&env));

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
