use std::sync::{Arc, Condvar, Mutex};

use dagsh::{make_interp, DagShell, OutputSink};

/// Run `f` in a background thread; fail if it doesn't finish within `secs` seconds.
fn assert_completes_within<F>(f: F, secs: u64)
where
    F: FnOnce() + Send + 'static,
{
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    std::thread::spawn(move || {
        f();
        let _ = tx.send(());
    });
    assert!(
        rx.recv_timeout(std::time::Duration::from_secs(secs))
            .is_ok(),
        "operation timed out after {secs}s — likely hung"
    );
}

// shared helper so we can re-use CapturingSink for both command and notification sinks

struct CapturingSink {
    inner: Arc<(Mutex<Vec<String>>, Condvar)>,
}

impl CapturingSink {
    fn new() -> Self {
        Self {
            inner: Arc::new((Mutex::new(Vec::new()), Condvar::new())),
        }
    }

    fn lines(&self) -> Vec<String> {
        self.inner.0.lock().unwrap().clone()
    }

    fn wait_for_line(&self, predicate: impl Fn(&[String]) -> bool, timeout_secs: u64) -> bool {
        let (lock, cvar) = &*self.inner;
        let guard = lock.lock().unwrap();
        let (_guard, timed_out) = cvar
            .wait_timeout_while(
                guard,
                std::time::Duration::from_secs(timeout_secs),
                |lines| !predicate(lines),
            )
            .unwrap();
        !timed_out.timed_out()
    }
}

impl Clone for CapturingSink {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl OutputSink for CapturingSink {
    fn print(&self, text: &str) {
        let (lock, cvar) = &*self.inner;
        let mut lines = lock.lock().unwrap();
        if let Some(last) = lines.last_mut() {
            last.push_str(text);
        } else {
            lines.push(text.to_string());
        }
        cvar.notify_all();
    }

    fn println(&self, line: &str) {
        let (lock, cvar) = &*self.inner;
        lock.lock().unwrap().push(line.to_string());
        cvar.notify_all();
    }
}

#[test]
fn execute_routes_output_through_sink() {
    let sink = CapturingSink::new();
    let mut shell = DagShell::new_with_sink(Box::new(sink.clone()));
    let (mut interp, ctx) = make_interp();
    shell.execute(&mut interp, ctx, "help").unwrap();
    let lines = sink.lines();
    assert!(lines.iter().any(|l| l.contains("Node Management")));
}

#[test]
fn show_does_not_repeat_global_param_on_unrelated_node() {
    let sink = CapturingSink::new();
    let mut shell = DagShell::new_with_sink(Box::new(sink.clone()));
    let (mut interp, ctx) = make_interp();
    shell
        .execute(&mut interp, ctx, "set a [value hello]")
        .unwrap();
    shell
        .execute(&mut interp, ctx, "set b [value world]")
        .unwrap();
    shell.execute(&mut interp, ctx, "param -g GLOBAL g").unwrap();
    shell
        .execute(&mut interp, ctx, "param $a LOCAL local-only")
        .unwrap();
    shell.execute(&mut interp, ctx, "show").unwrap();

    let output = sink.lines().join("\n");
    assert!(
        !output.contains("GLOBAL=g"),
        "global param should not be echoed per-node, got: {output:?}"
    );
    assert!(
        output.contains("LOCAL=local-only"),
        "per-actor param should still be shown, got: {output:?}"
    );
}

#[test]
fn run_completes_on_persistent_executor() {
    let sink = CapturingSink::new();
    let mut shell = DagShell::new_with_sink(Box::new(sink.clone()));
    let (mut interp, ctx) = make_interp();
    // value "hello" → cat (foreground run should block until terminated)
    shell
        .execute(&mut interp, ctx, "set v [value hello]")
        .unwrap();
    shell.execute(&mut interp, ctx, "set c [node cat]").unwrap();
    shell.execute(&mut interp, ctx, "dep $c $v").unwrap();
    shell.execute(&mut interp, ctx, "run $c").unwrap();
    shell.execute(&mut interp, ctx, "status $c").unwrap();
    let lines = sink.lines();
    assert!(lines.iter().any(|l| l.contains("built")));
}

#[test]
fn multiple_bg_runs_are_allowed() {
    let notification_sink = Arc::new(CapturingSink::new());
    let mut shell = DagShell::new_with_sinks(
        Box::new(CapturingSink::new()),
        Arc::clone(&notification_sink) as Arc<dyn OutputSink>,
    );
    let (mut interp, ctx) = make_interp();
    shell
        .execute(&mut interp, ctx, "set v1 [value alpha]")
        .unwrap();
    shell
        .execute(&mut interp, ctx, "set c1 [node cat]")
        .unwrap();
    shell.execute(&mut interp, ctx, "dep $c1 $v1").unwrap();
    shell.execute(&mut interp, ctx, "run $c1 --bg").unwrap();
    // second background run must not fail with "already running"
    shell
        .execute(&mut interp, ctx, "set v2 [value beta]")
        .unwrap();
    shell
        .execute(&mut interp, ctx, "set c2 [node cat]")
        .unwrap();
    shell.execute(&mut interp, ctx, "dep $c2 $v2").unwrap();
    shell.execute(&mut interp, ctx, "run $c2 --bg").unwrap();
    assert!(
        notification_sink.wait_for_line(
            |lines| lines.iter().filter(|l| l.contains("done")).count() >= 2,
            5,
        ),
        "timeout: expected 2 'done' notifications; lines: {:?}",
        notification_sink.lines()
    );
}

#[test]
fn run_alias_completes() {
    let sink = CapturingSink::new();
    let mut shell = DagShell::new_with_sink(Box::new(sink.clone()));
    let (mut interp, ctx) = make_interp();
    shell
        .execute(&mut interp, ctx, "set v [value hello]")
        .unwrap();
    shell.execute(&mut interp, ctx, "set c [node cat]").unwrap();
    shell.execute(&mut interp, ctx, "dep $c $v").unwrap();
    shell
        .execute(&mut interp, ctx, "set end [alias .end $c]")
        .unwrap();
    shell.execute(&mut interp, ctx, "run $end").unwrap(); // must not hang
    shell.execute(&mut interp, ctx, "status $c").unwrap();
    let lines = sink.lines();
    assert!(lines.iter().any(|l| l.contains("built")));
}

#[test]
fn two_follows_both_receive_output() {
    let notification_sink = Arc::new(CapturingSink::new());
    let mut shell = DagShell::new_with_sinks(
        Box::new(CapturingSink::new()),
        Arc::clone(&notification_sink) as Arc<dyn OutputSink>,
    );
    let (mut interp, ctx) = make_interp();
    shell
        .execute(&mut interp, ctx, "set v [value hello]")
        .unwrap();
    shell.execute(&mut interp, ctx, "set c [node cat]").unwrap();
    shell.execute(&mut interp, ctx, "dep $c $v").unwrap();
    shell.execute(&mut interp, ctx, "follow $c").unwrap();
    shell.execute(&mut interp, ctx, "follow $c").unwrap();
    shell.execute(&mut interp, ctx, "run $c").unwrap();

    // Both followers write to the shared notification sink — "hello" must appear twice.
    let combined = notification_sink.lines().join("");
    let count = combined.matches("hello").count();
    assert_eq!(
        count, 2,
        "expected 'hello' twice in notification output, got: {combined:?}"
    );
}

#[test]
fn background_termination_is_notified() {
    // Value nodes are pre-terminated (no actor runs), so use value → cat so
    // that cat actually spawns and produces a NodeTerminated event.
    let notification_sink = Arc::new(CapturingSink::new());
    let mut shell = DagShell::new_with_sinks(
        Box::new(CapturingSink::new()),
        Arc::clone(&notification_sink) as Arc<dyn OutputSink>,
    );
    let (mut interp, ctx) = make_interp();
    shell
        .execute(&mut interp, ctx, "set v [value hello]")
        .unwrap();
    shell.execute(&mut interp, ctx, "set c [node cat]").unwrap();
    shell.execute(&mut interp, ctx, "dep $c $v").unwrap();
    shell.execute(&mut interp, ctx, "run $c --bg").unwrap();
    assert!(
        notification_sink.wait_for_line(|lines| lines.iter().any(|l| l.contains("done")), 5,),
        "timeout: no 'done' notification; lines: {:?}",
        notification_sink.lines()
    );
}

#[test]
fn one_step_runs_first_pending_actor() {
    // v1 → cat2 → cat3: `run --one-step` must return (not hang) and run exactly cat2.
    let sink = CapturingSink::new();
    let mut shell = DagShell::new_with_sink(Box::new(sink.clone()));
    let (mut interp, ctx) = make_interp();
    shell
        .execute(&mut interp, ctx, "set v1 [value hello]")
        .unwrap();
    shell
        .execute(&mut interp, ctx, "set cat2 [node cat]")
        .unwrap();
    shell.execute(&mut interp, ctx, "dep $cat2 $v1").unwrap();
    shell
        .execute(&mut interp, ctx, "set cat3 [node cat]")
        .unwrap();
    shell.execute(&mut interp, ctx, "dep $cat3 $cat2").unwrap();
    shell.execute(&mut interp, ctx, "run --one-step").unwrap(); // must not hang
    shell.execute(&mut interp, ctx, "status").unwrap();
    let lines = sink.lines();
    // v1 pre-terminated + cat2 just ran = 2 terminated; cat3 must not have run.
    assert!(
        lines.iter().any(|l| l.contains("2 terminated")),
        "expected 2 terminated after one step; lines: {lines:?}"
    );
}

#[test]
fn one_step_advances_past_terminated_nodes() {
    // Second `run --one-step` must skip already-terminated nodes and run cat3.
    let sink = CapturingSink::new();
    let mut shell = DagShell::new_with_sink(Box::new(sink.clone()));
    let (mut interp, ctx) = make_interp();
    shell
        .execute(&mut interp, ctx, "set v1 [value hello]")
        .unwrap();
    shell
        .execute(&mut interp, ctx, "set cat2 [node cat]")
        .unwrap();
    shell.execute(&mut interp, ctx, "dep $cat2 $v1").unwrap();
    shell
        .execute(&mut interp, ctx, "set cat3 [node cat]")
        .unwrap();
    shell.execute(&mut interp, ctx, "dep $cat3 $cat2").unwrap();
    shell.execute(&mut interp, ctx, "run --one-step").unwrap(); // runs cat2
    shell.execute(&mut interp, ctx, "run --one-step").unwrap(); // must not hang; runs cat3
    shell.execute(&mut interp, ctx, "status").unwrap();
    let lines = sink.lines();
    // All three nodes terminated after two steps.
    assert!(
        lines
            .iter()
            .any(|l| l.contains("0 pending") && l.contains("3 terminated")),
        "expected 0 pending, 3 terminated after two steps; lines: {lines:?}"
    );
}

#[test]
fn foreground_run_suppresses_intermediate_notifications() {
    // Intermediate nodes in a foreground pipeline must not emit notifications.
    // The terminal node's notification may race with foreground_join being cleared
    // (see warning in NotificationWatcher::spawn); we accept that known race and
    // only assert that intermediate notifications are suppressed.
    let notification_sink = Arc::new(CapturingSink::new());
    let mut shell = DagShell::new_with_sinks(
        Box::new(CapturingSink::new()),
        Arc::clone(&notification_sink) as Arc<dyn OutputSink>,
    );
    let (mut interp, ctx) = make_interp();
    shell
        .execute(&mut interp, ctx, "set v [value hello]")
        .unwrap();
    shell
        .execute(&mut interp, ctx, "set c1 [node cat]")
        .unwrap();
    shell.execute(&mut interp, ctx, "dep $c1 $v").unwrap();
    shell
        .execute(&mut interp, ctx, "set c2 [node cat]")
        .unwrap();
    shell.execute(&mut interp, ctx, "dep $c2 $c1").unwrap();
    shell
        .execute(&mut interp, ctx, "set c3 [node cat]")
        .unwrap();
    shell.execute(&mut interp, ctx, "dep $c3 $c2").unwrap();
    shell
        .execute(&mut interp, ctx, "set c4 [node cat]")
        .unwrap();
    shell.execute(&mut interp, ctx, "dep $c4 $c3").unwrap();
    shell.execute(&mut interp, ctx, "run $c4").unwrap();

    let c4_id = interp
        .eval("set c4")
        .map(|v| v.as_str().to_string())
        .unwrap_or_default();
    let terminal_line = format!("[cat#{}] done", c4_id);
    let mut lines = notification_sink.lines();
    lines.retain(|l| l != &terminal_line);
    assert!(
        lines.is_empty(),
        "unexpected intermediate notifications during foreground run: {:?}",
        lines
    );
}

#[test]
fn run_stop_after_multialias_does_not_hang() {
    // v → cat_b → cat_c (chain); alias resolves to [cat_b, cat_c].
    // --stop-after cat_b stops the executor before cat_c runs.
    // join_handles must not wait for cat_c (which stays NotStarted).
    assert_completes_within(
        || {
            let sink = CapturingSink::new();
            let mut shell = DagShell::new_with_sink(Box::new(sink));
            let (mut interp, ctx) = make_interp();
            shell
                .execute(&mut interp, ctx, "set v [value hello]")
                .unwrap();
            shell
                .execute(&mut interp, ctx, "set cat_b [node cat]")
                .unwrap();
            shell.execute(&mut interp, ctx, "dep $cat_b $v").unwrap();
            shell
                .execute(&mut interp, ctx, "set cat_c [node cat]")
                .unwrap();
            shell
                .execute(&mut interp, ctx, "dep $cat_c $cat_b")
                .unwrap();
            shell
                .execute(&mut interp, ctx, "set alias [alias .end $cat_b $cat_c]")
                .unwrap();
            shell
                .execute(&mut interp, ctx, "run --stop-after $cat_b $alias")
                .unwrap();
        },
        3,
    );
}

#[test]
fn run_one_step_multialias_does_not_hang() {
    // alias resolves to [cat_a, cat_b] where cat_b depends on cat_a.
    // --one-step runs cat_a only; join_handles must not wait for cat_b.
    assert_completes_within(
        || {
            let sink = CapturingSink::new();
            let mut shell = DagShell::new_with_sink(Box::new(sink));
            let (mut interp, ctx) = make_interp();
            shell
                .execute(&mut interp, ctx, "set v [value hello]")
                .unwrap();
            shell
                .execute(&mut interp, ctx, "set cat_a [node cat]")
                .unwrap();
            shell.execute(&mut interp, ctx, "dep $cat_a $v").unwrap();
            shell
                .execute(&mut interp, ctx, "set cat_b [node cat]")
                .unwrap();
            shell
                .execute(&mut interp, ctx, "dep $cat_b $cat_a")
                .unwrap();
            shell
                .execute(&mut interp, ctx, "set alias [alias .end $cat_a $cat_b]")
                .unwrap();
            shell
                .execute(&mut interp, ctx, "run --one-step $alias")
                .unwrap();
        },
        3,
    );
}

#[test]
fn join_nonexistent_returns_error() {
    // executor.join() resolves Err immediately for nodes not in the DAG,
    // so join_handles returns without polling.
    let mut shell = DagShell::new_with_sink(Box::new(CapturingSink::new()));
    let (mut interp, ctx) = make_interp();
    let result = shell.execute(&mut interp, ctx, "join 99999");
    assert!(
        result.is_err(),
        "expected error for non-existent handle, got Ok"
    );
}

#[test]
fn run_stop_after_alias_handle_does_not_hang() {
    // When --stop-after receives an alias handle, attach_stdout_for_run must
    // resolve it to the concrete node before attaching the stdout reader.
    // Without resolution the reader waits on the alias node's non-existent
    // stdout pipe; prepare_exit then hangs draining the stuck reader task.
    assert_completes_within(
        || {
            let sink = CapturingSink::new();
            let mut shell = DagShell::new_with_sink(Box::new(sink));
            let (mut interp, ctx) = make_interp();
            shell
                .execute(&mut interp, ctx, "set v [value hello]")
                .unwrap();
            shell
                .execute(&mut interp, ctx, "set cat [node cat]")
                .unwrap();
            shell.execute(&mut interp, ctx, "dep $cat $v").unwrap();
            shell
                .execute(&mut interp, ctx, "set alias [alias .end $cat]")
                .unwrap();
            shell
                .execute(&mut interp, ctx, "run --stop-after $alias $cat")
                .unwrap();
            // shell drop drains reader_tasks; hangs if reader is on alias's pipe
        },
        5,
    );
}

#[test]
fn run_stop_before_does_not_hang() {
    // When no explicit target is given, handle is set to the stop_before node.
    // The executor stops before that node, so it never terminates; join_handles
    // must not wait for it.
    assert_completes_within(
        || {
            let sink = CapturingSink::new();
            let mut shell = DagShell::new_with_sink(Box::new(sink));
            let (mut interp, ctx) = make_interp();
            shell
                .execute(&mut interp, ctx, "set v [value hello]")
                .unwrap();
            shell.execute(&mut interp, ctx, "set b [node cat]").unwrap();
            shell.execute(&mut interp, ctx, "dep $b $v").unwrap();
            shell.execute(&mut interp, ctx, "set c [node cat]").unwrap();
            shell.execute(&mut interp, ctx, "dep $c $b").unwrap();
            shell
                .execute(&mut interp, ctx, "run --stop-before $c")
                .unwrap();
        },
        3,
    );
}

#[test]
fn tcl_multiline_value_sets_node() {
    let sink = CapturingSink::new();
    let mut shell = DagShell::new_with_sink(Box::new(sink.clone()));
    let (mut interp, ctx) = make_interp();
    shell
        .execute(&mut interp, ctx, "set v [value {hello world}]\nstatus $v")
        .unwrap();
    let lines = sink.lines();
    assert!(
        lines.iter().any(|l| l.contains("value")),
        "expected node status output; got: {lines:?}"
    );
}

#[test]
fn parse_handle_name_dot_n() {
    let sink = CapturingSink::new();
    let mut shell = DagShell::new_with_sink(Box::new(sink.clone()));
    let (mut interp, ctx) = make_interp();
    // node cat gets id 1; cat.1 must resolve, wrong.1 must not
    shell
        .execute(&mut interp, ctx, "set id [node cat]")
        .unwrap();
    shell.execute(&mut interp, ctx, "status cat.$id").unwrap();
    assert!(sink.lines().iter().any(|l| l.contains("cat")));
    shell.execute(&mut interp, ctx, "status wrong.$id").unwrap();
    assert!(sink
        .lines()
        .iter()
        .any(|l| l.contains("Invalid handle: wrong.")));
}

#[test]
fn parse_handle_unique_name() {
    let sink = CapturingSink::new();
    let mut shell = DagShell::new_with_sink(Box::new(sink.clone()));
    let (mut interp, ctx) = make_interp();
    // single cat resolves; adding a second makes it ambiguous
    shell.execute(&mut interp, ctx, "node cat").unwrap();
    shell.execute(&mut interp, ctx, "status cat").unwrap();
    assert!(sink.lines().iter().any(|l| l.contains("cat")));
    shell.execute(&mut interp, ctx, "node cat").unwrap();
    shell.execute(&mut interp, ctx, "status cat").unwrap();
    assert!(sink
        .lines()
        .iter()
        .any(|l| l.contains("Invalid handle: cat")));
}

#[test]
fn cat_stream_by_name_and_fd() {
    let sink = CapturingSink::new();
    let mut shell = DagShell::new_with_sink(Box::new(sink.clone()));
    let (mut interp, ctx) = make_interp();
    shell
        .execute(&mut interp, ctx, "set v [value hello]")
        .unwrap();
    shell.execute(&mut interp, ctx, "cat $v:stderr").unwrap();
    shell.execute(&mut interp, ctx, "cat $v:2").unwrap();
    let lines = sink.lines();
    assert_eq!(
        lines
            .iter()
            .filter(|l| l.contains("No output on stream"))
            .count(),
        2
    );
}
