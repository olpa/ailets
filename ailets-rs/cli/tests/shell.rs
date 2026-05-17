use std::sync::{Arc, Mutex};

use dagsh::{DagShell, OutputSink};

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
