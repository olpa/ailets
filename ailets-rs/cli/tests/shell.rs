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
