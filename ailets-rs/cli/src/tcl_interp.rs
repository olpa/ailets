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
// Interpreter factory
// ---------------------------------------------------------------------------

pub(crate) fn make_interp() -> (Interp, ContextID) {
    let mut interp = Interp::new();
    let ctx = interp.save_context(ShellContext {
        shell: std::ptr::null_mut(),
        exit_requested: false,
    });
    interp.add_context_command("node", tcl_node, ctx);
    interp.add_context_command("dep", tcl_dep, ctx);
    interp.add_context_command("deps", tcl_deps, ctx);
    interp.add_context_command("show", tcl_show, ctx);
    interp.add_context_command("run", tcl_run, ctx);
    interp.add_context_command("join", tcl_join, ctx);
    interp.add_context_command("await", tcl_join, ctx);
    interp.add_context_command("follow", tcl_follow, ctx);
    interp.add_context_command("cat", tcl_cat, ctx);
    interp.add_context_command("status", tcl_status, ctx);
    interp.add_context_command("source", tcl_source, ctx);
    interp.add_context_command("load", tcl_source, ctx);
    interp.add_context_command("suspend", tcl_suspend, ctx);
    interp.add_context_command("resume", tcl_resume, ctx);
    interp.add_context_command("wait", tcl_wait, ctx);
    interp.add_context_command("write", tcl_write, ctx);
    interp.add_context_command("close", tcl_close, ctx);
    interp.add_context_command("kill", tcl_kill, ctx);
    interp.add_context_command("help", tcl_help, ctx);
    interp.add_context_command("?", tcl_help, ctx);
    interp.add_context_command("quit", tcl_quit, ctx);
    interp.add_context_command("exit", tcl_quit, ctx);
    interp.add_context_command("q", tcl_quit, ctx);
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
