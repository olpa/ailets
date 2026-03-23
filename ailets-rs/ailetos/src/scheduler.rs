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

pub struct Scheduler<'a> {
    dag: &'a Dag,
    target: Handle,
    stop_conditions: StopConditions,
}

impl<'a> Scheduler<'a> {
    #[must_use]
    pub fn new(dag: &'a Dag, target: Handle) -> Self {
        Self {
            dag,
            target,
            stop_conditions: StopConditions::default(),
        }
    }

    #[must_use]
    pub fn with_stop_conditions(dag: &'a Dag, target: Handle, stop_conditions: StopConditions) -> Self {
        Self {
            dag,
            target,
            stop_conditions,
        }
    }

    /// Returns iterator over nodes needed to build target (topological order).
    /// Dependencies are yielded before dependents.
    pub fn iter(&self) -> impl Iterator<Item = Handle> + '_ {
        SchedulerIter::new(self.dag, self.target, self.stop_conditions.clone())
    }
}

struct SchedulerIter<'a> {
    dag: &'a Dag,
    stack: Vec<Handle>,
    visited: HashSet<Handle>,
    result: Vec<Handle>,
    done: bool,
    result_index: usize,
    stopped: bool,
    stop_conditions: StopConditions,
}

impl<'a> SchedulerIter<'a> {
    fn new(dag: &'a Dag, target: Handle, stop_conditions: StopConditions) -> Self {
        Self {
            dag,
            stack: vec![target],
            visited: HashSet::new(),
            result: Vec::new(),
            done: false,
            result_index: 0,
            stopped: false,
            stop_conditions,
        }
    }

    /// Build the full topological order, then drain it.
    /// Only concrete nodes are included; aliases are traversed but not yielded.
    fn build_order(&mut self) {
        // DFS-based topological sort
        while let Some(node) = self.stack.pop() {
            if self.visited.contains(&node) {
                continue;
            }
            self.visited.insert(node);

            let Some(node_info) = self.dag.get_node(node) else {
                continue;
            };

            // Get dependencies and push them to stack
            let deps: Vec<Handle> = self.dag.resolve_dependencies(node).collect();

            // Only add concrete nodes to result; skip aliases
            if node_info.kind == NodeKind::Concrete {
                self.result.push(node);
            }

            for dep in deps {
                if !self.visited.contains(&dep) {
                    self.stack.push(dep);
                }
            }
        }

        // Reverse to get dependencies before dependents
        self.result.reverse();
        self.done = true;
    }
}

impl Iterator for SchedulerIter<'_> {
    type Item = Handle;

    fn next(&mut self) -> Option<Self::Item> {
        if !self.done {
            self.build_order();
        }

        if self.stopped || self.result_index >= self.result.len() {
            return None;
        }

        let node = self.result[self.result_index];

        // Check stop_before - don't yield this node
        if self.stop_conditions.stop_before == Some(node) {
            self.stopped = true;
            return None;
        }

        self.result_index += 1;

        // Check one_step - stop after first node
        if self.stop_conditions.one_step {
            self.stopped = true;
        }

        // Check stop_after - yield but stop after
        if self.stop_conditions.stop_after == Some(node) {
            self.stopped = true;
        }

        Some(node)
    }
}
