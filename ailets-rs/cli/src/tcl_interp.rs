//! Molt (TCL) integration — register DAG shell commands with a molt `Interp`.

use molt::{
    check_args, molt_ok,
    types::{CommandFunc, ContextID, Exception, MoltResult, Value},
    Interp,
};

use crate::commands::{
    ENTRY_ALIAS, ENTRY_CAT, ENTRY_CLOSE, ENTRY_DAG, ENTRY_DEP, ENTRY_FOLLOW, ENTRY_HELP,
    ENTRY_JOIN, ENTRY_KILL, ENTRY_NODE, ENTRY_NODES, ENTRY_QUIT, ENTRY_RESUME, ENTRY_RUN,
    ENTRY_SHOW, ENTRY_SOURCE, ENTRY_STATUS, ENTRY_SUSPEND, ENTRY_VALUE, ENTRY_WAIT, ENTRY_WRITE,
};
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

// Safety: see the comment in DagShell::execute that explains why a raw pointer
// is used and what invariants keep it valid.
fn get_shell<'a>(interp: &mut Interp, ctx: ContextID) -> &'a mut DagShell {
    let ptr = interp.context::<ShellContext>(ctx).shell;
    debug_assert!(!ptr.is_null(), "no active DagShell context");
    unsafe { &mut *ptr }
}

fn wrap(r: Result<(), String>) -> MoltResult {
    r.map(|()| Value::empty())
        .map_err(|e| Exception::molt_err(Value::from(e)))
}

// ---------------------------------------------------------------------------
// Interpreter factory — pairs each CommandMeta with its TCL handler.
// ---------------------------------------------------------------------------

#[must_use]
pub fn make_interp() -> (Interp, ContextID) {
    let mut interp = Interp::new();
    let ctx = interp.save_context(ShellContext {
        shell: std::ptr::null_mut(),
        exit_requested: false,
    });
    let bindings: &[(&crate::commands::CommandMeta, CommandFunc)] = &[
        (&ENTRY_NODE, tcl_node),
        (&ENTRY_VALUE, tcl_value),
        (&ENTRY_ALIAS, tcl_alias),
        (&ENTRY_NODES, tcl_nodes),
        (&ENTRY_DEP, tcl_dep),
        (&ENTRY_SHOW, tcl_show),
        (&ENTRY_RUN, tcl_run),
        (&ENTRY_JOIN, tcl_join),
        (&ENTRY_FOLLOW, tcl_follow),
        (&ENTRY_KILL, tcl_kill),
        (&ENTRY_CAT, tcl_cat),
        (&ENTRY_STATUS, tcl_status),
        (&ENTRY_SUSPEND, tcl_suspend),
        (&ENTRY_RESUME, tcl_resume),
        (&ENTRY_WAIT, tcl_wait),
        (&ENTRY_WRITE, tcl_write),
        (&ENTRY_CLOSE, tcl_close),
        (&ENTRY_SOURCE, tcl_source),
        (&ENTRY_HELP, tcl_help),
        (&ENTRY_QUIT, tcl_quit),
        (&ENTRY_DAG, tcl_dag),
    ];
    for (entry, handler) in bindings {
        for &name in entry.names {
            interp.add_context_command(name, *handler, ctx);
        }
    }
    (interp, ctx)
}

// ---------------------------------------------------------------------------
// Command handlers
// ---------------------------------------------------------------------------

fn tcl_node(interp: &mut Interp, ctx: ContextID, argv: &[Value]) -> MoltResult {
    check_args(1, argv, 2, 0, "<actor> [--explain=text]")?;
    let tail: Vec<&str> = argv
        .get(1..)
        .unwrap_or_default()
        .iter()
        .map(Value::as_str)
        .collect();
    match get_shell(interp, ctx).cmd_node(&tail) {
        Ok(handle) => Ok(Value::from(handle.id().to_string())),
        Err(e) => Err(Exception::molt_err(Value::from(e))),
    }
}

fn tcl_value(interp: &mut Interp, ctx: ContextID, argv: &[Value]) -> MoltResult {
    check_args(1, argv, 2, 0, "<data> [--explain=text]")?;
    let tail: Vec<&str> = argv
        .get(1..)
        .unwrap_or_default()
        .iter()
        .map(Value::as_str)
        .collect();
    match get_shell(interp, ctx).cmd_value(&tail) {
        Ok(handle) => Ok(Value::from(handle.id().to_string())),
        Err(e) => Err(Exception::molt_err(Value::from(e))),
    }
}

fn tcl_alias(interp: &mut Interp, ctx: ContextID, argv: &[Value]) -> MoltResult {
    check_args(1, argv, 3, 0, "<name> <target> ...")?;
    let tail: Vec<&str> = argv
        .get(1..)
        .unwrap_or_default()
        .iter()
        .map(Value::as_str)
        .collect();
    match get_shell(interp, ctx).cmd_alias(&tail) {
        Ok(handle) => Ok(Value::from(handle.id().to_string())),
        Err(e) => Err(Exception::molt_err(Value::from(e))),
    }
}

fn tcl_nodes(interp: &mut Interp, ctx: ContextID, argv: &[Value]) -> MoltResult {
    check_args(1, argv, 1, 1, "")?;
    get_shell(interp, ctx).cmd_nodes();
    molt_ok!()
}

fn tcl_dep(interp: &mut Interp, ctx: ContextID, argv: &[Value]) -> MoltResult {
    check_args(1, argv, 3, 3, "node dependency")?;
    let tail: Vec<&str> = argv
        .get(1..)
        .unwrap_or_default()
        .iter()
        .map(Value::as_str)
        .collect();
    wrap(get_shell(interp, ctx).cmd_dep(&tail))
}

fn tcl_show(interp: &mut Interp, ctx: ContextID, argv: &[Value]) -> MoltResult {
    check_args(1, argv, 1, 2, "?node?")?;
    let tail: Vec<&str> = argv
        .get(1..)
        .unwrap_or_default()
        .iter()
        .map(Value::as_str)
        .collect();
    wrap(get_shell(interp, ctx).cmd_show(&tail))
}

fn tcl_run(interp: &mut Interp, ctx: ContextID, argv: &[Value]) -> MoltResult {
    check_args(1, argv, 1, 0, "?options? ?node?")?;
    let tail: Vec<&str> = argv
        .get(1..)
        .unwrap_or_default()
        .iter()
        .map(Value::as_str)
        .collect();
    wrap(get_shell(interp, ctx).cmd_run(&tail))
}

fn tcl_join(interp: &mut Interp, ctx: ContextID, argv: &[Value]) -> MoltResult {
    check_args(1, argv, 2, 2, "node")?;
    let tail: Vec<&str> = argv
        .get(1..)
        .unwrap_or_default()
        .iter()
        .map(Value::as_str)
        .collect();
    wrap(get_shell(interp, ctx).cmd_join(&tail))
}

fn tcl_follow(interp: &mut Interp, ctx: ContextID, argv: &[Value]) -> MoltResult {
    check_args(1, argv, 2, 0, "node ?--color name?")?;
    let tail: Vec<&str> = argv
        .get(1..)
        .unwrap_or_default()
        .iter()
        .map(Value::as_str)
        .collect();
    wrap(get_shell(interp, ctx).cmd_follow(&tail))
}

fn tcl_cat(interp: &mut Interp, ctx: ContextID, argv: &[Value]) -> MoltResult {
    check_args(1, argv, 2, 2, "node")?;
    let tail: Vec<&str> = argv
        .get(1..)
        .unwrap_or_default()
        .iter()
        .map(Value::as_str)
        .collect();
    wrap(get_shell(interp, ctx).cmd_cat(&tail))
}

fn tcl_status(interp: &mut Interp, ctx: ContextID, argv: &[Value]) -> MoltResult {
    check_args(1, argv, 1, 2, "?node?")?;
    let tail: Vec<&str> = argv
        .get(1..)
        .unwrap_or_default()
        .iter()
        .map(Value::as_str)
        .collect();
    get_shell(interp, ctx).cmd_status(&tail);
    molt_ok!()
}

// Recursive eval of a file; interp is already borrowed by the caller's eval.
fn tcl_source(interp: &mut Interp, _ctx: ContextID, argv: &[Value]) -> MoltResult {
    check_args(1, argv, 2, 2, "file")?;
    let content = std::fs::read_to_string(argv.get(1).map_or("", Value::as_str))
        .map_err(|e| Exception::molt_err(Value::from(e.to_string())))?;
    interp.eval(&content)
}

fn tcl_suspend(interp: &mut Interp, ctx: ContextID, argv: &[Value]) -> MoltResult {
    check_args(1, argv, 2, 2, "node")?;
    let tail: Vec<&str> = argv
        .get(1..)
        .unwrap_or_default()
        .iter()
        .map(Value::as_str)
        .collect();
    wrap(get_shell(interp, ctx).cmd_suspend(&tail))
}

fn tcl_resume(interp: &mut Interp, ctx: ContextID, argv: &[Value]) -> MoltResult {
    check_args(1, argv, 2, 2, "node")?;
    let tail: Vec<&str> = argv
        .get(1..)
        .unwrap_or_default()
        .iter()
        .map(Value::as_str)
        .collect();
    wrap(get_shell(interp, ctx).cmd_resume(&tail))
}

fn tcl_wait(interp: &mut Interp, ctx: ContextID, argv: &[Value]) -> MoltResult {
    check_args(1, argv, 2, 0, "condition ?args?")?;
    let tail: Vec<&str> = argv
        .get(1..)
        .unwrap_or_default()
        .iter()
        .map(Value::as_str)
        .collect();
    wrap(get_shell(interp, ctx).cmd_wait(&tail))
}

fn tcl_write(interp: &mut Interp, ctx: ContextID, argv: &[Value]) -> MoltResult {
    check_args(1, argv, 2, 0, "node ?data?")?;
    let tail: Vec<&str> = argv
        .get(1..)
        .unwrap_or_default()
        .iter()
        .map(Value::as_str)
        .collect();
    wrap(get_shell(interp, ctx).cmd_write(&tail))
}

fn tcl_close(interp: &mut Interp, ctx: ContextID, argv: &[Value]) -> MoltResult {
    check_args(1, argv, 2, 2, "node")?;
    let tail: Vec<&str> = argv
        .get(1..)
        .unwrap_or_default()
        .iter()
        .map(Value::as_str)
        .collect();
    wrap(get_shell(interp, ctx).cmd_close(&tail))
}

fn tcl_kill(interp: &mut Interp, ctx: ContextID, argv: &[Value]) -> MoltResult {
    check_args(1, argv, 2, 3, "?-N? node")?;
    let tail: Vec<&str> = argv
        .get(1..)
        .unwrap_or_default()
        .iter()
        .map(Value::as_str)
        .collect();
    wrap(get_shell(interp, ctx).cmd_kill(&tail))
}

fn tcl_help(interp: &mut Interp, ctx: ContextID, argv: &[Value]) -> MoltResult {
    check_args(1, argv, 1, 1, "")?;
    get_shell(interp, ctx).cmd_help();
    molt_ok!()
}

fn tcl_dag(interp: &mut Interp, ctx: ContextID, argv: &[Value]) -> MoltResult {
    check_args(1, argv, 3, 3, "exists|handle <name>")?;
    let tail: Vec<&str> = argv
        .get(1..)
        .unwrap_or_default()
        .iter()
        .map(Value::as_str)
        .collect();
    match get_shell(interp, ctx).cmd_dag(&tail) {
        Ok(result) => Ok(Value::from(result)),
        Err(e) => Err(Exception::molt_err(Value::from(e))),
    }
}

fn tcl_quit(interp: &mut Interp, ctx: ContextID, argv: &[Value]) -> MoltResult {
    check_args(1, argv, 1, 1, "")?;
    interp.context::<ShellContext>(ctx).exit_requested = true;
    // Raise an error to unwind the running script. exit_requested is a side-channel
    // that TCL `catch` cannot reach, so `quit` always exits even inside `catch { quit }`.
    Err(Exception::molt_err("exit".into()))
}
