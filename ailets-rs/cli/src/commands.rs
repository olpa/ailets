//! Command implementations for `DagShell`.

use std::sync::Arc;

use actor_runtime::StdHandle;
use ailetos::{
    pipe::{pipe_path, LatentState, PipeEntryInspection},
    DependsOn, For, Handle, KVBuffers, NodeKind, NodeState, OpenMode, StopConditions, TopologicalOrderIter,
};

use crate::output::{parse_color, OutputSinkWriter};
use crate::shell_ui::{
    format_state, parse_bytes_before_pause, parse_explain, parse_quoted_string, truncate,
};
use crate::{dbg_control, shell_input_control, DagShell};

const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(10);

// ---------------------------------------------------------------------------
// Command metadata — descriptions live adjacent to their implementations.
// ---------------------------------------------------------------------------

pub struct CommandMeta {
    /// Primary name first; the rest are aliases shown in help and for completion.
    pub names: &'static [&'static str],
    /// Argument signature — what follows the command name (matches `check_args` argsig).
    pub argsig: &'static str,
    pub section: &'static str,
    pub description: &'static str,
    /// Optional pre-formatted detail lines (sub-commands or flags), indented 4 spaces,
    /// descriptions aligned at column 36. No trailing newline on last line.
    pub detail: Option<&'static str>,
}

pub static SECTIONS: &[&str] = &[
    "Node Management",
    "Dependencies",
    "Visualization",
    "Execution",
    "Job Control",
    "I/O",
    "Status",
    "Debug",
    "Shell Input",
    "Session",
];

// ---------------------------------------------------------------------------
// Session — help and source/load
// ---------------------------------------------------------------------------

pub static ENTRY_HELP: CommandMeta = CommandMeta {
    names: &["help", "?"],
    argsig: "",
    section: "Session",
    description: "Show this help",
    detail: None,
};
impl DagShell {
    pub(crate) fn cmd_help(&self) {
        self.sink.println(&generate_help());
    }
}

pub static ENTRY_SOURCE: CommandMeta = CommandMeta {
    names: &["source", "load"],
    argsig: "file",
    section: "Session",
    description: "Run TCL script file",
    detail: None,
};
impl DagShell {
    /// # Errors
    /// Returns an error string if the file cannot be read or a command fails.
    pub fn cmd_source(
        &mut self,
        interp: &mut molt::Interp,
        ctx: molt::types::ContextID,
        args: &[&str],
    ) -> Result<crate::ShellControl, String> {
        let path = args.first().ok_or("Usage: source <file>")?;
        let content =
            std::fs::read_to_string(path).map_err(|e| format!("Failed to read {path}: {e}"))?;
        self.execute(interp, ctx, &content)
    }
}

pub static ENTRY_QUIT: CommandMeta = CommandMeta {
    names: &["quit", "exit", "q"],
    argsig: "",
    section: "Session",
    description: "Exit the shell",
    detail: None,
};
// quit has no cmd_ implementation — it is handled entirely in tcl_interp.rs.

// ---------------------------------------------------------------------------
// Node Management
// ---------------------------------------------------------------------------

pub static ENTRY_NODE: CommandMeta = CommandMeta {
    names: &["node"],
    argsig: "<actor> [--explain=text]",
    section: "Node Management",
    description: "Add actor node (actors: cat, dbg, shell_input)",
    detail: None,
};
impl DagShell {
    pub(crate) fn cmd_node(&mut self, args: &[&str]) -> Result<Handle, String> {
        let actor = args.first().ok_or("Usage: node <actor> [--explain=text]")?;
        let actor = (*actor).to_string();
        let rest = args.get(1..).unwrap_or_default();
        let explain = parse_explain(rest);
        let handle = self.env.add_node(actor.clone(), &[], explain.clone());

        if actor == "dbg" {
            let bytes_before_pause = parse_bytes_before_pause(rest);
            dbg_control::register_dbg_actor(handle, bytes_before_pause);
        }
        if actor == "shell_input" {
            shell_input_control::register_shell_input_actor(handle);
        }

        let id = handle.id();
        let expl = explain.map_or_else(String::new, |e| format!("({e})"));
        self.sink
            .println(&format!("Added node {id}: {actor} {expl}"));
        Ok(handle)
    }
}

pub static ENTRY_VALUE: CommandMeta = CommandMeta {
    names: &["value"],
    argsig: "<data> [--explain=text]",
    section: "Node Management",
    description: "Add value node (constant data)",
    detail: None,
};
impl DagShell {
    pub(crate) fn cmd_value(&mut self, args: &[&str]) -> Result<Handle, String> {
        if args.is_empty() {
            return Err("Usage: value <data> [--explain=text]".to_string());
        }
        let data = parse_quoted_string(args);
        let explain = parse_explain(args);
        let env = Arc::clone(&self.env);
        let data_bytes = data.as_bytes().to_vec();
        let explain_clone = explain.clone();
        let handle = self
            .ailetos_async_rt
            .block_on(async move { env.add_value_node(data_bytes, explain_clone).await })
            .map_err(|e| format!("Failed to add value node: {e}"))?;
        let id = handle.id();
        let truncated = truncate(&data, 30);
        let expl = explain.map_or_else(String::new, |e| format!("({e})"));
        self.sink
            .println(&format!("Added value node {id}: \"{truncated}\" {expl}"));
        Ok(handle)
    }
}

pub static ENTRY_ALIAS: CommandMeta = CommandMeta {
    names: &["alias"],
    argsig: "<name> <target> ...",
    section: "Node Management",
    description: "Add alias node (one or more targets)",
    detail: None,
};
impl DagShell {
    pub(crate) fn cmd_alias(&mut self, args: &[&str]) -> Result<Handle, String> {
        let (name, targets_strs) = match args {
            [name, rest @ ..] if !rest.is_empty() => (*name, rest),
            _ => return Err("Usage: alias <name> <target> [<target>...]".to_string()),
        };
        let name = name.to_string();
        let mut targets = Vec::new();
        for target_str in targets_strs {
            let target = self
                .parse_handle(target_str)
                .ok_or_else(|| format!("Invalid handle: {target_str}"))?;
            targets.push(target);
        }
        let handle = self.env.add_aliases(name.clone(), &targets);
        let id = handle.id();
        let tids: Vec<_> = targets.iter().map(|t| t.id().to_string()).collect();
        self.sink
            .println(&format!("Added alias {id}: {name} -> {}", tids.join(", ")));
        Ok(handle)
    }
}

pub static ENTRY_NODES: CommandMeta = CommandMeta {
    names: &["nodes"],
    argsig: "",
    section: "Node Management",
    description: "List all nodes with status",
    detail: None,
};
impl DagShell {
    pub(crate) fn cmd_nodes(&self) {
        let dag = self.env.dag.read();
        let mut found = false;
        for node in dag.nodes() {
            found = true;
            let state_str = format_state(node.state);
            let explain = node
                .explain
                .as_ref()
                .map_or_else(String::new, |e| format!(" # {e}"));
            let pid = node.pid.id();
            let suffix = if node.kind == NodeKind::Alias {
                format!(" (alias){explain}")
            } else {
                format!(" [{state_str}]{explain}")
            };
            self.sink.println(&format!("  {pid} {}{suffix}", node.idname));
        }
        if !found {
            self.sink.println("No nodes");
        }
    }
}

// ---------------------------------------------------------------------------
// Dependencies
// ---------------------------------------------------------------------------

pub static ENTRY_DEP: CommandMeta = CommandMeta {
    names: &["dep"],
    argsig: "node dependency",
    section: "Dependencies",
    description: "Add dependency (node depends on dependency)",
    detail: None,
};
impl DagShell {
    pub(crate) fn cmd_dep(&mut self, args: &[&str]) -> Result<(), String> {
        let (node_str, dep_str) = match args {
            [n, d, ..] => (*n, *d),
            _ => return Err("Usage: dep <node> <dependency>".to_string()),
        };
        let node = self
            .parse_handle(node_str)
            .ok_or_else(|| format!("Invalid handle: {node_str}"))?;
        let dep = self
            .parse_handle(dep_str)
            .ok_or_else(|| format!("Invalid handle: {dep_str}"))?;
        self.env
            .dag
            .write()
            .add_dependency(For(node), DependsOn(dep));
        let nid = node.id();
        let did = dep.id();
        self.sink
            .println(&format!("Added dependency: {nid} depends on {did}"));
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Visualization
// ---------------------------------------------------------------------------

pub static ENTRY_SHOW: CommandMeta = CommandMeta {
    names: &["show"],
    argsig: "?node?",
    section: "Visualization",
    description: "Tree view (default: whole DAG)",
    detail: None,
};
impl DagShell {
    fn format_alias_summary(dag: &ailetos::Dag, handle: Handle) -> Option<String> {
        let node = dag.get_node(handle).filter(|n| n.kind == NodeKind::Alias)?;
        let targets: Vec<String> = dag
            .resolve_dependencies(node.pid)
            .map(|h| {
                dag.get_node(h)
                    .map_or_else(|| format!("#{}", h.id()), |n| format!("{}.{}", n.idname, h.id()))
            })
            .collect();
        Some(format!(
            "alias {}.{} → {}",
            node.idname,
            node.pid.id(),
            targets.join(", ")
        ))
    }

    pub(crate) fn cmd_show(&self, args: &[&str]) -> Result<(), String> {
        // snapshot_pending() before dag.read() to avoid lock-order deadlock:
        // executor holds pending.lock() then acquires dag.write(); we must not
        // hold dag.read() while waiting for pending.lock().
        let pending = self.executor.snapshot_pending();
        let dag = self.env.dag.read();
        if args.is_empty() {
            let all_handles: Vec<Handle> = dag.nodes().map(|n| n.pid).collect();
            if all_handles.is_empty() {
                self.sink.println("No nodes");
                return Ok(());
            }

            for node in dag.nodes().filter(|n| n.kind == NodeKind::Alias) {
                if let Some(summary) = Self::format_alias_summary(&dag, node.pid) {
                    self.sink.println(&summary);
                }
            }

            let terminals: Vec<Handle> = all_handles
                .iter()
                .filter(|&&h| dag.get_direct_dependents(h).next().is_none())
                .copied()
                .collect();

            let suspension = Some(&*self.env.suspension);
            let roots = if terminals.is_empty() {
                all_handles
            } else {
                terminals
            };
            for handle in roots {
                let tree = dag.dump_colored(handle, suspension, Some(&pending));
                for line in tree.lines() {
                    self.sink.println(line);
                }
            }
            return Ok(());
        }
        let handle_str = args.first().ok_or("Usage: show <node>")?;
        let handle = self
            .parse_handle(handle_str)
            .ok_or_else(|| format!("Invalid handle: {handle_str}"))?;
        if let Some(summary) = Self::format_alias_summary(&dag, handle) {
            self.sink.println(&summary);
        }
        let suspension = Some(&*self.env.suspension);
        let tree = dag.dump_colored(handle, suspension, Some(&pending));
        for line in tree.lines() {
            self.sink.println(line);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Execution
// ---------------------------------------------------------------------------

pub static ENTRY_RUN: CommandMeta = CommandMeta {
    names: &["run"],
    argsig: "?options? ?node?",
    section: "Execution",
    description: "Submit run to ailetos; waits by default",
    detail: Some(concat!(
        "    --one-step                      Execute only the first ready node\n",
        "    --stop-before <node>            Stop before executing this node\n",
        "    --stop-after <node>             Stop after executing this node\n",
        "    --bg                            Submit and return immediately (background)\n",
        "    --color <name>                  Colorize output (CSS/X11 name or 0-255; --bg only)",
    )),
};
impl DagShell {
    pub(crate) fn cmd_run(&mut self, args: &[&str]) -> Result<(), String> {
        let mut one_step = false;
        let mut stop_before: Option<Handle> = None;
        let mut stop_after: Option<Handle> = None;
        let mut target_arg: Option<&str> = None;
        let mut bg_flag = false;
        let mut color: Option<u8> = None;

        let mut i = 0;
        while i < args.len() {
            let arg = args.get(i).ok_or("Internal error: index out of bounds")?;
            match arg {
                &"--one-step" => one_step = true,
                &"--bg" => bg_flag = true,
                &"--color" => {
                    i += 1;
                    let name = args.get(i).ok_or("--color requires a color name")?;
                    color = Some(parse_color(name)?);
                }
                &"--stop-before" => {
                    i += 1;
                    let h = args.get(i).ok_or("--stop-before requires a node")?;
                    stop_before = Some(
                        self.parse_handle(h)
                            .ok_or_else(|| format!("Invalid handle: {h}"))?,
                    );
                }
                &"--stop-after" => {
                    i += 1;
                    let h = args.get(i).ok_or("--stop-after requires a node")?;
                    stop_after = Some(
                        self.parse_handle(h)
                            .ok_or_else(|| format!("Invalid handle: {h}"))?,
                    );
                }
                arg if !arg.starts_with("--") => {
                    target_arg = Some(arg);
                }
                other => return Err(format!("Unknown option: {other}")),
            }
            i += 1;
        }

        let handle = if let Some(h) = target_arg {
            self.parse_handle(h)
                .ok_or_else(|| format!("Invalid handle: {h}"))?
        } else if let Some(sb) = stop_before {
            sb
        } else {
            self.find_default_target()?
        };
        let stop_conditions = StopConditions {
            one_step,
            stop_before,
            stop_after,
        };

        // Determine the node to join on before submitting the job.  The
        // one_step branch looks at NodeState::NotStarted; if we computed this
        // after submit the executor could race ahead and mark the first node
        // Running/Terminated, causing find() to land on the wrong node.
        let wait_targets = if bg_flag {
            vec![]
        } else if one_step {
            let dag = self.env.dag.read();
            TopologicalOrderIter::new(&dag, handle)
                .find(|&n| {
                    dag.get_node(n)
                        .is_some_and(|nd| nd.state == NodeState::NotStarted)
                })
                .map(|n| vec![n])
                .unwrap_or_default()
        } else if stop_before.is_some() || stop_after.is_some() {
            let dag = self.env.dag.read();
            TopologicalOrderIter::with_stop_conditions(&dag, handle, stop_conditions.clone())
                .last()
                .map(|n| vec![n])
                .unwrap_or_default()
        } else {
            self.env.resolve_all(handle)
        };

        // For foreground runs, raise the flag BEFORE submit so no NodeTerminated
        // event can reach the watcher while the flag is still false. The Release
        // store pairs with the Acquire load in the watcher task.
        if !bg_flag {
            self.foreground_join
                .store(true, std::sync::atomic::Ordering::Release);
        }

        self.executor
            .submit(handle, stop_conditions.clone())
            .map_err(|_| "Executor has shut down".to_string())?;

        if bg_flag {
            self.attach_stdout_for_run(handle, one_step, stop_before, stop_after, true, color);
        } else {
            self.attach_stdout_for_run(handle, one_step, stop_before, stop_after, false, color);
            self.join_handles(&wait_targets)?;
            self.sink.println("");
        }
        Ok(())
    }

    fn join_handles(&mut self, targets: &[Handle]) -> Result<(), String> {
        let receivers: Vec<_> = targets.iter().map(|&t| self.executor.join(t)).collect();
        let sink = &self.sink;
        let foreground_join = &self.foreground_join;
        self.cli_rt.block_on(async move {
            let wait_all =
                futures::future::join_all(receivers.into_iter().map(|rx| async move {
                    rx.await.map_err(|_| "join sender dropped".to_string())?
                }));
            let result = tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    sink.println("\n^C - Detached (node continues running in ailetos)");
                    Ok(())
                }
                results = wait_all => {
                    results.into_iter().find(Result::is_err).unwrap_or(Ok(()))
                }
            };
            foreground_join.store(false, std::sync::atomic::Ordering::Release);
            result
        })
    }

    pub(crate) fn find_default_target(&self) -> Result<Handle, String> {
        let dag = self.env.dag.read();
        let all_handles: Vec<Handle> = dag.nodes().map(|n| n.pid).collect();
        if all_handles.is_empty() {
            return Err("No nodes to run".to_string());
        }
        let terminals: Vec<Handle> = all_handles
            .iter()
            .filter(|&&h| dag.get_direct_dependents(h).next().is_none())
            .copied()
            .collect();
        match terminals.as_slice() {
            [] => Err("No terminal nodes found (circular dependencies?)".to_string()),
            [single] => Ok(*single),
            _ => {
                let ids: Vec<_> = terminals.iter().map(|h| h.id().to_string()).collect();
                Err(format!(
                    "Multiple terminal nodes: {}. Specify target explicitly.",
                    ids.join(", ")
                ))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Job Control
// ---------------------------------------------------------------------------

pub static ENTRY_JOIN: CommandMeta = CommandMeta {
    names: &["join"],
    argsig: "node",
    section: "Job Control",
    description: "Wait for node to terminate; Ctrl+C to detach",
    detail: None,
};
impl DagShell {
    pub(crate) fn cmd_join(&mut self, args: &[&str]) -> Result<(), String> {
        let handle_str = args.first().ok_or("Usage: join <node>")?;
        let handle = self
            .parse_handle(handle_str)
            .ok_or_else(|| format!("Invalid handle: {handle_str}"))?;
        let handles = self.env.resolve_all(handle);
        self.join_handles(&handles)
    }
}

pub static ENTRY_FOLLOW: CommandMeta = CommandMeta {
    names: &["follow"],
    argsig: "node ?--color name?",
    section: "Job Control",
    description: "Attach node stdout; optional 256-color name or 0-255",
    detail: None,
};
impl DagShell {
    pub(crate) fn cmd_follow(&mut self, args: &[&str]) -> Result<(), String> {
        let mut handle_str: Option<&str> = None;
        let mut color: Option<u8> = None;

        let mut i = 0;
        while i < args.len() {
            let Some(arg) = args.get(i).copied() else {
                break;
            };
            if arg == "--color" {
                i += 1;
                let name = args.get(i).ok_or("--color requires a color name")?;
                color = Some(parse_color(name)?);
            } else if arg.starts_with("--") {
                return Err(format!("Unknown option: {arg}"));
            } else if handle_str.is_none() {
                handle_str = Some(arg);
            } else {
                color = Some(parse_color(arg)?);
            }
            i += 1;
        }

        let handle_str = handle_str.ok_or("Usage: follow <node> [--color <name>]")?;
        let handle = self
            .parse_handle(handle_str)
            .ok_or_else(|| format!("Invalid handle: {handle_str}"))?;

        for target in self.env.resolve_all(handle) {
            if self.is_terminated_without_stdout(target) {
                continue;
            }
            let writer = OutputSinkWriter::new(Arc::clone(&self.notification_sink), color);
            let future = self.env.pipe_pool.reader_future(
                &self.env.idgen,
                (target, StdHandle::Stdout as isize),
                writer,
            );
            self.reader_tasks
                .spawn_on(future, self.ailetos_async_rt.handle());
        }

        Ok(())
    }

    pub(crate) fn attach_one_node(&mut self, handle: Handle, bg: bool, color: Option<u8>) {
        if self.is_terminated_without_stdout(handle) {
            return;
        }
        let writer: Box<dyn std::io::Write + Send + Sync> = if bg {
            Box::new(OutputSinkWriter::new(
                Arc::clone(&self.notification_sink),
                color,
            ))
        } else {
            Box::new(std::io::stdout())
        };
        let future = self.env.pipe_pool.reader_future(
            &self.env.idgen,
            (handle, StdHandle::Stdout as isize),
            writer,
        );
        self.reader_tasks
            .spawn_on(future, self.ailetos_async_rt.handle());
    }

    fn is_terminated_without_stdout(&self, handle: Handle) -> bool {
        let has_stdout = self
            .env
            .pipe_pool
            .get_already_realized_writer((handle, StdHandle::Stdout as isize))
            .is_some();
        if has_stdout {
            return false;
        }
        let dag = self.env.dag.read();
        dag.get_node(handle)
            .is_some_and(|n| n.state == NodeState::Terminated)
    }

    pub(crate) fn attach_stdout_for_run(
        &mut self,
        target: Handle,
        one_step: bool,
        stop_before: Option<Handle>,
        stop_after: Option<Handle>,
        bg: bool,
        color: Option<u8>,
    ) {
        if let Some(stop_after_handle) = stop_after {
            for concrete in self.env.resolve_all(stop_after_handle) {
                self.attach_one_node(concrete, bg, color);
            }
        } else if let Some(stop_before_handle) = stop_before {
            let deps: Vec<Handle> = {
                let dag = self.env.dag.read();
                dag.get_direct_dependencies(stop_before_handle).collect()
            };
            for dep in deps {
                for concrete in self.env.resolve_all(dep) {
                    self.attach_one_node(concrete, bg, color);
                }
            }
        } else if one_step {
            let ready_node = {
                let dag = self.env.dag.read();
                TopologicalOrderIter::new(&dag, target).find(|&n| {
                    dag.get_node(n)
                        .is_some_and(|node| node.state == NodeState::NotStarted)
                })
            };
            if let Some(ready_node) = ready_node {
                self.attach_one_node(ready_node, bg, color);
            } else {
                self.sink.println("All nodes already completed");
            }
        } else {
            for concrete in self.env.resolve_all(target) {
                self.attach_one_node(concrete, bg, color);
            }
        }
    }
}

pub static ENTRY_KILL: CommandMeta = CommandMeta {
    names: &["kill"],
    argsig: "?-N? node",
    section: "Job Control",
    description: "Kill actor with exit code N (default 130)",
    detail: None,
};
impl DagShell {
    pub(crate) fn cmd_kill(&mut self, args: &[&str]) -> Result<(), String> {
        let handle_str = match args {
            [flag, node] if flag.starts_with('-') => *node,
            [node] => *node,
            _ => return Err("Usage: kill [-N] <node>".to_string()),
        };

        let handle = self
            .parse_handle(handle_str)
            .ok_or_else(|| format!("Invalid handle: {handle_str}"))?;

        if !dbg_control::is_dbg_node(handle) {
            return Err("kill is only supported for dbg nodes".to_string());
        }

        dbg_control::kill_dbg_actor(handle);
        self.env.suspension.resume(handle);

        self.sink.println(&format!("Killed node {}", handle.id()));
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// I/O
// ---------------------------------------------------------------------------

pub static ENTRY_CAT: CommandMeta = CommandMeta {
    names: &["cat"],
    argsig: "node[:stream]",
    section: "I/O",
    description: "Show output of a node (default stream: stdout)",
    detail: None,
};
impl DagShell {
    fn parse_stream(s: &str) -> Result<isize, String> {
        if let Ok(h) = StdHandle::try_from(s) {
            return Ok(h as isize);
        }
        s.parse().map_err(|_| format!("Unknown stream: {s}"))
    }

    pub(crate) fn cmd_cat(&self, args: &[&str]) -> Result<(), String> {
        let arg = args.first().ok_or("Usage: cat <node>[:<stream>]")?;
        let (node_str, stream_fd) = if let Some((node_part, stream_part)) = arg.split_once(':') {
            (node_part, Self::parse_stream(stream_part)?)
        } else {
            (*arg, StdHandle::Stdout as isize)
        };
        let stream_name = StdHandle::try_from(stream_fd)
            .map_or_else(|()| format!("fd={stream_fd}"), |h| format!("{h:?}"));
        let handle = self
            .parse_handle(node_str)
            .ok_or_else(|| format!("Invalid handle: {node_str}"))?;

        for target in self.env.resolve_all(handle) {
            let node_state = self.env.dag.read().get_node(target).map(|n| n.state);
            let kv = Arc::clone(&self.kv);
            let stream_name = stream_name.clone();
            let output = self.ailetos_async_rt.block_on(async move {
                let path = pipe_path(target, stream_fd);
                match kv.open(&path, OpenMode::Read).await {
                    Ok(buffer) => {
                        let guard = buffer.lock();
                        Ok(String::from_utf8_lossy(&guard).into_owned())
                    }
                    Err(_) => Err(match node_state {
                        Some(NodeState::NotStarted) => {
                            format!("Node {} was never executed", target.id())
                        }
                        Some(NodeState::Running | NodeState::Terminating) => {
                            format!(
                                "Node {} is running but hasn't created stream {stream_name} yet",
                                target.id()
                            )
                        }
                        Some(NodeState::Terminated) | None => {
                            format!("No output on stream {stream_name} for node {}", target.id())
                        }
                    }),
                }
            });
            match output {
                Ok(text) => self.sink.println(&text),
                Err(e) => self.sink.println(&e),
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------

pub static ENTRY_STATUS: CommandMeta = CommandMeta {
    names: &["status"],
    argsig: "?node?",
    section: "Status",
    description: "Overall DAG status, or status of a specific node",
    detail: None,
};
impl DagShell {
    pub(crate) fn cmd_status(&self, args: &[&str]) {
        if args.is_empty() {
            self.cmd_status_dag();
            return;
        }
        let [handle_str, ..] = args else {
            return;
        };
        let Some(handle) = self.parse_handle(handle_str) else {
            self.sink.println(&format!("Invalid handle: {handle_str}"));
            return;
        };
        if matches!(self.cmd_status_node(handle), Found::No)
            && matches!(self.cmd_status_writer(handle), Found::No)
        {
            self.sink
                .println(&format!("Handle {} not found", handle.id()));
        }
    }

    fn cmd_status_dag(&self) {
        // snapshot_pending() before dag.read() — see cmd_show for the rationale.
        let pending_set = self.executor.snapshot_pending();
        let dag = self.env.dag.read();
        let mut total = 0;
        let mut running = 0;
        let mut terminated = 0;
        let mut pending = 0;
        let mut suspended = 0;

        for node in dag.nodes() {
            let handle = node.pid;
            total += 1;
            match node.state {
                NodeState::Running => running += 1,
                NodeState::Terminated => terminated += 1,
                NodeState::NotStarted => {
                    if pending_set.contains(&handle) {
                        pending += 1;
                    }
                }
                NodeState::Terminating => {}
            }
            if self.env.suspension.is_suspended(handle) {
                suspended += 1;
            }
        }
        self.sink.println(&format!("Nodes: {total} total, {pending} pending, {running} running, {suspended} suspended, {terminated} terminated"));
    }

    fn cmd_status_node(&self, handle: Handle) -> Found {
        let hid = handle.id();
        let dag = self.env.dag.read();
        let Some(node) = dag.get_node(handle) else {
            return Found::No;
        };
        let state = format_state(node.state);
        let node_line = format!("Node {hid}: {} [{state}]", node.idname);
        self.sink.println(&node_line);

        // in pipes: mirrors MergeReader alias resolution via resolve_dependencies
        for dep in dag.resolve_dependencies(handle) {
            let inspection = self
                .env
                .pipe_pool
                .inspect_entry((dep, StdHandle::Stdout as isize));
            let pipe_info: String = if let Some(insp) = &inspection {
                format_pipe_inspection(insp)
            } else {
                let kv = Arc::clone(&self.env.kv);
                let path = pipe_path(dep, StdHandle::Stdout as isize);
                let in_kv = self
                    .ailetos_async_rt
                    .block_on(async move { kv.stat(&path).await.is_ok() });
                if in_kv {
                    "kv, closed".to_string()
                } else {
                    "not created".to_string()
                }
            };
            self.sink.println(&format!(
                "  fd={}  in   actor={}, fd={}  {}",
                StdHandle::Stdin as isize,
                dep.id(),
                StdHandle::Stdout as isize,
                pipe_info,
            ));
        }

        // out pipes: all pool entries owned by this node, sorted by fd
        let mut out_pipes: Vec<_> = self
            .env
            .pipe_pool
            .inspect_entries()
            .into_iter()
            .filter(|(actor, _, _)| *actor == handle)
            .map(|(_, fd, insp)| (fd, insp))
            .collect();
        out_pipes.sort_by_key(|(fd, _)| *fd);
        for (fd, inspection) in &out_pipes {
            self.sink.println(&format!(
                "  fd={}  out  {}",
                fd,
                format_pipe_inspection(inspection),
            ));
        }
        Found::Yes
    }

    fn cmd_status_writer(&self, handle: Handle) -> Found {
        let hid = handle.id();
        let entry = self
            .env
            .pipe_pool
            .inspect_entries()
            .into_iter()
            .find(|(_, _, insp)| matches!(insp, PipeEntryInspection::Realized { handle: h, .. } if *h == handle));
        let Some((actor, fd, inspection)) = entry else {
            return Found::No;
        };
        self.sink.println(&format!(
            "Writer {hid}: fd={}  {}",
            fd,
            format_pipe_inspection(&inspection),
        ));
        let dag = self.env.dag.read();
        if let Some(node) = dag.get_node(actor) {
            self.sink
                .println(&format!("  source: node {}  {}", actor.id(), node.idname));
        }
        let dependents: Vec<_> = dag.get_direct_dependents(actor).collect();
        for dep in dependents {
            if let Some(node) = dag.get_node(dep) {
                self.sink
                    .println(&format!("  target: node {}  {}", dep.id(), node.idname));
            }
        }
        Found::Yes
    }
}

// ---------------------------------------------------------------------------
// Debug
// ---------------------------------------------------------------------------

pub static ENTRY_SUSPEND: CommandMeta = CommandMeta {
    names: &["suspend"],
    argsig: "node",
    section: "Debug",
    description: "Suspend a running actor",
    detail: None,
};
impl DagShell {
    pub(crate) fn cmd_suspend(&self, args: &[&str]) -> Result<(), String> {
        let handle_str = args.first().ok_or("Usage: suspend <node>")?;
        let handle = self
            .parse_handle(handle_str)
            .ok_or_else(|| format!("Invalid handle: {handle_str}"))?;
        self.env.suspension.suspend(handle);
        self.sink
            .println(&format!("Suspended node {}", handle.id()));
        Ok(())
    }
}

pub static ENTRY_RESUME: CommandMeta = CommandMeta {
    names: &["resume"],
    argsig: "node",
    section: "Debug",
    description: "Resume a suspended actor (dbg or general)",
    detail: None,
};
impl DagShell {
    pub(crate) fn cmd_resume(&self, args: &[&str]) -> Result<(), String> {
        let handle_str = args.first().ok_or("Usage: resume <node>")?;
        let handle = self
            .parse_handle(handle_str)
            .ok_or_else(|| format!("Invalid handle: {handle_str}"))?;
        self.env.suspension.resume(handle);
        self.sink.println(&format!("Resumed node {}", handle.id()));
        Ok(())
    }
}

pub static ENTRY_WAIT: CommandMeta = CommandMeta {
    names: &["wait"],
    argsig: "condition ?args?",
    section: "Debug",
    description: "Block until condition; Ctrl+C to detach",
    detail: Some(concat!(
        "    suspended <node>                Block until node is suspended\n",
        "    terminated <node>               Block until node is terminated",
    )),
};
impl DagShell {
    #[allow(clippy::disallowed_methods)] // polling loop without a notification channel
    pub(crate) fn cmd_wait(&mut self, args: &[&str]) -> Result<(), String> {
        let condition = args.first().ok_or("Usage: wait <condition> [args]")?;
        match *condition {
            "suspended" => {
                let handle_str = args.get(1).ok_or("Usage: wait suspended <node>")?;
                let handle = self
                    .parse_handle(handle_str)
                    .ok_or_else(|| format!("Invalid handle: {handle_str}"))?;
                let env = &self.env;
                let sink = &self.sink;
                self.cli_rt.block_on(async move {
                    tokio::select! {
                        _ = tokio::signal::ctrl_c() => {
                            sink.println("\n^C - Detached (node continues running in ailetos)");
                        }
                        () = async {
                            loop {
                                if env.suspension.is_suspended(handle) {
                                    break;
                                }
                                tokio::time::sleep(POLL_INTERVAL).await;
                            }
                        } => {}
                    }
                    Ok(())
                })
            }
            "terminated" => {
                let handle_str = args.get(1).ok_or("Usage: wait terminated <node>")?;
                let handle = self
                    .parse_handle(handle_str)
                    .ok_or_else(|| format!("Invalid handle: {handle_str}"))?;
                let env = &self.env;
                let sink = &self.sink;
                self.cli_rt.block_on(async move {
                    tokio::select! {
                        _ = tokio::signal::ctrl_c() => {
                            sink.println("\n^C - Detached (node continues running in ailetos)");
                        }
                        () = async {
                            loop {
                                if matches!(
                                    env.dag.read().get_node(handle).map(|n| n.state),
                                    Some(NodeState::Terminated)
                                ) {
                                    break;
                                }
                                tokio::time::sleep(POLL_INTERVAL).await;
                            }
                        } => {}
                    }
                    Ok(())
                })
            }
            other => Err(format!("Unknown wait condition: {other}")),
        }
    }
}

// ---------------------------------------------------------------------------
// Shell Input
// ---------------------------------------------------------------------------

pub static ENTRY_WRITE: CommandMeta = CommandMeta {
    names: &["write"],
    argsig: "node ?data?",
    section: "Shell Input",
    description: "Write data to a shell_input actor",
    detail: None,
};
impl DagShell {
    pub(crate) fn cmd_write(&self, args: &[&str]) -> Result<(), String> {
        let handle_str = args.first().ok_or("Usage: write <node> <data>")?;
        let handle = self
            .parse_handle(handle_str)
            .ok_or_else(|| format!("Invalid handle: {handle_str}"))?;

        let data = parse_quoted_string(args.get(1..).unwrap_or(&[]));

        match shell_input_control::write_to_shell_input(handle, data.into_bytes()) {
            Ok(()) => {
                let hid = handle.id();
                self.sink.println(&format!("Wrote data to node {hid}"));
                Ok(())
            }
            Err(e) => Err(format!("Failed to write: {e}")),
        }
    }
}

pub static ENTRY_CLOSE: CommandMeta = CommandMeta {
    names: &["close"],
    argsig: "node",
    section: "Shell Input",
    description: "Close a shell_input actor (send EOF)",
    detail: None,
};
impl DagShell {
    pub(crate) fn cmd_close(&self, args: &[&str]) -> Result<(), String> {
        let handle_str = args.first().ok_or("Usage: close <node>")?;
        let handle = self
            .parse_handle(handle_str)
            .ok_or_else(|| format!("Invalid handle: {handle_str}"))?;

        match shell_input_control::close_shell_input(handle) {
            Ok(()) => {
                let hid = handle.id();
                self.sink.println(&format!("Closed node {hid}"));
                Ok(())
            }
            Err(e) => Err(format!("Failed to close: {e}")),
        }
    }
}

// ---------------------------------------------------------------------------
// Command registry and help generation
// ---------------------------------------------------------------------------

pub static COMMANDS: &[&CommandMeta] = &[
    &ENTRY_NODE,
    &ENTRY_VALUE,
    &ENTRY_ALIAS,
    &ENTRY_NODES,
    &ENTRY_DEP,
    &ENTRY_SHOW,
    &ENTRY_RUN,
    &ENTRY_JOIN,
    &ENTRY_FOLLOW,
    &ENTRY_KILL,
    &ENTRY_CAT,
    &ENTRY_STATUS,
    &ENTRY_SUSPEND,
    &ENTRY_RESUME,
    &ENTRY_WAIT,
    &ENTRY_WRITE,
    &ENTRY_CLOSE,
    &ENTRY_SOURCE,
    &ENTRY_HELP,
    &ENTRY_QUIT,
];

pub fn generate_help() -> String {
    const DESC_COL: usize = 38;

    let mut out = String::from("DAG Shell Commands (TCL syntax):\n");

    for &section in SECTIONS {
        out.push('\n');
        out.push_str(section);
        out.push_str(":\n");

        for entry in COMMANDS.iter().filter(|e| e.section == section) {
            let names_display = entry.names.join(" / ");
            let usage = if entry.argsig.is_empty() {
                format!("  {names_display}")
            } else {
                format!("  {names_display} {}", entry.argsig)
            };
            let pad = DESC_COL.saturating_sub(usage.len());
            out.push_str(&usage);
            for _ in 0..pad {
                out.push(' ');
            }
            out.push_str(entry.description);
            out.push('\n');

            if let Some(detail) = entry.detail {
                out.push_str(detail);
                out.push('\n');
            }
        }
    }

    out.push_str("\nVariables (TCL):\n");
    out.push_str("  set v [node ...]                    Assign node handle to variable\n");
    out.push_str("  dep $foo $bar                       TCL expands $var before the command runs");

    out
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

enum Found {
    Yes,
    No,
}

fn format_pipe_inspection(inspection: &PipeEntryInspection) -> String {
    match inspection {
        PipeEntryInspection::Realized {
            is_closed: true,
            handle,
        } => {
            format!("realized, closed, writer_handle={}", handle.id())
        }
        PipeEntryInspection::Realized {
            is_closed: false,
            handle,
        } => {
            format!("realized, open, writer_handle={}", handle.id())
        }
        PipeEntryInspection::Latent(LatentState::Waiting) => "latent, waiting".to_string(),
        PipeEntryInspection::Latent(LatentState::Closed) => "latent, closed".to_string(),
    }
}

// ---------------------------------------------------------------------------
// dag — DAG introspection ensemble
// ---------------------------------------------------------------------------

pub static ENTRY_DAG: CommandMeta = CommandMeta {
    names: &["dag"],
    argsig: "exists|handle <name>",
    section: "Node Management",
    description: "DAG introspection",
    detail: Some(
        "    exists <name>               1 if a node named <name> exists, else 0\n    handle <name>               numeric handle of node named <name>",
    ),
};

impl DagShell {
    /// # Errors
    /// Returns an error for unknown subcommands or if `handle` is called with a name
    /// that does not exist.
    pub(crate) fn cmd_dag(&self, args: &[&str]) -> Result<String, String> {
        match args {
            ["exists", name] => {
                let exists = self.parse_handle(name).is_some();
                Ok(if exists { "1".to_string() } else { "0".to_string() })
            }
            ["handle", name] => {
                let handle = self
                    .parse_handle(name)
                    .ok_or_else(|| format!("no node named '{name}'"))?;
                Ok(handle.id().to_string())
            }
            [sub, ..] => Err(format!(
                "unknown subcommand '{sub}'; expected 'exists' or 'handle'"
            )),
            [] => Err("dag: subcommand required; expected 'exists' or 'handle'".to_string()),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::DagShell;

    fn make_shell() -> DagShell {
        DagShell::new()
    }

    // dag exists returns 0 for a nonexistent name
    #[test]
    fn test_dag_exists_absent() {
        let shell = make_shell();
        assert_eq!(shell.cmd_dag(&["exists", "input"]).unwrap(), "0");
    }

    // dag exists returns 1 after an alias with that name is created
    #[test]
    fn test_dag_exists_present() {
        let mut shell = make_shell();
        shell.cmd_value(&["hello"]).unwrap();
        shell.cmd_alias(&["input", "1"]).unwrap();
        assert_eq!(shell.cmd_dag(&["exists", "input"]).unwrap(), "1");
    }

    // dag handle returns the numeric id
    #[test]
    fn test_dag_handle_present() {
        let mut shell = make_shell();
        let value_handle = shell.cmd_value(&["hello"]).unwrap();
        let alias_handle = shell.cmd_alias(&["input", "1"]).unwrap();
        let id = shell.cmd_dag(&["handle", "input"]).unwrap();
        assert_eq!(id, alias_handle.id().to_string());
        // value node is also findable by handle number string
        let val_id = shell.cmd_dag(&["handle", &value_handle.id().to_string()]).unwrap();
        assert_eq!(val_id, value_handle.id().to_string());
    }

    // dag handle errors for nonexistent name
    #[test]
    fn test_dag_handle_absent() {
        let shell = make_shell();
        assert!(shell.cmd_dag(&["handle", "nosuchnode"]).is_err());
    }

    // unknown subcommand → error
    #[test]
    fn test_dag_unknown_subcommand() {
        let shell = make_shell();
        assert!(shell.cmd_dag(&["bogus", "x"]).is_err());
    }

    // missing subcommand → error
    #[test]
    fn test_dag_no_subcommand() {
        let shell = make_shell();
        assert!(shell.cmd_dag(&[]).is_err());
    }
}
