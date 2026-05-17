//! DAG traversal — topological ordering and stop conditions.

use std::collections::HashSet;

use crate::dag::{Dag, NodeKind};
use crate::idgen::Handle;

/// Conditions for stopping DAG iteration
#[derive(Debug, Clone, Default)]
pub struct StopConditions {
    /// Execute only the first ready node, then stop
    pub one_step: bool,
    /// Stop before executing this node
    pub stop_before: Option<Handle>,
    /// Stop after executing this node
    pub stop_after: Option<Handle>,
}

/// Iterator that yields DAG nodes in topological order (dependencies before dependents).
///
/// On first `next()`, computes the full order into `result`. Then yields nodes
/// one by one via `result_index`. The `stopped` flag allows early termination.
///
/// The topological ordering is no longer relied upon by the spawn loop (which uses
/// `is_ready_to_spawn` to enforce dependency order dynamically). This iterator is
/// still used for two things: DAG traversal (discovering all nodes reachable from a
/// target) and honouring `StopConditions` (`one_step`, `stop_before`, `stop_after`).
pub struct TopologicalOrderIter<'a> {
    dag: &'a Dag,
    // (node, deps_pushed): when false, push deps then re-push with true;
    // when true, emit the node (post-order ensures deps come first).
    stack: Vec<(Handle, bool)>,
    visited: HashSet<Handle>,
    result: Vec<Handle>,
    result_index: usize,
    stopped: bool,
    stop_conditions: StopConditions,
}

impl<'a> TopologicalOrderIter<'a> {
    #[must_use]
    pub fn new(dag: &'a Dag, target: Handle) -> Self {
        Self::with_stop_conditions(dag, target, StopConditions::default())
    }

    #[must_use]
    pub fn with_stop_conditions(
        dag: &'a Dag,
        target: Handle,
        stop_conditions: StopConditions,
    ) -> Self {
        Self {
            dag,
            stack: vec![(target, false)],
            visited: HashSet::new(),
            result: Vec::new(),
            result_index: 0,
            stopped: false,
            stop_conditions,
        }
    }

    /// Build the full topological order using post-order DFS.
    /// Only concrete nodes are included; aliases are traversed but not yielded.
    fn build_order(&mut self) {
        while let Some((node, deps_pushed)) = self.stack.pop() {
            if deps_pushed {
                if let Some(node_info) = self.dag.get_node(node) {
                    if node_info.kind == NodeKind::Concrete {
                        self.result.push(node);
                    }
                }
                continue;
            }

            if !self.visited.insert(node) {
                continue;
            }

            // Re-push to emit after all deps are processed
            self.stack.push((node, true));

            for dep in self.dag.resolve_dependencies(node) {
                if !self.visited.contains(&dep) {
                    self.stack.push((dep, false));
                }
            }
        }
    }
}

impl Iterator for TopologicalOrderIter<'_> {
    type Item = Handle;

    fn next(&mut self) -> Option<Self::Item> {
        if !self.stack.is_empty() {
            self.build_order();
        }

        if self.stopped {
            return None;
        }

        let node = *self.result.get(self.result_index)?;
        self.result_index += 1;

        if self.stop_conditions.stop_before == Some(node) {
            self.stopped = true;
            return None;
        }

        if self.stop_conditions.one_step || self.stop_conditions.stop_after == Some(node) {
            self.stopped = true;
        }

        Some(node)
    }
}
