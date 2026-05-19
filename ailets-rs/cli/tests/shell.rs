use std::sync::{Arc, Mutex};

use dagsh::{DagShell, OutputSink};

// shared helper so we can re-use CapturingSink for both command and notification sinks



struct CapturingSink {
    lines: Arc<Mutex<Vec<String>>>,
}

impl CapturingSink {
    fn new() -> Self {
        Self {
            lines: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn lines(&self) -> Vec<String> {
        self.lines.lock().unwrap().clone()
    }
}

impl Clone for CapturingSink {
    fn clone(&self) -> Self {
        Self {
            lines: Arc::clone(&self.lines),
        }
    }
}

impl OutputSink for CapturingSink {
    fn print(&self, text: &str) {
        let mut lines = self.lines.lock().unwrap();
        if let Some(last) = lines.last_mut() {
            last.push_str(text);
        } else {
            lines.push(text.to_string());
        }
    }

    fn println(&self, line: &str) {
        self.lines.lock().unwrap().push(line.to_string());
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
    let sink = CapturingSink::new();
    let mut shell = DagShell::new_with_sink(Box::new(sink.clone()));
    shell.execute("set a = node value alpha").unwrap();
    shell.execute("run $a --bg").unwrap();
    // second background run must not fail with "already running"
    shell.execute("set b = node value beta").unwrap();
    shell.execute("run $b --bg").unwrap();
    let lines = sink.lines();
    assert_eq!(lines.iter().filter(|l| l.contains("background run")).count(), 2);
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
    // Poll until the notification arrives (up to 5 s).
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        std::thread::sleep(std::time::Duration::from_millis(50));
        if notification_sink.lines().iter().any(|l| l.contains("done")) {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "timeout: no 'done' notification; lines: {:?}",
            notification_sink.lines()
        );
    }
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
    shell.execute("run $c").unwrap(); // foreground
    // Give the watcher a moment to flush any stray events.
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(
        notification_sink.lines().is_empty(),
        "unexpected notifications during foreground run: {:?}",
        notification_sink.lines()
    );
}
