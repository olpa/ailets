//! DAG Shell - Interactive tool to build and run DAGs
//!
//! Minimal implementation for manually building and running DAGs.

use std::collections::HashMap;
use std::sync::Arc;

use ailetos::{DependsOn, Environment, For, Handle, KVBuffers, MemKV, NodeState, OpenMode};
use rustyline::config::Configurer;
use rustyline::error::ReadlineError;
use rustyline::Editor;

struct DagShell {
    env: Environment<MemKV>,
    kv: Arc<MemKV>,
    /// Track all created node handles for listing
    handles: Vec<Handle>,
    /// Variables mapping names to handles
    vars: HashMap<String, Handle>,
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
            vars: HashMap::new(),
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
        if parts.is_empty() {
            return Ok(true);
        }

        match parts[0] {
            "quit" | "exit" | "q" => return Ok(false),
            "help" | "?" => self.cmd_help(),
            "set" => self.cmd_set(&parts[1..])?,
            "node" => {
                self.cmd_node(&parts[1..])?;
            }
            "dep" => self.cmd_dep(&parts[1..])?,
            "deps" => self.cmd_deps(&parts[1..])?,
            "show" => self.cmd_show(&parts[1..])?,
            "run" => self.cmd_run(&parts[1..])?,
            "cat" => self.cmd_cat(&parts[1..])?,
            "status" => self.cmd_status(&parts[1..])?,
            "source" | "load" => self.cmd_source(&parts[1..])?,
            "reset" => self.cmd_reset()?,
            _ => {
                println!("Unknown command: {}. Type 'help' for usage.", parts[0]);
            }
        };

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
  load <file>                         Run script file (alias: source)
  reset                               Clear all nodes and start fresh
  help                                Show this help
  quit                                Exit

Variables:
  set var = node ...                  Assign node to variable
  dep $foo $bar                       Use $var to reference variables"#
        );
    }

    fn cmd_set(&mut self, args: &[&str]) -> Result<(), String> {
        // set var = node ...
        if args.len() < 3 {
            return Err("Usage: set <var> = node ...".to_string());
        }
        let var_name = args[0];
        if args[1] != "=" {
            return Err("Usage: set <var> = node ...".to_string());
        }
        // args[2..] should be a node command
        if args.len() < 3 || args[2] != "node" {
            return Err("Usage: set <var> = node ...".to_string());
        }
        let handle = self.cmd_node_inner(&args[3..])?;
        self.vars.insert(var_name.to_string(), handle);
        Ok(())
    }

    fn cmd_node(&mut self, args: &[&str]) -> Result<(), String> {
        if args.first() == Some(&"list") {
            self.cmd_node_list()
        } else {
            self.cmd_node_inner(args)?;
            Ok(())
        }
    }

    fn cmd_node_list(&self) -> Result<(), String> {
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
        Ok(())
    }

    fn cmd_node_inner(&mut self, args: &[&str]) -> Result<Handle, String> {
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
                println!(
                    "Added node {}: {} {}",
                    handle.id(),
                    actor,
                    explain.map(|e| format!("({})", e)).unwrap_or_default()
                );
                Ok(handle)
            }
            "value" => {
                if args.len() < 2 {
                    return Err("Usage: node value <data> [--explain=text]".to_string());
                }
                let data = self.parse_quoted_string(&args[1..]);
                let explain = self.parse_explain(&args[1..]);
                let handle = self.env.add_value_node(data.as_bytes().to_vec(), explain.clone());
                self.handles.push(handle);
                println!(
                    "Added value node {}: \"{}\" {}",
                    handle.id(),
                    truncate(&data, 30),
                    explain.map(|e| format!("({})", e)).unwrap_or_default()
                );
                Ok(handle)
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
                println!(
                    "Added alias {}: {} -> {}",
                    handle.id(),
                    name,
                    target.id()
                );
                Ok(handle)
            }
            _ => Err(format!("Unknown node subcommand: {}", args[0])),
        }
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
        self.env.dag.write().add_dependency(For(node), DependsOn(dep));
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
        let dag = self.env.dag.read();
        let deps: Vec<_> = dag.get_direct_dependencies(handle).collect();
        if deps.is_empty() {
            println!("Node {} has no dependencies", handle.id());
        } else {
            println!("Node {} depends on:", handle.id());
            for dep in deps {
                let node = dag.get_node(dep);
                let name = node.map(|n| n.idname.as_str()).unwrap_or("?");
                println!("  {} ({})", dep.id(), name);
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
            for handle in terminals {
                let tree = dag.dump_colored(handle);
                print!("{}", tree);
            }
            return Ok(());
        }
        let handle = self
            .parse_handle(args[0])
            .ok_or_else(|| format!("Invalid handle: {}", args[0]))?;
        let tree = dag.dump_colored(handle);
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
        let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
        rt.block_on(async {
            self.env.run(handle).await;
        });

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
        self.vars.clear();
        self.env = Environment::new(Arc::clone(&self.kv));
        self.env.actor_registry.register("cat", cat::execute);
        println!("DAG cleared.");
        Ok(())
    }

    fn cmd_status(&self, args: &[&str]) -> Result<(), String> {
        let dag = self.env.dag.read();
        if args.is_empty() {
            // Overall status
            let mut total = 0;
            let mut running = 0;
            let mut terminated = 0;
            let mut not_started = 0;

            for &handle in &self.handles {
                if let Some(node) = dag.get_node(handle) {
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
            if let Some(node) = dag.get_node(handle) {
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

fn print_usage() {
    println!("Usage: dagsh [OPTIONS]");
    println!();
    println!("Options:");
    println!("  -l, --load <file>   Load script file on startup, then continue interactively");
    println!("  -h, --help          Show this help");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Parse command line arguments
    let mut load_script: Option<String> = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print_usage();
                return;
            }
            "-l" | "--load" => {
                if i + 1 >= args.len() {
                    eprintln!("Error: --load requires a file argument");
                    std::process::exit(1);
                }
                load_script = Some(args[i + 1].clone());
                i += 2;
            }
            arg if arg.starts_with('-') => {
                eprintln!("Unknown option: {}", arg);
                print_usage();
                std::process::exit(1);
            }
            _ => {
                eprintln!("Unexpected argument: {}", args[i]);
                print_usage();
                std::process::exit(1);
            }
        }
    }

    let mut shell = DagShell::new();
    let mut rl = Editor::<(), rustyline::history::DefaultHistory>::new()
        .expect("Failed to create editor");
    rl.set_max_history_size(1000).unwrap();

    println!("DAG Shell v0.1");
    println!("Type 'help' for available commands.\n");

    // Load script from command line argument if provided
    if let Some(script_path) = load_script {
        println!("Loading {}...\n", script_path);
        if let Err(e) = shell.cmd_source(&[&script_path]) {
            println!("Error: {}", e);
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
