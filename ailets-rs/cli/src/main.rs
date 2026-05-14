//! DAG Shell - Interactive tool to build and run DAGs
//!
//! Minimal implementation for manually building and running DAGs.

mod dbg_actor;
mod dbg_control;
mod shell_input_actor;
mod shell_input_control;

use std::collections::HashMap;
use std::sync::Arc;

use ailetos::{
    Executor, DependsOn, Environment, For, Handle, IoBridge, KVBuffers, MemKV, NodeState,
    OpenMode, StopConditions, TopologicalOrderIter, EOWNERDEAD,
};
use futures::future::Abortable;
use rustyline::config::Configurer;
use rustyline::error::ReadlineError;
use rustyline::Editor;

struct BackgroundJob {
    thread: std::thread::JoinHandle<()>,
    abort_handle: futures::future::AbortHandle,
    bridge: std::sync::Arc<IoBridge>,
    runtime_handle: tokio::runtime::Handle,
}

struct DagShell {
    env: std::sync::Arc<Environment>,
    kv: Arc<MemKV>,
    /// Track all created node handles for listing
    handles: Vec<Handle>,
    /// Variables mapping names to handles
    vars: HashMap<String, Handle>,
    bg_job: Option<BackgroundJob>,
}

impl DagShell {
    fn new() -> Self {
        let kv = Arc::new(MemKV::new());
        let env = Arc::new(Environment::new(Arc::clone(&kv) as Arc<dyn KVBuffers>));
        env.actor_registry.write().register("cat", cat::execute);
        env.actor_registry
            .write()
            .register("dbg", dbg_actor::execute);
        env.actor_registry
            .write()
            .register("shell_input", shell_input_actor::execute);
        Self {
            env,
            kv,
            handles: Vec::new(),
            vars: HashMap::new(),
            bg_job: None,
        }
    }

    fn parse_handle(&self, s: &str) -> Option<Handle> {
        // Check for $var syntax
        if let Some(var_name) = s.strip_prefix('$') {
            return self.vars.get(var_name).copied();
        }
        // Otherwise try as numeric handle
        s.parse::<i64>().ok().map(Handle::new)
    }

    fn execute(&mut self, line: &str) -> Result<bool, String> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        let (cmd, rest) = match parts.split_first() {
            None => return Ok(true),
            Some((cmd, rest)) => (*cmd, rest),
        };

        match cmd {
            "quit" | "exit" | "q" => {
                self.release_background_job();
                return Ok(false);
            }
            "help" | "?" => Self::cmd_help(),
            "set" => self.cmd_set(rest)?,
            "node" => {
                self.cmd_node(rest)?;
            }
            "dep" => self.cmd_dep(rest)?,
            "deps" => self.cmd_deps(rest)?,
            "show" => self.cmd_show(rest)?,
            "run" => self.cmd_run(rest)?,
            "cat" => self.cmd_cat(rest)?,
            "status" => self.cmd_status(rest)?,
            "source" | "load" => self.cmd_source(rest)?,
            "reset" => self.cmd_reset(),
            "suspend" => self.cmd_suspend(rest)?,
            "resume" => self.cmd_resume(rest)?,
            "wait" => self.cmd_wait(rest)?,
            "write" => self.cmd_write(rest)?,
            "close" => self.cmd_close(rest)?,
            "fg" => self.cmd_fg(rest)?,
            "kill" => self.cmd_kill(rest)?,
            _ => {
                println!("Unknown command: {cmd}. Type 'help' for usage.");
            }
        }

        Ok(true)
    }

    fn cmd_help() {
        println!(
            r"DAG Shell Commands:

Node Management:
  node add <actor> [--explain=text]   Add actor node (actors: cat, dbg, shell_input)
  node value <data> [--explain=text]  Add value node (constant data)
  node alias <name> <target>          Add alias node
  node list                           List all nodes with status

Dependencies:
  dep <node> <dependency>             Add dependency (node depends on dependency)
  deps <node>                         Show direct dependencies

Visualization:
  show [node]                         Tree view (default: whole DAG)

Execution:
  run [node] [options]                Run the DAG (default: last node)
    --one-step                        Execute only the first ready node
    --stop-before <node>              Stop before executing this node
    --stop-after <node>               Stop after executing this node
    --bg                              Run in background

Job Control:
  fg                                  Wait for background job to complete
  kill [-N] <node>                    Kill actor with exit code N (default 130)

I/O:
  cat <node>                          Show output of a node

Status:
  status                              Overall DAG status
  status <node>                       Node status

Debug:
  suspend <node>                      Suspend a running actor
  resume <node>                       Resume a suspended actor (dbg or general)
  wait suspended <node>               Block until node is suspended (polls with 10 ms interval, 5 s timeout)
  wait terminated <node>              Block until node is terminated (polls with 10 ms interval, 5 s timeout)

Shell Input:
  write <node> <data>                 Write data to a shell_input actor
  close <node>                        Close a shell_input actor (send EOF)

Session:
  load <file>                         Run script file (alias: source)
  reset                               Clear all nodes and start fresh
  help                                Show this help
  quit                                Exit

Variables:
  set var = node ...                  Assign node to variable
  dep $foo $bar                       Use $var to reference variables"
        );
    }

    fn cmd_set(&mut self, args: &[&str]) -> Result<(), String> {
        // set var = node ...
        match args {
            [var_name, "=", "node", rest @ ..] => {
                let handle = self.cmd_node_inner(rest)?;
                self.vars.insert((*var_name).to_string(), handle);
                Ok(())
            }
            _ => Err("Usage: set <var> = node ...".to_string()),
        }
    }

    fn cmd_node(&mut self, args: &[&str]) -> Result<(), String> {
        if args.first() == Some(&"list") {
            self.cmd_node_list();
        } else {
            self.cmd_node_inner(args)?;
        }
        Ok(())
    }

    fn cmd_node_list(&self) {
        if self.handles.is_empty() {
            println!("No nodes");
        } else {
            let dag = self.env.dag.read();
            for &handle in &self.handles {
                if let Some(node) = dag.get_node(handle) {
                    let state_str = format_state(node.state);
                    let explain = node
                        .explain
                        .as_ref()
                        .map_or_else(String::new, |e| format!(" # {e}"));
                    let pid = node.pid.id();
                    println!("  {pid} {} [{state_str}]{explain}", node.idname);
                }
            }
        }
    }

    fn cmd_node_inner(&mut self, args: &[&str]) -> Result<Handle, String> {
        match args {
            ["add", actor, rest @ ..] => {
                let actor = (*actor).to_string();
                let explain = parse_explain(rest);
                let handle = self.env.add_node(actor.clone(), &[], explain.clone());
                self.handles.push(handle);

                // If this is a dbg actor, register it with configuration
                if actor == "dbg" {
                    let bytes_before_pause = parse_bytes_before_pause(rest);
                    dbg_control::register_dbg_actor(handle, bytes_before_pause);
                }

                // If this is a shell_input actor, register it
                if actor == "shell_input" {
                    shell_input_control::register_shell_input_actor(handle);
                }

                let id = handle.id();
                let expl = explain.map_or_else(String::new, |e| format!("({e})"));
                println!("Added node {id}: {actor} {expl}");
                Ok(handle)
            }
            ["add"] => Err("Usage: node add <actor> [--explain=text]".to_string()),
            ["value", rest @ ..] if !rest.is_empty() => {
                let data = parse_quoted_string(rest);
                let explain = parse_explain(rest);
                let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
                let handle = rt
                    .block_on(
                        self.env
                            .add_value_node(data.as_bytes().to_vec(), explain.clone()),
                    )
                    .map_err(|e| format!("Failed to add value node: {e}"))?;
                self.handles.push(handle);
                let id = handle.id();
                let truncated = truncate(&data, 30);
                let expl = explain.map_or_else(String::new, |e| format!("({e})"));
                println!("Added value node {id}: \"{truncated}\" {expl}");
                Ok(handle)
            }
            ["value"] => Err("Usage: node value <data> [--explain=text]".to_string()),
            ["alias", name, target_str, ..] => {
                let name = (*name).to_string();
                let target = self
                    .parse_handle(target_str)
                    .ok_or_else(|| format!("Invalid handle: {target_str}"))?;
                let handle = self.env.add_alias(name.clone(), target);
                self.handles.push(handle);
                let id = handle.id();
                let tid = target.id();
                println!("Added alias {id}: {name} -> {tid}");
                Ok(handle)
            }
            ["alias", ..] => Err("Usage: node alias <name> <target>".to_string()),
            [cmd, ..] => Err(format!("Unknown node subcommand: {cmd}")),
            [] => Err("Usage: node <add|value|alias|list> ...".to_string()),
        }
    }

    fn cmd_dep(&mut self, args: &[&str]) -> Result<(), String> {
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
        println!("Added dependency: {nid} depends on {did}");
        Ok(())
    }

    fn cmd_deps(&self, args: &[&str]) -> Result<(), String> {
        let handle_str = args.first().ok_or("Usage: deps <node>")?;
        let handle = self
            .parse_handle(handle_str)
            .ok_or_else(|| format!("Invalid handle: {handle_str}"))?;
        let dag = self.env.dag.read();
        let deps: Vec<_> = dag.get_direct_dependencies(handle).collect();
        let hid = handle.id();
        if deps.is_empty() {
            println!("Node {hid} has no dependencies");
        } else {
            println!("Node {hid} depends on:");
            for dep in deps {
                let node = dag.get_node(dep);
                let name = node.map_or("?", |n| n.idname.as_str());
                let did = dep.id();
                println!("  {did} ({name})");
            }
        }
        Ok(())
    }

    fn cmd_show(&self, args: &[&str]) -> Result<(), String> {
        let dag = self.env.dag.read();
        if args.is_empty() {
            // Show whole DAG: find terminal nodes (nodes that nothing depends on)
            if self.handles.is_empty() {
                println!("No nodes");
                return Ok(());
            }
            let terminals: Vec<Handle> = self
                .handles
                .iter()
                .filter(|&&h| dag.get_direct_dependents(h).next().is_none())
                .copied()
                .collect();

            // If no terminals (e.g., due to circular dependencies), show all nodes
            let suspension = Some(&*self.env.suspension);
            if terminals.is_empty() {
                for handle in &self.handles {
                    let tree = dag.dump_colored(*handle, suspension);
                    print!("{tree}");
                }
            } else {
                for handle in terminals {
                    let tree = dag.dump_colored(handle, suspension);
                    print!("{tree}");
                }
            }
            return Ok(());
        }
        let suspension = Some(&*self.env.suspension);
        let handle_str = args.first().ok_or("Usage: show <node>")?;
        let handle = self
            .parse_handle(handle_str)
            .ok_or_else(|| format!("Invalid handle: {handle_str}"))?;
        let tree = dag.dump_colored(handle, suspension);
        print!("{tree}");
        Ok(())
    }

    fn cmd_run(&mut self, args: &[&str]) -> Result<(), String> {
        let mut one_step = false;
        let mut stop_before: Option<Handle> = None;
        let mut stop_after: Option<Handle> = None;
        let mut target_arg: Option<&str> = None;
        let mut bg_flag = false;

        // Parse arguments
        let mut i = 0;
        while i < args.len() {
            let arg = args.get(i).ok_or("Internal error: index out of bounds")?;
            match arg {
                &"--one-step" => one_step = true,
                &"--bg" => bg_flag = true,
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
            // When --stop-before X is specified, use X as target to run all its dependencies
            sb
        } else {
            // Find terminal nodes (nodes that nothing depends on)
            self.find_default_target()?
        };

        // Attach stdout based on stop conditions
        self.attach_stdout_for_run(handle, one_step, stop_before, stop_after);

        let stop_conditions = StopConditions {
            one_step,
            stop_before,
            stop_after,
        };

        if bg_flag {
            // Background run
            self.run_background(handle, stop_conditions)?;
        } else {
            // Foreground run with Ctrl+C support
            self.run_foreground(handle, stop_conditions)?;
        }

        println!();
        Ok(())
    }

    fn run_foreground(
        &mut self,
        handle: Handle,
        stop_conditions: StopConditions,
    ) -> Result<(), String> {
        if self.bg_job.is_some() {
            return Err("Background job already running. Use 'fg' or 'kill' first.".to_string());
        }

        let env = Arc::clone(&self.env);

        let (abort_handle, abort_registration) = futures::future::AbortHandle::new_pair();
        let (ctrlc_abort_handle, ctrlc_abort_reg) = futures::future::AbortHandle::new_pair();
        let (ready_tx, ready_rx) =
            std::sync::mpsc::channel::<Result<(Arc<IoBridge>, tokio::runtime::Handle), String>>();

        let thread = std::thread::spawn(move || {
            tracing::info!("Foreground thread starting");
            let Ok(rt) = tokio::runtime::Runtime::new() else {
                ready_tx
                    .send(Err("Failed to create tokio runtime".to_string()))
                    .ok();
                return;
            };
            let rt_handle = rt.handle().clone();
            rt.block_on(async move {
                // todo: kill command is broken — fix as part of fix-kill-command.md
                let executor = Executor::start(Arc::clone(&env), None);
                let bridge = executor.io_bridge();
                ready_tx.send(Ok((bridge, rt_handle))).ok();
                executor.submit(handle, stop_conditions).ok();
                tracing::info!("About to run environment");
                let result = Abortable::new(executor.shutdown(), abort_registration).await;
                if let Ok(()) = result {
                    tracing::info!("Run completed");
                } else {
                    tracing::info!("Run aborted");
                }
            });
        });

        // Wait for system runtime to be ready before entering the Ctrl+C loop
        let (bridge, runtime_handle) = match ready_rx.recv() {
            Ok(Ok(pair)) => pair,
            Ok(Err(e)) => {
                thread.join().ok();
                return Err(e);
            }
            Err(_) => {
                // Thread finished before relaying bridge (instant job)
                thread.join().ok();
                return Ok(());
            }
        };

        // Create a channel to signal when Ctrl+C is pressed
        let (tx, rx) = std::sync::mpsc::channel();

        // Spawn a thread to wait for Ctrl+C; exits early if ctrlc_abort_reg is fired
        std::thread::spawn(move || {
            let Ok(rt) = tokio::runtime::Runtime::new() else {
                return;
            };
            rt.block_on(async {
                if Abortable::new(tokio::signal::ctrl_c(), ctrlc_abort_reg)
                    .await
                    .is_ok_and(|r| r.is_ok())
                {
                    let _ = tx.send(());
                }
            });
        });

        // Wait for either the job to complete or Ctrl+C
        let mut job = Some(BackgroundJob {
            thread,
            abort_handle,
            bridge,
            runtime_handle,
        });

        loop {
            match rx.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(()) => {
                    // Ctrl+C pressed - move to background
                    println!("\n^C - Moved to background (use 'fg' to wait, 'kill' to terminate)");
                    self.bg_job = job.take();
                    return Ok(());
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    // Check if thread is done
                    if job.as_ref().is_some_and(|j| j.thread.is_finished()) {
                        ctrlc_abort_handle.abort();
                        if let Some(j) = job.take() {
                            j.thread.join().ok();
                        }
                        return Ok(());
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    // Ctrl+C handler exited on its own; wait for job completion
                    if let Some(j) = job.take() {
                        j.thread.join().ok();
                    }
                    return Ok(());
                }
            }
        }
    }

    fn run_background(
        &mut self,
        handle: Handle,
        stop_conditions: StopConditions,
    ) -> Result<(), String> {
        if self.bg_job.is_some() {
            return Err("Background job already running. Use 'fg' or 'kill' first.".to_string());
        }

        let env = Arc::clone(&self.env);

        let (abort_handle, abort_registration) = futures::future::AbortHandle::new_pair();
        let (ready_tx, ready_rx) =
            std::sync::mpsc::channel::<Result<(Arc<IoBridge>, tokio::runtime::Handle), String>>();

        let thread = std::thread::spawn(move || {
            tracing::info!("Background thread starting");
            let Ok(rt) = tokio::runtime::Runtime::new() else {
                ready_tx
                    .send(Err("Failed to create tokio runtime".to_string()))
                    .ok();
                return;
            };
            let rt_handle = rt.handle().clone();
            rt.block_on(async move {
                // todo: kill command is broken — fix as part of fix-kill-command.md
                let executor = Executor::start(Arc::clone(&env), None);
                let bridge = executor.io_bridge();
                ready_tx.send(Ok((bridge, rt_handle))).ok();
                executor.submit(handle, stop_conditions).ok();
                let result = Abortable::new(executor.shutdown(), abort_registration).await;
                if let Ok(()) = result {
                    tracing::info!("Background job completed");
                } else {
                    tracing::info!("Background job aborted");
                }
            });
        });

        let (bridge, runtime_handle) = match ready_rx.recv() {
            Ok(Ok(pair)) => pair,
            Ok(Err(e)) => {
                thread.join().ok();
                return Err(e);
            }
            Err(_) => {
                thread.join().ok();
                return Err("Background thread exited before signalling ready".to_string());
            }
        };

        self.bg_job = Some(BackgroundJob {
            thread,
            abort_handle,
            bridge,
            runtime_handle,
        });

        println!("Started background run (use 'fg' to wait, 'kill' to terminate)");

        Ok(())
    }

    fn attach_stdout_for_run(
        &mut self,
        target: Handle,
        one_step: bool,
        stop_before: Option<Handle>,
        stop_after: Option<Handle>,
    ) {
        if let Some(stop_after_handle) = stop_after {
            // --stop-after X: attach stdout to X
            let resolved = self.env.resolve(stop_after_handle);
            self.env.attach_stdout(resolved);
        } else if let Some(stop_before_handle) = stop_before {
            // --stop-before X: attach stdout to all direct dependencies of X
            let deps: Vec<Handle> = {
                let dag = self.env.dag.read();
                dag.get_direct_dependencies(stop_before_handle).collect()
            };
            for dep in deps {
                let resolved = self.env.resolve(dep);
                self.env.attach_stdout(resolved);
            }
        } else if one_step {
            // --one-step: find first ready node and attach stdout to it
            let ready_node = {
                let dag = self.env.dag.read();
                let first = TopologicalOrderIter::new(&dag, target).next();
                first
            };
            if let Some(ready_node) = ready_node {
                let resolved = self.env.resolve(ready_node);
                self.env.attach_stdout(resolved);
            }
        } else {
            // Normal run: attach to target
            let resolved = self.env.resolve(target);
            self.env.attach_stdout(resolved);
        }
    }

    fn find_default_target(&self) -> Result<Handle, String> {
        if self.handles.is_empty() {
            return Err("No nodes to run".to_string());
        }
        let dag = self.env.dag.read();
        let terminals: Vec<Handle> = self
            .handles
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

    fn cmd_cat(&self, args: &[&str]) -> Result<(), String> {
        let handle_str = args.first().ok_or("Usage: cat <node>")?;
        let handle = self
            .parse_handle(handle_str)
            .ok_or_else(|| format!("Invalid handle: {handle_str}"))?;

        let hid = handle.id();
        let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
        rt.block_on(async {
            let path = format!("{hid}/stdout");
            match self.kv.open(&path, OpenMode::Read).await {
                Ok(buffer) => {
                    let guard = buffer.lock();
                    let text = String::from_utf8_lossy(&guard);
                    println!("{text}");
                }
                Err(e) => {
                    println!("No output available for node {hid}: {e:?}");
                }
            }
        });
        Ok(())
    }

    fn cmd_source(&mut self, args: &[&str]) -> Result<(), String> {
        let path = args.first().ok_or("Usage: source <file>")?;
        let content =
            std::fs::read_to_string(path).map_err(|e| format!("Failed to read {path}: {e}"))?;

        for line in content.lines() {
            let line = line.trim();
            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            println!("dagsh> {line}");
            match self.execute(line) {
                Ok(true) => {}
                Ok(false) => return Ok(()), // quit command
                Err(e) => println!("Error: {e}"),
            }
        }
        Ok(())
    }

    fn cmd_reset(&mut self) {
        // Kill background job if running
        if let Some(job) = self.bg_job.take() {
            println!("Killing background job...");
            job.abort_handle.abort();
            job.thread.join().ok();
        }

        self.handles.clear();
        self.vars.clear();
        let env = Arc::new(Environment::new(Arc::clone(&self.kv) as Arc<dyn KVBuffers>));
        env.actor_registry.write().register("cat", cat::execute);
        env.actor_registry
            .write()
            .register("dbg", dbg_actor::execute);
        env.actor_registry
            .write()
            .register("shell_input", shell_input_actor::execute);
        self.env = env;
        println!("DAG cleared.");
    }

    fn cmd_status(&self, args: &[&str]) -> Result<(), String> {
        let dag = self.env.dag.read();
        if args.is_empty() {
            // Overall status
            let mut total = 0;
            let mut running = 0;
            let mut terminated = 0;
            let mut not_started = 0;
            let mut suspended = 0;

            for &handle in &self.handles {
                if let Some(node) = dag.get_node(handle) {
                    total += 1;
                    match node.state {
                        NodeState::Running => running += 1,
                        NodeState::Terminated => terminated += 1,
                        NodeState::NotStarted => not_started += 1,
                        NodeState::Terminating => {}
                    }
                    if self.env.suspension.is_suspended(handle) {
                        suspended += 1;
                    }
                }
            }
            println!("Nodes: {total} total, {not_started} pending, {running} running, {suspended} suspended, {terminated} terminated");
        } else if let Some(handle_str) = args.first() {
            let handle = self
                .parse_handle(handle_str)
                .ok_or_else(|| format!("Invalid handle: {handle_str}"))?;
            let hid = handle.id();
            if let Some(node) = dag.get_node(handle) {
                let state = format_state(node.state);
                println!("Node {hid}: {} [{state}]", node.idname);
            } else {
                println!("Node {hid} not found");
            }
        }
        Ok(())
    }

    fn cmd_suspend(&self, args: &[&str]) -> Result<(), String> {
        let handle_str = args.first().ok_or("Usage: suspend <node>")?;
        let handle = self
            .parse_handle(handle_str)
            .ok_or_else(|| format!("Invalid handle: {handle_str}"))?;

        self.env.suspension.suspend(handle);
        println!("Suspended node {}", handle.id());
        Ok(())
    }

    fn cmd_resume(&self, args: &[&str]) -> Result<(), String> {
        let handle_str = args.first().ok_or("Usage: resume <node>")?;
        let handle = self
            .parse_handle(handle_str)
            .ok_or_else(|| format!("Invalid handle: {handle_str}"))?;

        self.env.suspension.resume(handle);
        println!("Resumed node {}", handle.id());
        Ok(())
    }

    fn cmd_wait(&self, args: &[&str]) -> Result<(), String> {
        let condition = args.first().ok_or("Usage: wait <condition> [args]")?;
        match *condition {
            "suspended" => {
                let handle_str = args.get(1).ok_or("Usage: wait suspended <node>")?;
                let handle = self
                    .parse_handle(handle_str)
                    .ok_or_else(|| format!("Invalid handle: {handle_str}"))?;

                let timeout = std::time::Duration::from_secs(5);
                let poll_interval = std::time::Duration::from_millis(10);
                let deadline = std::time::Instant::now() + timeout;

                loop {
                    if self.env.suspension.is_suspended(handle) {
                        return Ok(());
                    }
                    if std::time::Instant::now() >= deadline {
                        return Err(format!(
                            "Timeout: node {} not suspended after {}s",
                            handle.id(),
                            timeout.as_secs()
                        ));
                    }
                    std::thread::sleep(poll_interval);
                }
            }
            "terminated" => {
                let handle_str = args.get(1).ok_or("Usage: wait terminated <node>")?;
                let handle = self
                    .parse_handle(handle_str)
                    .ok_or_else(|| format!("Invalid handle: {handle_str}"))?;

                let timeout = std::time::Duration::from_secs(5);
                let poll_interval = std::time::Duration::from_millis(10);
                let deadline = std::time::Instant::now() + timeout;

                loop {
                    let state = self.env.dag.read().get_node(handle).map(|n| n.state);
                    if matches!(state, Some(ailetos::dag::NodeState::Terminated)) {
                        return Ok(());
                    }
                    if std::time::Instant::now() >= deadline {
                        return Err(format!(
                            "Timeout: node {} not terminated after {}s",
                            handle.id(),
                            timeout.as_secs()
                        ));
                    }
                    std::thread::sleep(poll_interval);
                }
            }
            other => Err(format!("Unknown wait condition: {other}")),
        }
    }

    fn cmd_write(&self, args: &[&str]) -> Result<(), String> {
        let handle_str = args.first().ok_or("Usage: write <node> <data>")?;
        let handle = self
            .parse_handle(handle_str)
            .ok_or_else(|| format!("Invalid handle: {handle_str}"))?;

        let data = parse_quoted_string(args.get(1..).unwrap_or(&[]));

        match shell_input_control::write_to_shell_input(handle, data.into_bytes()) {
            Ok(()) => {
                let hid = handle.id();
                println!("Wrote data to node {hid}");
                Ok(())
            }
            Err(e) => Err(format!("Failed to write: {e}")),
        }
    }

    fn cmd_close(&self, args: &[&str]) -> Result<(), String> {
        let handle_str = args.first().ok_or("Usage: close <node>")?;
        let handle = self
            .parse_handle(handle_str)
            .ok_or_else(|| format!("Invalid handle: {handle_str}"))?;

        match shell_input_control::close_shell_input(handle) {
            Ok(()) => {
                let hid = handle.id();
                println!("Closed node {hid}");
                Ok(())
            }
            Err(e) => Err(format!("Failed to close: {e}")),
        }
    }

    fn cmd_fg(&mut self, _args: &[&str]) -> Result<(), String> {
        if let Some(job) = self.bg_job.take() {
            println!("Waiting for background job to complete...");
            job.thread
                .join()
                .map_err(|_| "Background job panicked".to_string())?;
            println!("Job completed");
            Ok(())
        } else {
            Err("No background job running".to_string())
        }
    }

    fn cmd_kill(&mut self, args: &[&str]) -> Result<(), String> {
        let (exit_code, handle_str) = match args {
            [flag, node] if flag.starts_with('-') => {
                let code: i32 = flag[1..]
                    .parse()
                    .map_err(|_| format!("Invalid exit code: {flag}"))?;
                (code, *node)
            }
            [node] => (EOWNERDEAD, *node),
            _ => return Err("Usage: kill [-N] <node>".to_string()),
        };

        let handle = self
            .parse_handle(handle_str)
            .ok_or_else(|| format!("Invalid handle: {handle_str}"))?;

        let job = self.bg_job.as_ref().ok_or("No background job running")?;

        job.runtime_handle
            .block_on(job.bridge.cleanup_actor_io(handle, exit_code))
            .map_err(|e| format!("kill failed: {e}"))?;

        println!("Killed node {} with exit code {}", handle.id(), exit_code);
        Ok(())
    }

    fn release_background_job(&mut self) {
        shell_input_control::close_all_shell_inputs();
        for &handle in &self.handles {
            self.env.suspension.resume(handle);
        }
    }
}

impl Drop for DagShell {
    fn drop(&mut self) {
        if let Some(job) = self.bg_job.take() {
            self.release_background_job();
            drop(job.bridge);
            let _ = job.thread.join();
        }
    }
}

fn parse_explain(args: &[&str]) -> Option<String> {
    // Look for --explain= and collect the value (may span multiple args if quoted)
    let joined = args.join(" ");
    let rest = joined.strip_prefix("--explain=").or_else(|| {
        joined
            .find("--explain=")
            .map(|pos| &joined[pos + "--explain=".len()..])
    })?;
    if let Some(quoted) = rest.strip_prefix('"') {
        // Find closing quote
        if let Some(end) = quoted.find('"') {
            return quoted.get(..end).map(str::to_string);
        }
        // No closing quote, take everything
        return Some(quoted.to_string());
    }
    // No quotes, take until whitespace
    rest.split_whitespace().next().map(str::to_string)
}

fn parse_bytes_before_pause(args: &[&str]) -> Option<usize> {
    args.iter()
        .find_map(|a| a.strip_prefix("--bytes-before-pause="))
        .and_then(|s| s.parse().ok())
}

fn parse_quoted_string(args: &[&str]) -> String {
    // Parse first argument which may be quoted and span multiple tokens
    let joined = args.join(" ");

    // Find where --explain starts (if present)
    let value_part = joined.find("--explain=").map_or_else(
        || joined.trim(),
        |pos| joined.get(..pos).unwrap_or("").trim(),
    );

    // Remove surrounding quotes
    value_part.trim_matches('"').to_string()
}

fn format_state(state: NodeState) -> &'static str {
    match state {
        NodeState::NotStarted => "⋯ pending",
        NodeState::Running => "⚙ running",
        NodeState::Terminating => "⏳ terminating",
        NodeState::Terminated => "✓ built",
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        // Take chars to avoid splitting multi-byte characters
        let truncated: String = s.chars().take(max_len.saturating_sub(3)).collect();
        format!("{truncated}...")
    }
}

fn print_usage() {
    println!("Usage: dagsh [OPTIONS]");
    println!();
    println!("Options:");
    println!("  -l, --load <file>   Load script file on startup, then continue interactively");
    println!("  -h, --help          Show this help");
}

fn main() {
    // Initialize tracing subscriber to enable logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args: Vec<String> = std::env::args().collect();

    // Parse command line arguments
    let mut load_script: Option<String> = None;
    let mut i = 1;
    while i < args.len() {
        let Some(arg) = args.get(i) else { break };
        match arg.as_str() {
            "-h" | "--help" => {
                print_usage();
                return;
            }
            "-l" | "--load" => {
                let Some(path) = args.get(i + 1) else {
                    eprintln!("Error: --load requires a file argument");
                    std::process::exit(1);
                };
                load_script = Some(path.clone());
                i += 2;
            }
            a if a.starts_with('-') => {
                eprintln!("Unknown option: {a}");
                print_usage();
                std::process::exit(1);
            }
            a => {
                eprintln!("Unexpected argument: {a}");
                print_usage();
                std::process::exit(1);
            }
        }
    }

    let mut shell = DagShell::new();
    let Ok(mut rl) = Editor::<(), rustyline::history::DefaultHistory>::new() else {
        eprintln!("Failed to create editor");
        std::process::exit(1);
    };
    let _ = rl.set_max_history_size(1000);

    println!("DAG Shell v0.1");
    println!("Type 'help' for available commands.\n");

    // Load script from command line argument if provided
    if let Some(script_path) = load_script {
        println!("Loading {script_path}...\n");
        if let Err(e) = shell.cmd_source(&[&script_path]) {
            println!("Error: {e}");
        }
        println!();
    }

    loop {
        match rl.readline("dagsh> ") {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let _ = rl.add_history_entry(line);
                match shell.execute(line) {
                    Ok(true) => {}
                    Ok(false) => {
                        println!("Goodbye!");
                        break;
                    }
                    Err(e) => println!("Error: {e}"),
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("^C");
            }
            Err(ReadlineError::Eof) => {
                println!("Goodbye!");
                break;
            }
            Err(err) => {
                println!("Error: {err:?}");
                break;
            }
        }
    }
}
