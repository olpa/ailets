use std::sync::{Arc, Condvar, Mutex};

use dagsh::{DagShell, OutputSink};

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
        rx.recv_timeout(std::time::Duration::from_secs(secs)).is_ok(),
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
    shell.execute("help").unwrap();
    let lines = sink.lines();
    assert!(lines.iter().any(|l| l.contains("Node Management")));
}

#[test]
fn run_completes_on_persistent_executor() {
    let sink = CapturingSink::new();
    let mut shell = DagShell::new_with_sink(Box::new(sink.clone()));
    // value "hello" → cat (foreground run should block until terminated)
    shell.execute("set v = node value hello").unwrap();
    shell.execute("set c = node add cat").unwrap();
    shell.execute("dep $c $v").unwrap();
    shell.execute("run $c").unwrap();
    shell.execute("status $c").unwrap();
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
    shell.execute("set v1 = node value alpha").unwrap();
    shell.execute("set c1 = node add cat").unwrap();
    shell.execute("dep $c1 $v1").unwrap();
    shell.execute("run $c1 --bg").unwrap();
    // second background run must not fail with "already running"
    shell.execute("set v2 = node value beta").unwrap();
    shell.execute("set c2 = node add cat").unwrap();
    shell.execute("dep $c2 $v2").unwrap();
    shell.execute("run $c2 --bg").unwrap();
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
    shell.execute("set v = node value hello").unwrap();
    shell.execute("set c = node add cat").unwrap();
    shell.execute("dep $c $v").unwrap();
    shell.execute("set end = node alias .end $c").unwrap();
    shell.execute("run $end").unwrap(); // must not hang
    shell.execute("status $c").unwrap();
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
    shell.execute("set v = node value hello").unwrap();
    shell.execute("set c = node add cat").unwrap();
    shell.execute("dep $c $v").unwrap();
    shell.execute("follow $c").unwrap();
    shell.execute("follow $c").unwrap();
    shell.execute("run $c").unwrap();

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
    shell.execute("set v = node value hello").unwrap();
    shell.execute("set c = node add cat").unwrap();
    shell.execute("dep $c $v").unwrap();
    shell.execute("run $c --bg").unwrap();
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
    shell.execute("set v1 = node value hello").unwrap();
    shell.execute("set cat2 = node add cat").unwrap();
    shell.execute("dep $cat2 $v1").unwrap();
    shell.execute("set cat3 = node add cat").unwrap();
    shell.execute("dep $cat3 $cat2").unwrap();
    shell.execute("run --one-step").unwrap(); // must not hang
    shell.execute("status").unwrap();
    let lines = sink.lines();
    // v1 pre-terminated + cat2 just ran = 2 terminated; cat3 still pending.
    assert!(
        lines
            .iter()
            .any(|l| l.contains("1 pending") && l.contains("2 terminated")),
        "expected 1 pending, 2 terminated after one step; lines: {lines:?}"
    );
}

#[test]
fn one_step_advances_past_terminated_nodes() {
    // Second `run --one-step` must skip already-terminated nodes and run cat3.
    let sink = CapturingSink::new();
    let mut shell = DagShell::new_with_sink(Box::new(sink.clone()));
    shell.execute("set v1 = node value hello").unwrap();
    shell.execute("set cat2 = node add cat").unwrap();
    shell.execute("dep $cat2 $v1").unwrap();
    shell.execute("set cat3 = node add cat").unwrap();
    shell.execute("dep $cat3 $cat2").unwrap();
    shell.execute("run --one-step").unwrap(); // runs cat2
    shell.execute("run --one-step").unwrap(); // must not hang; runs cat3
    shell.execute("status").unwrap();
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
    let notification_sink = Arc::new(CapturingSink::new());
    let mut shell = DagShell::new_with_sinks(
        Box::new(CapturingSink::new()),
        Arc::clone(&notification_sink) as Arc<dyn OutputSink>,
    );
    shell.execute("set v = node value hello").unwrap();
    shell.execute("set c = node add cat").unwrap();
    shell.execute("dep $c $v").unwrap();
    shell.execute("run $c").unwrap(); // foreground — watcher processes all events inside block_on
    assert!(
        notification_sink.lines().is_empty(),
        "unexpected notifications during foreground run: {:?}",
        notification_sink.lines()
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
            shell.execute("set v = node value hello").unwrap();
            shell.execute("set b = node add cat").unwrap();
            shell.execute("dep $b $v").unwrap();
            shell.execute("set c = node add cat").unwrap();
            shell.execute("dep $c $b").unwrap();
            shell.execute("run --stop-before $c").unwrap();
        },
        3,
    );
}
