use std::collections::HashSet;

use ailetos::dag::{Dag, NodeKind};
use ailetos::idgen::Handle;

pub struct Scheduler<'a> {
    dag: &'a Dag,
    target: Handle,
}

impl<'a> Scheduler<'a> {
    pub fn new(dag: &'a Dag, target: Handle) -> Self {
        Self { dag, target }
    }

    /// Returns iterator over nodes needed to build target (topological order).
    /// Dependencies are yielded before dependents.
    pub fn iter(&self) -> impl Iterator<Item = Handle> + '_ {
        SchedulerIter::new(self.dag, self.target)
    }
}

struct SchedulerIter<'a> {
    dag: &'a Dag,
    stack: Vec<Handle>,
    visited: HashSet<Handle>,
    result: Vec<Handle>,
    done: bool,
}

impl<'a> SchedulerIter<'a> {
    fn new(dag: &'a Dag, target: Handle) -> Self {
        Self {
            dag,
            stack: vec![target],
            visited: HashSet::new(),
            result: Vec::new(),
            done: false,
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
        // Pop from front (drain in order)
        if self.result.is_empty() {
            None
        } else {
            Some(self.result.remove(0))
        }
    }
}
