//! Molt (TCL) integration — register DAG shell commands with a molt `Interp`.

use molt::{check_args, molt_ok, types::*, Interp};

use crate::commands::{
    ENTRY_CAT, ENTRY_CLOSE, ENTRY_DEP, ENTRY_DEPS, ENTRY_FOLLOW, ENTRY_HELP, ENTRY_JOIN,
    ENTRY_KILL, ENTRY_NODE, ENTRY_QUIT, ENTRY_RESUME, ENTRY_RUN, ENTRY_SHOW, ENTRY_SOURCE,
    ENTRY_STATUS, ENTRY_SUSPEND, ENTRY_WAIT, ENTRY_WRITE,
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
// Interpreter factory — pairs each CommandMeta with its TCL handler.
// ---------------------------------------------------------------------------

pub(crate) fn make_interp() -> (Interp, ContextID) {
    let mut interp = Interp::new();
    let ctx = interp.save_context(ShellContext {
        shell: std::ptr::null_mut(),
        exit_requested: false,
    });
    let bindings: &[(&crate::commands::CommandMeta, CommandFunc)] = &[
        (&ENTRY_NODE, tcl_node),
        (&ENTRY_DEP, tcl_dep),
        (&ENTRY_DEPS, tcl_deps),
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
