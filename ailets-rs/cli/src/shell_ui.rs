//! Shell UI support - parsing, formatting, and user-facing strings.

use std::sync::Arc;

use ailetos::NodeState;
use rustyline::ExternalPrinter;

use crate::OutputSink;

// ---------------------------------------------------------------------------
// ChannelSink - rustyline ExternalPrinter integration
// ---------------------------------------------------------------------------

/// Sends background notifications through a channel consumed by a thread that
/// holds the rustyline `ExternalPrinter`. Printing via `ExternalPrinter` ensures
/// notifications never corrupt the current input line.
pub struct ChannelSink {
    tx: std::sync::mpsc::Sender<String>,
}

impl ChannelSink {
    #[must_use]
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

    fn println(&self, line: &str) {
        if let Err(e) = self.tx.send(format!("{line}\n")) {
            tracing::warn!("ChannelSink: receiver dropped: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// Command-line argument parsing
// ---------------------------------------------------------------------------

pub fn print_usage() {
    println!("Usage: dagsh [OPTIONS] [PROMPT_ITEMS...]");
    println!();
    println!("Options:");
    println!("  -l, --load <file>          Load TCL script file on startup");
    println!("  --system-prompt <text>     Add a system prompt item");
    println!("  -h, --help                 Show this help");
    println!();
    println!("Prompt items (positional):");
    println!("  \"text\"                     Plain text prompt");
    println!("  @path                      Read file as prompt content");
    println!("  - or @-                    Read stdin as prompt content");
}

#[derive(Debug, PartialEq)]
pub enum PromptArg {
    SystemPrompt(String),
    Text(String),
    File { path: String, attrs: Vec<(String, String)> },
    Stdin,
}

pub struct CliArgs {
    pub load_scripts: Vec<String>,
    pub prompt_items: Vec<PromptArg>,
}

/// Parses the part of a `@...` arg after the `@` prefix.
/// If the string contains `=`, it is treated as a comma-separated `key=value`
/// list; the required `file=` key provides the path and remaining pairs are attrs.
/// Otherwise the whole string is the path with no attrs.
fn parse_at_arg(s: &str) -> Result<(String, Vec<(String, String)>), String> {
    if !s.contains('=') {
        return Ok((s.to_string(), vec![]));
    }
    let mut path: Option<String> = None;
    let mut attrs: Vec<(String, String)> = Vec::new();
    for pair in s.split(',').filter(|p| !p.is_empty()) {
        let (k, v) = pair
            .split_once('=')
            .ok_or_else(|| format!("invalid attr '{pair}' in '@{s}'; expected key=value"))?;
        if k == "file" {
            path = Some(v.to_string());
        } else {
            attrs.push((k.to_string(), v.to_string()));
        }
    }
    let path = path.ok_or_else(|| format!("missing required 'file=' key in '@{s}'"))?;
    Ok((path, attrs))
}

pub fn parse_args(args: &[String]) -> Result<CliArgs, String> {
    let mut load_scripts: Vec<String> = Vec::new();
    let mut prompt_items: Vec<PromptArg> = Vec::new();
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
                load_scripts.push(path.clone());
                i += 2;
            }
            "--system-prompt" => {
                let Some(text) = args.get(i + 1) else {
                    return Err("--system-prompt requires a text argument".to_string());
                };
                prompt_items.push(PromptArg::SystemPrompt(text.clone()));
                i += 2;
            }
            "-" => {
                prompt_items.push(PromptArg::Stdin);
                i += 1;
            }
            a if a.starts_with('-') => {
                return Err(format!("Unknown option: {a}"));
            }
            "@-" => {
                prompt_items.push(PromptArg::Stdin);
                i += 1;
            }
            a if a.starts_with('@') => {
                let (path, attrs) = parse_at_arg(&a[1..])?;
                prompt_items.push(PromptArg::File { path, attrs });
                i += 1;
            }
            a => {
                prompt_items.push(PromptArg::Text(a.to_string()));
                i += 1;
            }
        }
    }
    Ok(CliArgs { load_scripts, prompt_items })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Rustyline helper — tab completion
// ---------------------------------------------------------------------------

pub struct ShellHelper;

impl rustyline::completion::Completer for ShellHelper {
    type Candidate = rustyline::completion::Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Self::Candidate>)> {
        let before = &line[..pos];
        // Only complete the first word (the command name).
        if before.contains(' ') {
            return Ok((pos, vec![]));
        }
        let candidates = crate::commands::COMMANDS
            .iter()
            .flat_map(|e| e.names.iter().copied())
            .filter(|name| name.starts_with(before))
            .map(|name| rustyline::completion::Pair {
                display: name.to_string(),
                replacement: name.to_string(),
            })
            .collect();
        Ok((0, candidates))
    }
}

impl rustyline::hint::Hinter for ShellHelper {
    type Hint = String;
}

impl rustyline::highlight::Highlighter for ShellHelper {}

impl rustyline::validate::Validator for ShellHelper {}

impl rustyline::Helper for ShellHelper {}

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
    // Take until the next flag or end of string.
    let end = rest.find(" --").unwrap_or(rest.len());
    rest.get(..end).map(str::to_string)
}

#[must_use]
pub fn parse_bytes_before_pause(args: &[&str]) -> Option<usize> {
    args.iter()
        .find_map(|a| a.strip_prefix("--bytes-before-pause="))
        .and_then(|s| s.parse().ok())
}

#[must_use]
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

#[must_use]
pub fn format_state(state: NodeState) -> &'static str {
    match state {
        NodeState::NotStarted => "⋯ pending",
        NodeState::Running => "⚙ running",
        NodeState::Terminating => "⏳ terminating",
        NodeState::Terminated => "✓ built",
    }
}

#[must_use]
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

/// Creates a notification sink that routes through rustyline's `ExternalPrinter`,
/// or falls back to `StdoutSink` if the printer can't be created.
pub fn create_notification_sink<H>(
    rl: &mut rustyline::Editor<ShellHelper, H>,
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
