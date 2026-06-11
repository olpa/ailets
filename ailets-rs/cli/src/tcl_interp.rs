//! Molt (TCL) integration — register DAG shell commands with a molt `Interp`.

use molt::{molt_ok, types::*, Interp};

use crate::DagShell;

// ---------------------------------------------------------------------------
// Thread-locals — active shell pointer and exit signal
// ---------------------------------------------------------------------------

thread_local! {
    // raw pointer to DagShell, valid only during DagShell::execute
    static CURRENT_SHELL: std::cell::Cell<*mut DagShell> =
        std::cell::Cell::new(std::ptr::null_mut());

    static EXIT_REQUESTED: std::cell::Cell<bool> = std::cell::Cell::new(false);
}

pub(crate) fn set_shell(shell: *mut DagShell) {
    CURRENT_SHELL.with(|c| c.set(shell));
}

pub(crate) fn clear_shell() {
    CURRENT_SHELL.with(|c| c.set(std::ptr::null_mut()));
}

pub(crate) fn take_exit_requested() -> bool {
    EXIT_REQUESTED.with(|c| c.replace(false))
}

// Safety: called only from command handlers invoked by DagShell::execute,
// which sets CURRENT_SHELL before calling interp.eval and clears it after.
// The pointer is valid for the entire duration of the eval call.
// Command handlers only access DagShell fields other than `tcl` (which was
// moved out of self before eval), so no aliasing of the same memory occurs.
fn get_shell<'a>() -> &'a mut DagShell {
    CURRENT_SHELL.with(|c| {
        let ptr = c.get();
        debug_assert!(!ptr.is_null(), "no active DagShell context");
        unsafe { &mut *ptr }
    })
}

fn wrap(r: Result<(), String>) -> MoltResult {
    r.map(|_| Value::empty())
        .map_err(|e| Exception::molt_err(Value::from(e)))
}

// ---------------------------------------------------------------------------
// Interpreter factory
// ---------------------------------------------------------------------------

pub(crate) fn make_interp() -> Interp {
    let mut interp = Interp::new();
    interp.add_command("node", tcl_node);
    interp.add_command("dep", tcl_dep);
    interp.add_command("deps", tcl_deps);
    interp.add_command("show", tcl_show);
    interp.add_command("run", tcl_run);
    interp.add_command("join", tcl_join);
    interp.add_command("await", tcl_join);
    interp.add_command("follow", tcl_follow);
    interp.add_command("cat", tcl_cat);
    interp.add_command("status", tcl_status);
    interp.add_command("source", tcl_source);
    interp.add_command("load", tcl_source);
    interp.add_command("suspend", tcl_suspend);
    interp.add_command("resume", tcl_resume);
    interp.add_command("wait", tcl_wait);
    interp.add_command("write", tcl_write);
    interp.add_command("close", tcl_close);
    interp.add_command("kill", tcl_kill);
    interp.add_command("help", tcl_help);
    interp.add_command("?", tcl_help);
    interp.add_command("quit", tcl_quit);
    interp.add_command("exit", tcl_quit);
    interp.add_command("q", tcl_quit);
    interp
}

// ---------------------------------------------------------------------------
// Command handlers
// ---------------------------------------------------------------------------

fn tcl_node(_interp: &mut Interp, _ctx: ContextID, argv: &[Value]) -> MoltResult {
    let shell = get_shell();
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

fn tcl_dep(_interp: &mut Interp, _ctx: ContextID, argv: &[Value]) -> MoltResult {
    let args: Vec<&str> = argv[1..].iter().map(|v| v.as_str()).collect();
    wrap(get_shell().cmd_dep(&args))
}

fn tcl_deps(_interp: &mut Interp, _ctx: ContextID, argv: &[Value]) -> MoltResult {
    let args: Vec<&str> = argv[1..].iter().map(|v| v.as_str()).collect();
    wrap(get_shell().cmd_deps(&args))
}

fn tcl_show(_interp: &mut Interp, _ctx: ContextID, argv: &[Value]) -> MoltResult {
    let args: Vec<&str> = argv[1..].iter().map(|v| v.as_str()).collect();
    wrap(get_shell().cmd_show(&args))
}

fn tcl_run(_interp: &mut Interp, _ctx: ContextID, argv: &[Value]) -> MoltResult {
    let args: Vec<&str> = argv[1..].iter().map(|v| v.as_str()).collect();
    wrap(get_shell().cmd_run(&args))
}

fn tcl_join(_interp: &mut Interp, _ctx: ContextID, argv: &[Value]) -> MoltResult {
    let args: Vec<&str> = argv[1..].iter().map(|v| v.as_str()).collect();
    wrap(get_shell().cmd_join(&args))
}

fn tcl_follow(_interp: &mut Interp, _ctx: ContextID, argv: &[Value]) -> MoltResult {
    let args: Vec<&str> = argv[1..].iter().map(|v| v.as_str()).collect();
    wrap(get_shell().cmd_follow(&args))
}

fn tcl_cat(_interp: &mut Interp, _ctx: ContextID, argv: &[Value]) -> MoltResult {
    let args: Vec<&str> = argv[1..].iter().map(|v| v.as_str()).collect();
    wrap(get_shell().cmd_cat(&args))
}

fn tcl_status(_interp: &mut Interp, _ctx: ContextID, argv: &[Value]) -> MoltResult {
    let args: Vec<&str> = argv[1..].iter().map(|v| v.as_str()).collect();
    get_shell().cmd_status(&args);
    molt_ok!()
}

// Recursive eval of a file; interp is already borrowed by the caller's eval.
fn tcl_source(interp: &mut Interp, _ctx: ContextID, argv: &[Value]) -> MoltResult {
    let path = argv
        .get(1)
        .ok_or_else(|| Exception::molt_err("usage: source <file>".into()))?;
    let content = std::fs::read_to_string(path.as_str())
        .map_err(|e| Exception::molt_err(Value::from(e.to_string())))?;
    interp.eval(&content)
}

fn tcl_suspend(_interp: &mut Interp, _ctx: ContextID, argv: &[Value]) -> MoltResult {
    let args: Vec<&str> = argv[1..].iter().map(|v| v.as_str()).collect();
    wrap(get_shell().cmd_suspend(&args))
}

fn tcl_resume(_interp: &mut Interp, _ctx: ContextID, argv: &[Value]) -> MoltResult {
    let args: Vec<&str> = argv[1..].iter().map(|v| v.as_str()).collect();
    wrap(get_shell().cmd_resume(&args))
}

fn tcl_wait(_interp: &mut Interp, _ctx: ContextID, argv: &[Value]) -> MoltResult {
    let args: Vec<&str> = argv[1..].iter().map(|v| v.as_str()).collect();
    wrap(get_shell().cmd_wait(&args))
}

fn tcl_write(_interp: &mut Interp, _ctx: ContextID, argv: &[Value]) -> MoltResult {
    let args: Vec<&str> = argv[1..].iter().map(|v| v.as_str()).collect();
    wrap(get_shell().cmd_write(&args))
}

fn tcl_close(_interp: &mut Interp, _ctx: ContextID, argv: &[Value]) -> MoltResult {
    let args: Vec<&str> = argv[1..].iter().map(|v| v.as_str()).collect();
    wrap(get_shell().cmd_close(&args))
}

fn tcl_kill(_interp: &mut Interp, _ctx: ContextID, argv: &[Value]) -> MoltResult {
    let args: Vec<&str> = argv[1..].iter().map(|v| v.as_str()).collect();
    wrap(get_shell().cmd_kill(&args))
}

fn tcl_help(_interp: &mut Interp, _ctx: ContextID, _argv: &[Value]) -> MoltResult {
    get_shell().cmd_help();
    molt_ok!()
}

fn tcl_quit(_interp: &mut Interp, _ctx: ContextID, _argv: &[Value]) -> MoltResult {
    EXIT_REQUESTED.with(|c| c.set(true));
    // Return an error to unwind the current script; execute() converts this to ShellControl::Exit.
    Err(Exception::molt_err("exit".into()))
}
