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
        let (cmd, rest) = match parts.split_first() {
            None => return Ok(true),
            Some((cmd, rest)) => (*cmd, rest),
        };

        match cmd {
            "quit" | "exit" | "q" => return Ok(false),
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
                let id = handle.id();
                let expl = explain.map_or_else(String::new, |e| format!("({e})"));
                println!("Added node {id}: {actor} {expl}");
                Ok(handle)
            }
            ["add"] => Err("Usage: node add <actor> [--explain=text]".to_string()),
            ["value", rest @ ..] if !rest.is_empty() => {
                let data = parse_quoted_string(rest);
                let explain = parse_explain(rest);
                let handle = self
                    .env
                    .add_value_node(data.as_bytes().to_vec(), explain.clone());
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
            if terminals.is_empty() {
                for handle in &self.handles {
                    let tree = dag.dump_colored(*handle);
                    print!("{tree}");
                }
            } else {
                for handle in terminals {
                    let tree = dag.dump_colored(handle);
                    print!("{tree}");
                }
            }
            return Ok(());
        }
        let handle_str = args.first().ok_or("Usage: show <node>")?;
        let handle = self
            .parse_handle(handle_str)
            .ok_or_else(|| format!("Invalid handle: {handle_str}"))?;
        let tree = dag.dump_colored(handle);
        print!("{tree}");
        Ok(())
    }

    fn cmd_run(&mut self, args: &[&str]) -> Result<(), String> {
        let handle = if let Some(handle_str) = args.first() {
            self.parse_handle(handle_str)
                .ok_or_else(|| format!("Invalid handle: {handle_str}"))?
        } else {
            // Find last node
            *self
                .handles
                .last()
                .ok_or_else(|| "No nodes to run".to_string())?
        };

        let hid = handle.id();
        println!("Running DAG from node {hid}...");

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
        self.handles.clear();
        self.vars.clear();
        self.env = Environment::new(Arc::clone(&self.kv));
        self.env.actor_registry.register("cat", cat::execute);
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
            println!("Nodes: {total} total, {not_started} not started, {running} running, {terminated} terminated");
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
