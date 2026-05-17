//! DAG Shell library - DagShell and OutputSink.

pub(crate) mod dbg_actor;
pub(crate) mod dbg_control;
pub(crate) mod shell_input_actor;
pub(crate) mod shell_input_control;

use std::collections::HashMap;
use std::sync::Arc;

use ailetos::{
    DependsOn, Environment, Executor, For, Handle, KVBuffers, MemKV, NodeState, OpenMode,
    StopConditions, TopologicalOrderIter,
};
use futures::future::Abortable;

// ---------------------------------------------------------------------------
// OutputSink
// ---------------------------------------------------------------------------

/// Where DagShell command output is written.
/// Production code uses [`StdoutSink`]; tests use a capturing sink.
pub trait OutputSink {
    fn println(&self, line: &str);
}

pub struct StdoutSink;

impl OutputSink for StdoutSink {
    fn println(&self, line: &str) {
        println!("{line}");
    }
}

// ---------------------------------------------------------------------------
// DagShell
// ---------------------------------------------------------------------------

struct BackgroundJob {
    thread: std::thread::JoinHandle<()>,
    abort_handle: futures::future::AbortHandle,
}

pub struct DagShell {
    env: std::sync::Arc<Environment>,
    kv: Arc<MemKV>,
    handles: Vec<Handle>,
    vars: HashMap<String, Handle>,
    bg_job: Option<BackgroundJob>,
    sink: Box<dyn OutputSink>,
}

impl DagShell {
    pub fn new() -> Self {
        Self::new_with_sink(Box::new(StdoutSink))
    }

    pub fn new_with_sink(sink: Box<dyn OutputSink>) -> Self {
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
            sink,
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
                self.release_background_job();
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
                self.sink
                    .println(&format!("Unknown command: {cmd}. Type 'help' for usage."));
            }
        }

        Ok(true)
    }

    fn cmd_help(&self) {
        self.sink.println(
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
  dep $foo $bar                       Use $var to reference variables",
        );
    }

    fn cmd_set(&mut self, args: &[&str]) -> Result<(), String> {
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
            self.sink.println("No nodes");
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
                    self.sink
                        .println(&format!("  {pid} {} [{state_str}]{explain}", node.idname));
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

                if actor == "dbg" {
                    let bytes_before_pause = parse_bytes_before_pause(rest);
                    dbg_control::register_dbg_actor(handle, bytes_before_pause);
                }
                if actor == "shell_input" {
                    shell_input_control::register_shell_input_actor(handle);
                }

                let id = handle.id();
                let expl = explain.map_or_else(String::new, |e| format!("({e})"));
                self.sink.println(&format!("Added node {id}: {actor} {expl}"));
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
                self.sink
                    .println(&format!("Added value node {id}: \"{truncated}\" {expl}"));
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
                self.sink
                    .println(&format!("Added alias {id}: {name} -> {tid}"));
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
        self.sink
            .println(&format!("Added dependency: {nid} depends on {did}"));
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
            self.sink.println(&format!("Node {hid} has no dependencies"));
        } else {
            self.sink.println(&format!("Node {hid} depends on:"));
            for dep in deps {
                let node = dag.get_node(dep);
                let name = node.map_or("?", |n| n.idname.as_str());
                let did = dep.id();
                self.sink.println(&format!("  {did} ({name})"));
            }
        }
        Ok(())
    }

    fn cmd_show(&self, args: &[&str]) -> Result<(), String> {
        let dag = self.env.dag.read();
        if args.is_empty() {
            if self.handles.is_empty() {
                self.sink.println("No nodes");
                return Ok(());
            }
            let terminals: Vec<Handle> = self
                .handles
                .iter()
                .filter(|&&h| dag.get_direct_dependents(h).next().is_none())
                .copied()
                .collect();

            let suspension = Some(&*self.env.suspension);
            if terminals.is_empty() {
                for handle in &self.handles {
                    let tree = dag.dump_colored(*handle, suspension);
                    for line in tree.lines() {
                        self.sink.println(line);
                    }
                }
            } else {
                for handle in terminals {
                    let tree = dag.dump_colored(handle, suspension);
                    for line in tree.lines() {
                        self.sink.println(line);
                    }
                }
            }
            return Ok(());
        }
        let handle_str = args.first().ok_or("Usage: show <node>")?;
        let handle = self
            .parse_handle(handle_str)
            .ok_or_else(|| format!("Invalid handle: {handle_str}"))?;
        let suspension = Some(&*self.env.suspension);
        let tree = dag.dump_colored(handle, suspension);
        for line in tree.lines() {
            self.sink.println(line);
        }
        Ok(())
    }

    fn cmd_run(&mut self, args: &[&str]) -> Result<(), String> {
        let mut one_step = false;
        let mut stop_before: Option<Handle> = None;
        let mut stop_after: Option<Handle> = None;
        let mut target_arg: Option<&str> = None;
        let mut bg_flag = false;

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
            sb
        } else {
            self.find_default_target()?
        };

        self.attach_stdout_for_run(handle, one_step, stop_before, stop_after);

        let stop_conditions = StopConditions {
            one_step,
            stop_before,
            stop_after,
        };

        if bg_flag {
            self.run_background(handle, stop_conditions)?;
        } else {
            self.run_foreground(handle, stop_conditions)?;
        }

        self.sink.println("");
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
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();

        let thread = std::thread::spawn(move || {
            tracing::info!("Foreground thread starting");
            let Ok(rt) = tokio::runtime::Runtime::new() else {
                ready_tx
                    .send(Err("Failed to create tokio runtime".to_string()))
                    .ok();
                return;
            };
            rt.block_on(async move {
                let executor = Executor::start(Arc::clone(&env), None);
                ready_tx.send(Ok(())).ok();
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

        match ready_rx.recv() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                thread.join().ok();
                return Err(e);
            }
            Err(_) => {
                thread.join().ok();
                return Ok(());
            }
        }

        let (tx, rx) = std::sync::mpsc::channel();

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

        let mut job = Some(BackgroundJob {
            thread,
            abort_handle,
        });

        loop {
            match rx.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(()) => {
                    println!("\n^C - Moved to background (use 'fg' to wait, 'kill' to terminate)");
                    self.bg_job = job.take();
                    return Ok(());
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if job.as_ref().is_some_and(|j| j.thread.is_finished()) {
                        ctrlc_abort_handle.abort();
                        if let Some(j) = job.take() {
                            j.thread.join().ok();
                        }
                        return Ok(());
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
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
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();

        let thread = std::thread::spawn(move || {
            tracing::info!("Background thread starting");
            let Ok(rt) = tokio::runtime::Runtime::new() else {
                ready_tx
                    .send(Err("Failed to create tokio runtime".to_string()))
                    .ok();
                return;
            };
            rt.block_on(async move {
                let executor = Executor::start(Arc::clone(&env), None);
                ready_tx.send(Ok(())).ok();
                executor.submit(handle, stop_conditions).ok();
                let result = Abortable::new(executor.shutdown(), abort_registration).await;
                if let Ok(()) = result {
                    tracing::info!("Background job completed");
                } else {
                    tracing::info!("Background job aborted");
                }
            });
        });

        match ready_rx.recv() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                thread.join().ok();
                return Err(e);
            }
            Err(_) => {
                thread.join().ok();
                return Err("Background thread exited before signalling ready".to_string());
            }
        }

        self.bg_job = Some(BackgroundJob {
            thread,
            abort_handle,
        });

        self.sink
            .println("Started background run (use 'fg' to wait, 'kill' to terminate)");

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
            let resolved = self.env.resolve(stop_after_handle);
            self.env.attach_stdout(resolved);
        } else if let Some(stop_before_handle) = stop_before {
            let deps: Vec<Handle> = {
                let dag = self.env.dag.read();
                dag.get_direct_dependencies(stop_before_handle).collect()
            };
            for dep in deps {
                let resolved = self.env.resolve(dep);
                self.env.attach_stdout(resolved);
            }
        } else if one_step {
            let ready_node = {
                let dag = self.env.dag.read();
                TopologicalOrderIter::new(&dag, target).next()
            };
            if let Some(ready_node) = ready_node {
                let resolved = self.env.resolve(ready_node);
                self.env.attach_stdout(resolved);
            }
        } else {
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
        let kv = Arc::clone(&self.kv);
        let output = rt.block_on(async move {
            let path = format!("{hid}/stdout");
            match kv.open(&path, OpenMode::Read).await {
                Ok(buffer) => {
                    let guard = buffer.lock();
                    Ok(String::from_utf8_lossy(&guard).into_owned())
                }
                Err(e) => Err(format!("No output available for node {hid}: {e:?}")),
            }
        });
        match output {
            Ok(text) => self.sink.println(&text),
            Err(e) => self.sink.println(&e),
        }
        Ok(())
    }

    pub fn cmd_source(&mut self, args: &[&str]) -> Result<(), String> {
        let path = args.first().ok_or("Usage: source <file>")?;
        let content =
            std::fs::read_to_string(path).map_err(|e| format!("Failed to read {path}: {e}"))?;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            self.sink.println(&format!("dagsh> {line}"));
            match self.execute(line) {
                Ok(true) => {}
                Ok(false) => return Ok(()),
                Err(e) => self.sink.println(&format!("Error: {e}")),
            }
        }
        Ok(())
    }

    fn cmd_reset(&mut self) {
        if let Some(job) = self.bg_job.take() {
            self.sink.println("Killing background job...");
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
        self.sink.println("DAG cleared.");
    }

    fn cmd_status(&self, args: &[&str]) -> Result<(), String> {
        let dag = self.env.dag.read();
        if args.is_empty() {
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
            self.sink.println(&format!("Nodes: {total} total, {not_started} pending, {running} running, {suspended} suspended, {terminated} terminated"));
        } else if let Some(handle_str) = args.first() {
            let handle = self
                .parse_handle(handle_str)
                .ok_or_else(|| format!("Invalid handle: {handle_str}"))?;
            let hid = handle.id();
            if let Some(node) = dag.get_node(handle) {
                let state = format_state(node.state);
                self.sink
                    .println(&format!("Node {hid}: {} [{state}]", node.idname));
            } else {
                self.sink.println(&format!("Node {hid} not found"));
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
        self.sink.println(&format!("Suspended node {}", handle.id()));
        Ok(())
    }

    fn cmd_resume(&self, args: &[&str]) -> Result<(), String> {
        let handle_str = args.first().ok_or("Usage: resume <node>")?;
        let handle = self
            .parse_handle(handle_str)
            .ok_or_else(|| format!("Invalid handle: {handle_str}"))?;
        self.env.suspension.resume(handle);
        self.sink.println(&format!("Resumed node {}", handle.id()));
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
                    if matches!(state, Some(NodeState::Terminated)) {
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
                self.sink.println(&format!("Wrote data to node {hid}"));
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
                self.sink.println(&format!("Closed node {hid}"));
                Ok(())
            }
            Err(e) => Err(format!("Failed to close: {e}")),
        }
    }

    fn cmd_fg(&mut self, _args: &[&str]) -> Result<(), String> {
        if let Some(job) = self.bg_job.take() {
            self.sink.println("Waiting for background job to complete...");
            job.thread
                .join()
                .map_err(|_| "Background job panicked".to_string())?;
            self.sink.println("Job completed");
            Ok(())
        } else {
            Err("No background job running".to_string())
        }
    }

    fn cmd_kill(&mut self, args: &[&str]) -> Result<(), String> {
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

    fn release_background_job(&mut self) {
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
        if let Some(job) = self.bg_job.take() {
            self.release_background_job();
            let _ = job.thread.join();
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_explain(args: &[&str]) -> Option<String> {
    let joined = args.join(" ");
    let rest = joined.strip_prefix("--explain=").or_else(|| {
        joined
            .find("--explain=")
            .map(|pos| &joined[pos + "--explain=".len()..])
    })?;
    if let Some(quoted) = rest.strip_prefix('"') {
        if let Some(end) = quoted.find('"') {
            return quoted.get(..end).map(str::to_string);
        }
        return Some(quoted.to_string());
    }
    rest.split_whitespace().next().map(str::to_string)
}

fn parse_bytes_before_pause(args: &[&str]) -> Option<usize> {
    args.iter()
        .find_map(|a| a.strip_prefix("--bytes-before-pause="))
        .and_then(|s| s.parse().ok())
}

fn parse_quoted_string(args: &[&str]) -> String {
    let joined = args.join(" ");
    let value_part = joined.find("--explain=").map_or_else(
        || joined.trim(),
        |pos| joined.get(..pos).unwrap_or("").trim(),
    );
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
        let truncated: String = s.chars().take(max_len.saturating_sub(3)).collect();
        format!("{truncated}...")
    }
}
