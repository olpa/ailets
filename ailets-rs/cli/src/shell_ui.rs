//! Shell UI support - parsing, formatting, and user-facing strings.

use std::sync::Arc;

use ailetos::NodeState;
use rustyline::ExternalPrinter;

use crate::OutputSink;

// ---------------------------------------------------------------------------
// ChannelSink - rustyline ExternalPrinter integration
// ---------------------------------------------------------------------------

/// Sends background notifications through a channel consumed by a thread that
/// holds the rustyline ExternalPrinter. Printing via ExternalPrinter ensures
/// notifications never corrupt the current input line.
pub struct ChannelSink {
    tx: std::sync::mpsc::Sender<String>,
}

impl ChannelSink {
    pub fn new(tx: std::sync::mpsc::Sender<String>) -> Self {
        Self { tx }
    }
}

impl OutputSink for ChannelSink {
    fn print(&self, text: &str) {
        if let Err(e) = self.tx.send(text.to_string()) {
            tracing::warn!("ChannelSink: receiver dropped: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// Command-line argument parsing
// ---------------------------------------------------------------------------

pub fn print_usage() {
    println!("Usage: dagsh [OPTIONS]");
    println!();
    println!("Options:");
    println!("  -l, --load <file>   Load script file on startup, then continue interactively");
    println!("  -h, --help          Show this help");
}

pub struct CliArgs {
    pub load_script: Option<String>,
}

pub fn parse_args(args: &[String]) -> Result<CliArgs, String> {
    let mut load_script: Option<String> = None;
    let mut i = 1;
    while i < args.len() {
        let Some(arg) = args.get(i) else { break };
        match arg.as_str() {
            "-h" | "--help" => {
                print_usage();
                std::process::exit(0);
            }
            "-l" | "--load" => {
                let Some(path) = args.get(i + 1) else {
                    return Err("--load requires a file argument".to_string());
                };
                load_script = Some(path.clone());
                i += 2;
            }
            a if a.starts_with('-') => {
                return Err(format!("Unknown option: {a}"));
            }
            a => {
                return Err(format!("Unexpected argument: {a}"));
            }
        }
    }
    Ok(CliArgs { load_script })
}

// ---------------------------------------------------------------------------
// Help text
// ---------------------------------------------------------------------------

pub const HELP_TEXT: &str = r"DAG Shell Commands:

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
  run [node] [options]                Submit run to ailetos; waits by default
    --one-step                        Execute only the first ready node
    --stop-before <node>              Stop before executing this node
    --stop-after <node>               Stop after executing this node
    --bg                              Submit and return immediately (background)
    --color <name>                    Colorize output (CSS/X11 name or 0-255; --bg only)

Job Control:
  join <node>                         Wait for node to terminate; Ctrl+C to detach
  await <node>                        Synonym for join
  follow <node> [--color <name>]      Attach node stdout; optional 256-color name or 0-255
  kill [-N] <node>                    Kill actor with exit code N (default 130)

I/O:
  cat <node>                          Show output of a node

Status:
  status                              Overall DAG status
  status <node>                       Node status

Debug:
  suspend <node>                      Suspend a running actor
  resume <node>                       Resume a suspended actor (dbg or general)
  wait suspended <node>               Block until node is suspended (polls 10 ms, 5 s timeout)
  wait terminated <node>              Block until node is terminated (Ctrl+C to detach)

Shell Input:
  write <node> <data>                 Write data to a shell_input actor
  close <node>                        Close a shell_input actor (send EOF)

Session:
  load <file>                         Run script file (alias: source)
  help                                Show this help
  quit                                Exit

Variables:
  set var = node ...                  Assign node to variable
  dep $foo $bar                       Use $var to reference variables";

// ---------------------------------------------------------------------------
// Argument parsing helpers
// ---------------------------------------------------------------------------

pub fn parse_explain(args: &[&str]) -> Option<String> {
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

pub fn parse_bytes_before_pause(args: &[&str]) -> Option<usize> {
    args.iter()
        .find_map(|a| a.strip_prefix("--bytes-before-pause="))
        .and_then(|s| s.parse().ok())
}

pub fn parse_quoted_string(args: &[&str]) -> String {
    let joined = args.join(" ");
    let value_part = joined.find("--explain=").map_or_else(
        || joined.trim(),
        |pos| joined.get(..pos).unwrap_or("").trim(),
    );
    value_part.trim_matches('"').to_string()
}

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

pub fn format_state(state: NodeState) -> &'static str {
    match state {
        NodeState::NotStarted => "⋯ pending",
        NodeState::Running => "⚙ running",
        NodeState::Terminating => "⏳ terminating",
        NodeState::Terminated => "✓ built",
    }
}

pub fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_len.saturating_sub(3)).collect();
        format!("{truncated}...")
    }
}

// ---------------------------------------------------------------------------
// Rustyline notification sink setup
// ---------------------------------------------------------------------------

/// Creates a notification sink that routes through rustyline's ExternalPrinter,
/// or falls back to StdoutSink if the printer can't be created.
pub fn create_notification_sink<H>(
    rl: &mut rustyline::Editor<(), H>,
    rt: &tokio::runtime::Handle,
) -> Arc<dyn OutputSink>
where
    H: rustyline::history::History,
{
    match rl.create_external_printer() {
        Ok(mut printer) => {
            let (tx, rx) = std::sync::mpsc::channel::<String>();
            rt.spawn_blocking(move || {
                while let Ok(msg) = rx.recv() {
                    if let Err(e) = printer.print(msg) {
                        tracing::warn!("external printer failed: {e}");
                    }
                }
            });
            Arc::new(ChannelSink::new(tx))
        }
        Err(_) => Arc::new(crate::StdoutSink),
    }
}
