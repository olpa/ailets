//! Command implementations for `DagShell`.

use std::sync::Arc;

use actor_runtime::StdHandle;
use ailetos::{
    pipe::{pipe_path, LatentState, PipeEntryInspection},
    DependsOn, For, Handle, KVBuffers, NodeState, OpenMode, StopConditions, TopologicalOrderIter,
};

use crate::output::{parse_color, OutputSinkWriter};
use crate::shell_ui::{
    format_state, parse_bytes_before_pause, parse_explain, parse_quoted_string, truncate, HELP_TEXT,
};
use crate::{dbg_control, shell_input_control, DagShell};

const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(10);

impl DagShell {
    pub(crate) fn cmd_help(&self) {
        self.sink.println(HELP_TEXT);
    }

    pub(crate) fn cmd_set(&mut self, args: &[&str]) -> Result<(), String> {
        match args {
            [var_name, "=", "node", rest @ ..] => {
                let handle = self.cmd_node_inner(rest)?;
                self.vars.insert((*var_name).to_string(), handle);
                Ok(())
            }
            _ => Err("Usage: set <var> = node ...".to_string()),
        }
    }

    pub(crate) fn cmd_node(&mut self, args: &[&str]) -> Result<(), String> {
        if args.first() == Some(&"list") {
            self.cmd_node_list();
        } else {
            self.cmd_node_inner(args)?;
        }
        Ok(())
    }

    pub(crate) fn cmd_node_list(&self) {
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

    pub(crate) fn cmd_node_inner(&mut self, args: &[&str]) -> Result<Handle, String> {
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
                self.sink
                    .println(&format!("Added node {id}: {actor} {expl}"));
                Ok(handle)
            }
            ["add"] => Err("Usage: node add <actor> [--explain=text]".to_string()),
            ["value", rest @ ..] if !rest.is_empty() => {
                let data = parse_quoted_string(rest);
                let explain = parse_explain(rest);
                let env = Arc::clone(&self.env);
                let data_bytes = data.as_bytes().to_vec();
                let explain_clone = explain.clone();
                let handle = self
                    .ailetos_async_rt
                    .block_on(async move { env.add_value_node(data_bytes, explain_clone).await })
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
            ["alias", name, rest @ ..] if !rest.is_empty() => {
                let name = (*name).to_string();
                let mut targets = Vec::new();
                for target_str in rest {
                    let target = self
                        .parse_handle(target_str)
                        .ok_or_else(|| format!("Invalid handle: {target_str}"))?;
                    targets.push(target);
                }
                let handle = self.env.add_aliases(name.clone(), &targets);
                self.handles.push(handle);
                let id = handle.id();
                let tids: Vec<_> = targets.iter().map(|t| t.id().to_string()).collect();
                self.sink
                    .println(&format!("Added alias {id}: {name} -> {}", tids.join(", ")));
                Ok(handle)
            }
            ["alias", ..] => Err("Usage: node alias <name> <target> [<target>...]".to_string()),
            [cmd, ..] => Err(format!("Unknown node subcommand: {cmd}")),
            [] => Err("Usage: node <add|value|alias|list> ...".to_string()),
        }
    }

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

    pub(crate) fn cmd_deps(&self, args: &[&str]) -> Result<(), String> {
        let handle_str = args.first().ok_or("Usage: deps <node>")?;
        let handle = self
            .parse_handle(handle_str)
            .ok_or_else(|| format!("Invalid handle: {handle_str}"))?;
        let dag = self.env.dag.read();
        let deps: Vec<_> = dag.get_direct_dependencies(handle).collect();
        let hid = handle.id();
        if deps.is_empty() {
            self.sink
                .println(&format!("Node {hid} has no dependencies"));
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

    pub(crate) fn cmd_show(&self, args: &[&str]) -> Result<(), String> {
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
            let pending = self.executor.snapshot_pending();
            let roots = if terminals.is_empty() {
                self.handles.clone()
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
        let suspension = Some(&*self.env.suspension);
        let pending = self.executor.snapshot_pending();
        let tree = dag.dump_colored(handle, suspension, Some(&pending));
        for line in tree.lines() {
            self.sink.println(line);
        }
        Ok(())
    }

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

        self.executor
            .submit(handle, stop_conditions.clone())
            .map_err(|_| "Executor has shut down".to_string())?;

        if bg_flag {
            self.attach_stdout_for_run(handle, one_step, stop_before, stop_after, true, color);
        } else {
            self.attach_stdout_for_run(handle, one_step, stop_before, stop_after, false, color);
            self.join_handles(wait_targets)?;
            self.sink.println("");
        }
        Ok(())
    }

    fn join_handles(&mut self, targets: Vec<Handle>) -> Result<(), String> {
        let targets: Vec<Handle> = targets
            .into_iter()
            .flat_map(|t| self.env.resolve_all(t))
            .collect();
        let receivers: Vec<_> = targets.iter().map(|&t| self.executor.join(t)).collect();
        self.foreground_join
            .store(true, std::sync::atomic::Ordering::Relaxed);
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
            foreground_join.store(false, std::sync::atomic::Ordering::Relaxed);
            result
        })
    }

    pub(crate) fn cmd_join(&mut self, args: &[&str]) -> Result<(), String> {
        let handle_str = args.first().ok_or("Usage: join <node>")?;
        let handle = self
            .parse_handle(handle_str)
            .ok_or_else(|| format!("Invalid handle: {handle_str}"))?;
        self.join_handles(self.env.resolve_all(handle))
    }

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

    pub(crate) fn find_default_target(&self) -> Result<Handle, String> {
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

    pub(crate) fn cmd_cat(&self, args: &[&str]) -> Result<(), String> {
        let handle_str = args.first().ok_or("Usage: cat <node>")?;
        let handle = self
            .parse_handle(handle_str)
            .ok_or_else(|| format!("Invalid handle: {handle_str}"))?;

        let kv = Arc::clone(&self.kv);
        let output = self.ailetos_async_rt.block_on(async move {
            let path = pipe_path(handle, StdHandle::Stdout as isize);
            match kv.open(&path, OpenMode::Read).await {
                Ok(buffer) => {
                    let guard = buffer.lock();
                    Ok(String::from_utf8_lossy(&guard).into_owned())
                }
                Err(e) => Err(format!(
                    "No output available for node {}: {e:?}",
                    handle.id()
                )),
            }
        });
        match output {
            Ok(text) => self.sink.println(&text),
            Err(e) => self.sink.println(&e),
        }
        Ok(())
    }

    /// # Errors
    /// Returns an error string if the file cannot be read or a command fails.
    pub fn cmd_source(&mut self, args: &[&str]) -> Result<crate::ShellControl, String> {
        let path = args.first().ok_or("Usage: source <file>")?;
        let content =
            std::fs::read_to_string(path).map_err(|e| format!("Failed to read {path}: {e}"))?;
        self.execute_lines(content.lines())
    }

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
        let dag = self.env.dag.read();
        let pending_set = self.executor.snapshot_pending();
        let mut total = 0;
        let mut running = 0;
        let mut terminated = 0;
        let mut pending = 0;
        let mut suspended = 0;

        for &handle in &self.handles {
            if let Some(node) = dag.get_node(handle) {
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

    pub(crate) fn cmd_resume(&self, args: &[&str]) -> Result<(), String> {
        let handle_str = args.first().ok_or("Usage: resume <node>")?;
        let handle = self
            .parse_handle(handle_str)
            .ok_or_else(|| format!("Invalid handle: {handle_str}"))?;
        self.env.suspension.resume(handle);
        self.sink.println(&format!("Resumed node {}", handle.id()));
        Ok(())
    }

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
