//! DAG Shell - Interactive tool to build and run DAGs
//!
//! Minimal implementation for manually building and running DAGs.

use std::sync::Arc;

use ailetos::{DependsOn, Environment, For, Handle, KVBuffers, MemKV, NodeState, OpenMode};
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

/// Node definition for rebuilding DAG after execution
#[derive(Clone)]
enum NodeDef {
    Value {
        data: Vec<u8>,
        explain: Option<String>,
    },
    Actor {
        actor: String,
        explain: Option<String>,
    },
    Alias {
        name: String,
        target: Handle,
    },
}

struct DagShell {
    env: Environment<MemKV>,
    kv: Arc<MemKV>,
    /// Track all created node handles for listing
    handles: Vec<Handle>,
    /// Store node definitions for rebuilding after run
    defs: Vec<(Handle, NodeDef)>,
    /// Store dependencies for rebuilding
    deps: Vec<(Handle, Handle)>,
}

impl DagShell {
    fn new() -> Self {
        let kv = Arc::new(MemKV::new());
        let mut env = Environment::new(Arc::clone(&kv));
        env.actor_registry.register("cat", cat::execute);
        Self {
            env,
            kv,
            handles: Vec::new(),
            defs: Vec::new(),
            deps: Vec::new(),
        }
    }

    /// Rebuild environment from stored definitions
    fn rebuild_env(&mut self) {
        self.env = Environment::new(Arc::clone(&self.kv));
        self.env.actor_registry.register("cat", cat::execute);

        // Rebuild nodes in order
        for (handle, def) in &self.defs {
            let new_handle = match def {
                NodeDef::Value { data, explain } => {
                    self.env.add_value_node(data.clone(), explain.clone())
                }
                NodeDef::Actor { actor, explain } => {
                    self.env.add_node(actor.clone(), &[], explain.clone())
                }
                NodeDef::Alias { name, target } => self.env.add_alias(name.clone(), *target),
            };
            debug_assert_eq!(new_handle, *handle, "Handle mismatch during rebuild");
        }

        // Rebuild dependencies
        for (node, dep) in &self.deps {
            self.env.dag.add_dependency(For(*node), DependsOn(*dep));
        }
    }

    fn parse_handle(&self, s: &str) -> Option<Handle> {
        s.parse::<i64>().ok().map(Handle::new)
    }

    fn execute(&mut self, line: &str) -> Result<bool, String> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            return Ok(true);
        }

        match parts[0] {
            "quit" | "exit" | "q" => return Ok(false),
            "help" | "?" => self.cmd_help(),
            "node" => self.cmd_node(&parts[1..])?,
            "dep" => self.cmd_dep(&parts[1..])?,
            "deps" => self.cmd_deps(&parts[1..])?,
            "show" => self.cmd_show(&parts[1..])?,
            "run" => self.cmd_run(&parts[1..])?,
            "cat" => self.cmd_cat(&parts[1..])?,
            "status" => self.cmd_status(&parts[1..])?,
            "source" => self.cmd_source(&parts[1..])?,
            "reset" => self.cmd_reset()?,
            _ => println!("Unknown command: {}. Type 'help' for usage.", parts[0]),
        }

        Ok(true)
    }

    fn cmd_help(&self) {
        println!(
            r#"DAG Shell Commands:

Node Management:
  node add <actor> [--explain=text]   Add actor node (actors: cat)
  node value <data> [--explain=text]  Add value node (constant data)
  node alias <name> <target>          Add alias node
  node list                           List all nodes with status

Dependencies:
  dep <node> <dependency>             Add dependency (node depends on dependency)
  deps <node>                         Show direct dependencies

Visualization:
  show [node]                         Tree view (default: whole DAG)

Execution:
  run [node]                          Run the DAG (default: last node)

I/O:
  cat <node>                          Show output of a node

Status:
  status                              Overall DAG status
  status <node>                       Node status

Session:
  source <file>                       Run script file
  reset                               Clear all nodes and start fresh
  help                                Show this help
  quit                                Exit"#
        );
    }

    fn cmd_node(&mut self, args: &[&str]) -> Result<(), String> {
        if args.is_empty() {
            return Err("Usage: node <add|value|alias|list> ...".to_string());
        }

        match args[0] {
            "add" => {
                if args.len() < 2 {
                    return Err("Usage: node add <actor> [--explain=text]".to_string());
                }
                let actor = args[1].to_string();
                let explain = self.parse_explain(&args[2..]);
                let handle = self.env.add_node(actor.clone(), &[], explain.clone());
                self.handles.push(handle);
                self.defs.push((
                    handle,
                    NodeDef::Actor {
                        actor: actor.clone(),
                        explain: explain.clone(),
                    },
                ));
                println!(
                    "Added node {}: {} {}",
                    handle.id(),
                    actor,
                    explain.map(|e| format!("({})", e)).unwrap_or_default()
                );
            }
            "value" => {
                if args.len() < 2 {
                    return Err("Usage: node value <data> [--explain=text]".to_string());
                }
                let data = self.parse_quoted_string(&args[1..]);
                let explain = self.parse_explain(&args[1..]);
                let handle = self.env.add_value_node(data.as_bytes().to_vec(), explain.clone());
                self.handles.push(handle);
                self.defs.push((
                    handle,
                    NodeDef::Value {
                        data: data.as_bytes().to_vec(),
                        explain: explain.clone(),
                    },
                ));
                println!(
                    "Added value node {}: \"{}\" {}",
                    handle.id(),
                    truncate(&data, 30),
                    explain.map(|e| format!("({})", e)).unwrap_or_default()
                );
            }
            "alias" => {
                if args.len() < 3 {
                    return Err("Usage: node alias <name> <target>".to_string());
                }
                let name = args[1].to_string();
                let target = self
                    .parse_handle(args[2])
                    .ok_or_else(|| format!("Invalid handle: {}", args[2]))?;
                let handle = self.env.add_alias(name.clone(), target);
                self.handles.push(handle);
                self.defs.push((
                    handle,
                    NodeDef::Alias {
                        name: name.clone(),
                        target,
                    },
                ));
                println!(
                    "Added alias {}: {} -> {}",
                    handle.id(),
                    name,
                    target.id()
                );
            }
            "list" => {
                if self.handles.is_empty() {
                    println!("No nodes");
                } else {
                    for &handle in &self.handles {
                        if let Some(node) = self.env.dag.get_node(handle) {
                            let state_str = format_state(node.state);
                            let explain = node
                                .explain
                                .as_ref()
                                .map(|e| format!(" # {}", e))
                                .unwrap_or_default();
                            println!(
                                "  {} {} [{}]{}",
                                node.pid.id(),
                                node.idname,
                                state_str,
                                explain
                            );
                        }
                    }
                }
            }
            _ => return Err(format!("Unknown node subcommand: {}", args[0])),
        }
        Ok(())
    }

    fn cmd_dep(&mut self, args: &[&str]) -> Result<(), String> {
        if args.len() < 2 {
            return Err("Usage: dep <node> <dependency>".to_string());
        }
        let node = self
            .parse_handle(args[0])
            .ok_or_else(|| format!("Invalid handle: {}", args[0]))?;
        let dep = self
            .parse_handle(args[1])
            .ok_or_else(|| format!("Invalid handle: {}", args[1]))?;
        self.env.dag.add_dependency(For(node), DependsOn(dep));
        self.deps.push((node, dep));
        println!("Added dependency: {} depends on {}", node.id(), dep.id());
        Ok(())
    }

    fn cmd_deps(&self, args: &[&str]) -> Result<(), String> {
        if args.is_empty() {
            return Err("Usage: deps <node>".to_string());
        }
        let handle = self
            .parse_handle(args[0])
            .ok_or_else(|| format!("Invalid handle: {}", args[0]))?;
        let deps: Vec<_> = self.env.dag.get_direct_dependencies(handle).collect();
        if deps.is_empty() {
            println!("Node {} has no dependencies", handle.id());
        } else {
            println!("Node {} depends on:", handle.id());
            for dep in deps {
                let node = self.env.dag.get_node(dep);
                let name = node.map(|n| n.idname.as_str()).unwrap_or("?");
                println!("  {} ({})", dep.id(), name);
            }
        }
        Ok(())
    }

    fn cmd_show(&self, args: &[&str]) -> Result<(), String> {
        if args.is_empty() {
            // Show whole DAG: find terminal nodes (nodes that nothing depends on)
            if self.handles.is_empty() {
                println!("No nodes");
                return Ok(());
            }
            let terminals: Vec<Handle> = self
                .handles
                .iter()
                .filter(|&&h| self.env.dag.get_direct_dependents(h).next().is_none())
                .copied()
                .collect();
            for handle in terminals {
                let tree = self.env.dag.dump_colored(handle);
                print!("{}", tree);
            }
            return Ok(());
        }
        let handle = self
            .parse_handle(args[0])
            .ok_or_else(|| format!("Invalid handle: {}", args[0]))?;
        let tree = self.env.dag.dump_colored(handle);
        print!("{}", tree);
        Ok(())
    }

    fn cmd_run(&mut self, args: &[&str]) -> Result<(), String> {
        let handle = if args.is_empty() {
            // Find last node
            *self
                .handles
                .last()
                .ok_or_else(|| "No nodes to run".to_string())?
        } else {
            self.parse_handle(args[0])
                .ok_or_else(|| format!("Invalid handle: {}", args[0]))?
        };

        println!("Running DAG from node {}...", handle.id());

        // Attach stdout to the target node
        let resolved = self.env.resolve(handle);
        self.env.attach_stdout(resolved);

        // Run synchronously using tokio runtime
        let kv = Arc::clone(&self.kv);
        let env = std::mem::replace(&mut self.env, Environment::new(Arc::clone(&kv)));

        let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
        rt.block_on(async {
            env.run(handle).await;
        });

        // Rebuild environment from stored definitions
        self.rebuild_env();

        println!("\nDAG execution completed.");
        Ok(())
    }

    fn cmd_cat(&self, args: &[&str]) -> Result<(), String> {
        if args.is_empty() {
            return Err("Usage: cat <node>".to_string());
        }
        let handle = self
            .parse_handle(args[0])
            .ok_or_else(|| format!("Invalid handle: {}", args[0]))?;

        let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
        rt.block_on(async {
            let path = format!("{}/stdout", handle.id());
            match self.kv.open(&path, OpenMode::Read).await {
                Ok(buffer) => {
                    let guard = buffer.lock();
                    let text = String::from_utf8_lossy(&guard);
                    println!("{}", text);
                }
                Err(e) => {
                    println!("No output available for node {}: {:?}", handle.id(), e);
                }
            }
        });
        Ok(())
    }

    fn cmd_source(&mut self, args: &[&str]) -> Result<(), String> {
        if args.is_empty() {
            return Err("Usage: source <file>".to_string());
        }
        let path = args[0];
        let content =
            std::fs::read_to_string(path).map_err(|e| format!("Failed to read {}: {}", path, e))?;

        for line in content.lines() {
            let line = line.trim();
            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            println!("dagsh> {}", line);
            match self.execute(line) {
                Ok(true) => {}
                Ok(false) => return Ok(()), // quit command
                Err(e) => println!("Error: {}", e),
            }
        }
        Ok(())
    }

    fn cmd_reset(&mut self) -> Result<(), String> {
        self.handles.clear();
        self.defs.clear();
        self.deps.clear();
        self.env = Environment::new(Arc::clone(&self.kv));
        self.env.actor_registry.register("cat", cat::execute);
        println!("DAG cleared.");
        Ok(())
    }

    fn cmd_status(&self, args: &[&str]) -> Result<(), String> {
        if args.is_empty() {
            // Overall status
            let mut total = 0;
            let mut running = 0;
            let mut terminated = 0;
            let mut not_started = 0;

            for &handle in &self.handles {
                if let Some(node) = self.env.dag.get_node(handle) {
                    total += 1;
                    match node.state {
                        NodeState::Running => running += 1,
                        NodeState::Terminated => terminated += 1,
                        NodeState::NotStarted => not_started += 1,
                        NodeState::Terminating => {}
                    }
                }
            }
            println!(
                "Nodes: {} total, {} not started, {} running, {} terminated",
                total, not_started, running, terminated
            );
        } else {
            let handle = self
                .parse_handle(args[0])
                .ok_or_else(|| format!("Invalid handle: {}", args[0]))?;
            if let Some(node) = self.env.dag.get_node(handle) {
                println!(
                    "Node {}: {} [{}]",
                    handle.id(),
                    node.idname,
                    format_state(node.state)
                );
            } else {
                println!("Node {} not found", handle.id());
            }
        }
        Ok(())
    }

    fn parse_explain(&self, args: &[&str]) -> Option<String> {
        // Look for --explain= and collect the value (may span multiple args if quoted)
        let joined = args.join(" ");
        if let Some(pos) = joined.find("--explain=") {
            let rest = &joined[pos + "--explain=".len()..];
            if rest.starts_with('"') {
                // Find closing quote
                if let Some(end) = rest[1..].find('"') {
                    return Some(rest[1..end + 1].to_string());
                }
                // No closing quote, take everything
                return Some(rest[1..].to_string());
            }
            // No quotes, take until whitespace
            let value = rest.split_whitespace().next().unwrap_or("");
            return Some(value.to_string());
        }
        None
    }

    fn parse_quoted_string(&self, args: &[&str]) -> String {
        // Parse first argument which may be quoted and span multiple tokens
        let joined = args.join(" ");

        // Find where --explain starts (if present)
        let value_part = if let Some(pos) = joined.find("--explain=") {
            joined[..pos].trim()
        } else {
            joined.trim()
        };

        // Remove surrounding quotes
        value_part.trim_matches('"').to_string()
    }
}

fn format_state(state: NodeState) -> &'static str {
    match state {
        NodeState::NotStarted => "⋯ not built",
        NodeState::Running => "⚙ running",
        NodeState::Terminating => "⏳ terminating",
        NodeState::Terminated => "✓ built",
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

fn main() {
    let mut shell = DagShell::new();
    let mut rl = DefaultEditor::new().expect("Failed to create editor");

    println!("DAG Shell v0.1");
    println!("Type 'help' for available commands.\n");

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
                    Err(e) => println!("Error: {}", e),
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
                println!("Error: {:?}", err);
                break;
            }
        }
    }
}
