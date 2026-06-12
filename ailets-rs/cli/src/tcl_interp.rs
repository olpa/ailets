//! Molt (TCL) integration — register DAG shell commands with a molt `Interp`.

use molt::{check_args, molt_ok, types::*, Interp};

use crate::DagShell;

// ---------------------------------------------------------------------------
// Context stored inside the molt Interp — no global/thread-local state needed.
// ---------------------------------------------------------------------------

pub(crate) struct ShellContext {
    // Raw pointer to DagShell, valid only while DagShell::execute is on the stack.
    pub(crate) shell: *mut DagShell,
    pub(crate) exit_requested: bool,
}

// Safety: ShellContext is only ever accessed from the single CLI thread that
// owns DagShell. The Interp (and therefore ShellContext) never crosses threads.
unsafe impl Send for ShellContext {}

// Safety: shell pointer is set before eval and cleared after; command handlers
// only run during eval, so the pointer is always valid when dereferenced.
// Command handlers access DagShell fields other than `tcl` (which was moved out
// of self before eval), so no aliasing of the same memory occurs.
fn get_shell<'a>(interp: &mut Interp, ctx: ContextID) -> &'a mut DagShell {
    let ptr = interp.context::<ShellContext>(ctx).shell;
    debug_assert!(!ptr.is_null(), "no active DagShell context");
    unsafe { &mut *ptr }
}

fn wrap(r: Result<(), String>) -> MoltResult {
    r.map(|_| Value::empty())
        .map_err(|e| Exception::molt_err(Value::from(e)))
}

// ---------------------------------------------------------------------------
// Command metadata — single source of truth for registration, help, completion
// ---------------------------------------------------------------------------

pub struct CommandEntry {
    /// Primary name first; the rest are aliases shown in help and available for completion.
    pub names: &'static [&'static str],
    pub handler: CommandFunc,
    /// Argument signature — what follows the command name (matches check_args argsig).
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

pub static COMMANDS: &[CommandEntry] = &[
    CommandEntry {
        names: &["node"],
        handler: tcl_node,
        argsig: "<add|value|alias|list> ...",
        section: "Node Management",
        description: "Manage DAG nodes",
        detail: Some(concat!(
            "    add <actor> [--explain=text]    Add actor node (actors: cat, dbg, shell_input)\n",
            "    value <data> [--explain=text]   Add value node (constant data)\n",
            "    alias <name> <target> ...       Add alias (one or more targets)\n",
            "    list                            List all nodes with status",
        )),
    },
    CommandEntry {
        names: &["dep"],
        handler: tcl_dep,
        argsig: "node dependency",
        section: "Dependencies",
        description: "Add dependency (node depends on dependency)",
        detail: None,
    },
    CommandEntry {
        names: &["deps"],
        handler: tcl_deps,
        argsig: "node",
        section: "Dependencies",
        description: "Show direct dependencies of a node",
        detail: None,
    },
    CommandEntry {
        names: &["show"],
        handler: tcl_show,
        argsig: "?node?",
        section: "Visualization",
        description: "Tree view (default: whole DAG)",
        detail: None,
    },
    CommandEntry {
        names: &["run"],
        handler: tcl_run,
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
    },
    CommandEntry {
        names: &["join", "await"],
        handler: tcl_join,
        argsig: "node",
        section: "Job Control",
        description: "Wait for node to terminate; Ctrl+C to detach",
        detail: None,
    },
    CommandEntry {
        names: &["follow"],
        handler: tcl_follow,
        argsig: "node ?--color name?",
        section: "Job Control",
        description: "Attach node stdout; optional 256-color name or 0-255",
        detail: None,
    },
    CommandEntry {
        names: &["kill"],
        handler: tcl_kill,
        argsig: "?-N? node",
        section: "Job Control",
        description: "Kill actor with exit code N (default 130)",
        detail: None,
    },
    CommandEntry {
        names: &["cat"],
        handler: tcl_cat,
        argsig: "node",
        section: "I/O",
        description: "Show output of a node",
        detail: None,
    },
    CommandEntry {
        names: &["status"],
        handler: tcl_status,
        argsig: "?node?",
        section: "Status",
        description: "Overall DAG status, or status of a specific node",
        detail: None,
    },
    CommandEntry {
        names: &["suspend"],
        handler: tcl_suspend,
        argsig: "node",
        section: "Debug",
        description: "Suspend a running actor",
        detail: None,
    },
    CommandEntry {
        names: &["resume"],
        handler: tcl_resume,
        argsig: "node",
        section: "Debug",
        description: "Resume a suspended actor (dbg or general)",
        detail: None,
    },
    CommandEntry {
        names: &["wait"],
        handler: tcl_wait,
        argsig: "condition ?args?",
        section: "Debug",
        description: "Block until condition; Ctrl+C to detach",
        detail: Some(concat!(
            "    suspended <node>                Block until node is suspended\n",
            "    terminated <node>               Block until node is terminated",
        )),
    },
    CommandEntry {
        names: &["write"],
        handler: tcl_write,
        argsig: "node ?data?",
        section: "Shell Input",
        description: "Write data to a shell_input actor",
        detail: None,
    },
    CommandEntry {
        names: &["close"],
        handler: tcl_close,
        argsig: "node",
        section: "Shell Input",
        description: "Close a shell_input actor (send EOF)",
        detail: None,
    },
    CommandEntry {
        names: &["source", "load"],
        handler: tcl_source,
        argsig: "file",
        section: "Session",
        description: "Run TCL script file",
        detail: None,
    },
    CommandEntry {
        names: &["help", "?"],
        handler: tcl_help,
        argsig: "",
        section: "Session",
        description: "Show this help",
        detail: None,
    },
    CommandEntry {
        names: &["quit", "exit", "q"],
        handler: tcl_quit,
        argsig: "",
        section: "Session",
        description: "Exit the shell",
        detail: None,
    },
];

/// Generate the help text from `COMMANDS`, grouped by section.
pub fn generate_help() -> String {
    // Column at which descriptions start on main command lines.
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
// Interpreter factory — iterates COMMANDS to register all handlers.
// ---------------------------------------------------------------------------

pub(crate) fn make_interp() -> (Interp, ContextID) {
    let mut interp = Interp::new();
    let ctx = interp.save_context(ShellContext {
        shell: std::ptr::null_mut(),
        exit_requested: false,
    });
    for entry in COMMANDS {
        for &name in entry.names {
            interp.add_context_command(name, entry.handler, ctx);
        }
    }
    (interp, ctx)
}

// ---------------------------------------------------------------------------
// Command handlers
// ---------------------------------------------------------------------------

fn tcl_node(interp: &mut Interp, ctx: ContextID, argv: &[Value]) -> MoltResult {
    check_args(1, argv, 2, 0, "<add|value|alias|list> ...")?;
    let shell = get_shell(interp, ctx);
    let args: Vec<&str> = argv[1..].iter().map(|v| v.as_str()).collect();
    if args.first() == Some(&"list") {
        shell.cmd_node_list();
        return molt_ok!();
    }
    match shell.cmd_node_inner(&args) {
        Ok(handle) => Ok(Value::from(handle.id().to_string())),
        Err(e) => Err(Exception::molt_err(Value::from(e))),
    }
}

fn tcl_dep(interp: &mut Interp, ctx: ContextID, argv: &[Value]) -> MoltResult {
    check_args(1, argv, 3, 3, "node dependency")?;
    let args: Vec<&str> = argv[1..].iter().map(|v| v.as_str()).collect();
    wrap(get_shell(interp, ctx).cmd_dep(&args))
}

fn tcl_deps(interp: &mut Interp, ctx: ContextID, argv: &[Value]) -> MoltResult {
    check_args(1, argv, 2, 2, "node")?;
    let args: Vec<&str> = argv[1..].iter().map(|v| v.as_str()).collect();
    wrap(get_shell(interp, ctx).cmd_deps(&args))
}

fn tcl_show(interp: &mut Interp, ctx: ContextID, argv: &[Value]) -> MoltResult {
    check_args(1, argv, 1, 2, "?node?")?;
    let args: Vec<&str> = argv[1..].iter().map(|v| v.as_str()).collect();
    wrap(get_shell(interp, ctx).cmd_show(&args))
}

fn tcl_run(interp: &mut Interp, ctx: ContextID, argv: &[Value]) -> MoltResult {
    check_args(1, argv, 1, 0, "?options? ?node?")?;
    let args: Vec<&str> = argv[1..].iter().map(|v| v.as_str()).collect();
    wrap(get_shell(interp, ctx).cmd_run(&args))
}

fn tcl_join(interp: &mut Interp, ctx: ContextID, argv: &[Value]) -> MoltResult {
    check_args(1, argv, 2, 2, "node")?;
    let args: Vec<&str> = argv[1..].iter().map(|v| v.as_str()).collect();
    wrap(get_shell(interp, ctx).cmd_join(&args))
}

fn tcl_follow(interp: &mut Interp, ctx: ContextID, argv: &[Value]) -> MoltResult {
    check_args(1, argv, 2, 0, "node ?--color name?")?;
    let args: Vec<&str> = argv[1..].iter().map(|v| v.as_str()).collect();
    wrap(get_shell(interp, ctx).cmd_follow(&args))
}

fn tcl_cat(interp: &mut Interp, ctx: ContextID, argv: &[Value]) -> MoltResult {
    check_args(1, argv, 2, 2, "node")?;
    let args: Vec<&str> = argv[1..].iter().map(|v| v.as_str()).collect();
    wrap(get_shell(interp, ctx).cmd_cat(&args))
}

fn tcl_status(interp: &mut Interp, ctx: ContextID, argv: &[Value]) -> MoltResult {
    check_args(1, argv, 1, 2, "?node?")?;
    let args: Vec<&str> = argv[1..].iter().map(|v| v.as_str()).collect();
    get_shell(interp, ctx).cmd_status(&args);
    molt_ok!()
}

// Recursive eval of a file; interp is already borrowed by the caller's eval.
fn tcl_source(interp: &mut Interp, _ctx: ContextID, argv: &[Value]) -> MoltResult {
    check_args(1, argv, 2, 2, "file")?;
    let content = std::fs::read_to_string(argv[1].as_str())
        .map_err(|e| Exception::molt_err(Value::from(e.to_string())))?;
    interp.eval(&content)
}

fn tcl_suspend(interp: &mut Interp, ctx: ContextID, argv: &[Value]) -> MoltResult {
    check_args(1, argv, 2, 2, "node")?;
    let args: Vec<&str> = argv[1..].iter().map(|v| v.as_str()).collect();
    wrap(get_shell(interp, ctx).cmd_suspend(&args))
}

fn tcl_resume(interp: &mut Interp, ctx: ContextID, argv: &[Value]) -> MoltResult {
    check_args(1, argv, 2, 2, "node")?;
    let args: Vec<&str> = argv[1..].iter().map(|v| v.as_str()).collect();
    wrap(get_shell(interp, ctx).cmd_resume(&args))
}

fn tcl_wait(interp: &mut Interp, ctx: ContextID, argv: &[Value]) -> MoltResult {
    check_args(1, argv, 2, 0, "condition ?args?")?;
    let args: Vec<&str> = argv[1..].iter().map(|v| v.as_str()).collect();
    wrap(get_shell(interp, ctx).cmd_wait(&args))
}

fn tcl_write(interp: &mut Interp, ctx: ContextID, argv: &[Value]) -> MoltResult {
    check_args(1, argv, 2, 0, "node ?data?")?;
    let args: Vec<&str> = argv[1..].iter().map(|v| v.as_str()).collect();
    wrap(get_shell(interp, ctx).cmd_write(&args))
}

fn tcl_close(interp: &mut Interp, ctx: ContextID, argv: &[Value]) -> MoltResult {
    check_args(1, argv, 2, 2, "node")?;
    let args: Vec<&str> = argv[1..].iter().map(|v| v.as_str()).collect();
    wrap(get_shell(interp, ctx).cmd_close(&args))
}

fn tcl_kill(interp: &mut Interp, ctx: ContextID, argv: &[Value]) -> MoltResult {
    check_args(1, argv, 2, 3, "?-N? node")?;
    let args: Vec<&str> = argv[1..].iter().map(|v| v.as_str()).collect();
    wrap(get_shell(interp, ctx).cmd_kill(&args))
}

fn tcl_help(interp: &mut Interp, ctx: ContextID, argv: &[Value]) -> MoltResult {
    check_args(1, argv, 1, 1, "")?;
    get_shell(interp, ctx).cmd_help();
    molt_ok!()
}

fn tcl_quit(interp: &mut Interp, ctx: ContextID, argv: &[Value]) -> MoltResult {
    check_args(1, argv, 1, 1, "")?;
    interp.context::<ShellContext>(ctx).exit_requested = true;
    // Return an error to unwind the current script; execute() converts this to ShellControl::Exit.
    Err(Exception::molt_err("exit".into()))
}
