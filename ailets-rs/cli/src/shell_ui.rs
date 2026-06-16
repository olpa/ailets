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
    pub load_script: Option<String>,
    pub prompt_items: Vec<PromptArg>,
}

/// # Errors
/// Returns an error string if an unknown option or missing argument is encountered.
pub fn parse_args(args: &[String]) -> Result<CliArgs, String> {
    let mut load_script: Option<String> = None;
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
                load_script = Some(path.clone());
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
                // extension point: @{...} attr-override block would be parsed here
                let path = a[1..].to_string();
                prompt_items.push(PromptArg::File { path, attrs: vec![] });
                i += 1;
            }
            a => {
                prompt_items.push(PromptArg::Text(a.to_string()));
                i += 1;
            }
        }
    }
    Ok(CliArgs { load_script, prompt_items })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        std::iter::once("dagsh")
            .chain(v.iter().copied())
            .map(str::to_string)
            .collect()
    }

    // test 1: plain text arg → PromptArg::Text with correct string
    #[test]
    fn test_plain_text_arg() {
        let result = parse_args(&args(&["hello"])).unwrap();
        assert_eq!(result.prompt_items, vec![PromptArg::Text("hello".to_string())]);
    }

    // test 2: @file.txt → PromptArg::File with path "file.txt" (prefix stripped)
    #[test]
    fn test_at_file_arg() {
        let result = parse_args(&args(&["@notes.txt"])).unwrap();
        assert_eq!(
            result.prompt_items,
            vec![PromptArg::File {
                path: "notes.txt".to_string(),
                attrs: vec![]
            }]
        );
    }

    // test 3: - and @- both → PromptArg::Stdin
    #[test]
    fn test_stdin_dash() {
        let result = parse_args(&args(&["-"])).unwrap();
        assert_eq!(result.prompt_items, vec![PromptArg::Stdin]);
    }

    #[test]
    fn test_stdin_at_dash() {
        let result = parse_args(&args(&["@-"])).unwrap();
        assert_eq!(result.prompt_items, vec![PromptArg::Stdin]);
    }

    // test 4: --system-prompt "S" → PromptArg::SystemPrompt("S")
    #[test]
    fn test_system_prompt() {
        let result = parse_args(&args(&["--system-prompt", "Be concise"])).unwrap();
        assert_eq!(
            result.prompt_items,
            vec![PromptArg::SystemPrompt("Be concise".to_string())]
        );
    }

    // test 5: mixed args preserve order: --system-prompt "S" "hello" @f.txt → [SystemPrompt, Text, File]
    #[test]
    fn test_mixed_order_preserved() {
        let result = parse_args(&args(&["--system-prompt", "S", "hello", "@f.txt"])).unwrap();
        assert_eq!(
            result.prompt_items,
            vec![
                PromptArg::SystemPrompt("S".to_string()),
                PromptArg::Text("hello".to_string()),
                PromptArg::File {
                    path: "f.txt".to_string(),
                    attrs: vec![]
                },
            ]
        );
    }

    // test 6: --system-prompt with no following value → error
    #[test]
    fn test_system_prompt_missing_value() {
        let result = parse_args(&args(&["--system-prompt"]));
        assert!(result.is_err());
    }

    // test 7: -l script.tcl coexists with prompt args
    #[test]
    fn test_load_script_coexists_with_prompt_items() {
        let result = parse_args(&args(&["-l", "run.tcl", "hello"])).unwrap();
        assert_eq!(result.load_script, Some("run.tcl".to_string()));
        assert_eq!(result.prompt_items, vec![PromptArg::Text("hello".to_string())]);
    }
}

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
