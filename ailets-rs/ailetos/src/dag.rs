use std::collections::HashSet;
use std::fmt::Write;
use std::sync::Arc;

use crate::idgen::{Handle, IdGen};

/// Wrapper for the dependent node in `add_dependency(For(A), DependsOn(B))`.
/// Reads as: "for node A, add dependency on B".
#[derive(Clone, Copy)]
pub struct For(pub Handle);

/// Wrapper for the dependency node in `add_dependency(For(A), DependsOn(B))`.
/// Reads as: "A depends on B".
#[derive(Clone, Copy)]
pub struct DependsOn(pub Handle);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeState {
    NotStarted,
    Running,
    Terminated,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    Concrete,
    Alias,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub pid: Handle,
    pub idname: String,
    pub kind: NodeKind,
    pub state: NodeState,
}

#[derive(Debug)]
pub struct Dag {
    nodes: Vec<Node>,
    // Dependencies: each (for_node, depends_on) edge
    deps: Vec<(Handle, Handle)>,
    idgen: Arc<IdGen>,
}

impl Dag {
    pub fn new(idgen: Arc<IdGen>) -> Self {
        Self {
            nodes: Vec::new(),
            deps: Vec::new(),
            idgen,
        }
    }

    pub fn add_node(&mut self, idname: String, kind: NodeKind) -> Handle {
        let pid = Handle::new(self.idgen.get_next());

        self.nodes.push(Node {
            pid,
            idname,
            kind,
            state: NodeState::NotStarted,
        });

        pid
    }

    #[must_use]
    pub fn get_node(&self, pid: Handle) -> Option<&Node> {
        self.nodes.iter().find(|n| n.pid == pid)
    }

    pub fn get_node_mut(&mut self, pid: Handle) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| n.pid == pid)
    }

    pub fn set_state(&mut self, pid: Handle, state: NodeState) {
        if let Some(node) = self.get_node_mut(pid) {
            node.state = state;
        }
    }

    pub fn add_dependency(&mut self, node: For, dependency: DependsOn) {
        let For(for_node) = node;
        let DependsOn(depends_on) = dependency;

        self.deps.push((for_node, depends_on));
    }

    pub fn get_direct_dependencies(&self, pid: Handle) -> impl Iterator<Item = Handle> + '_ {
        self.deps
            .iter()
            .filter(move |(for_node, _)| *for_node == pid)
            .map(|(_, depends_on)| *depends_on)
    }

    pub fn get_direct_dependents(&self, pid: Handle) -> impl Iterator<Item = Handle> + '_ {
        self.deps
            .iter()
            .filter(move |(_, depends_on)| *depends_on == pid)
            .map(|(for_node, _)| *for_node)
    }

    #[must_use]
    pub fn resolve_dependencies(&self, pid: Handle) -> DependencyIterator<'_> {
        let to_visit: Vec<Handle> = self.get_direct_dependencies(pid).collect();

        DependencyIterator {
            dag: self,
            to_visit,
            visited: HashSet::new(),
        }
    }

    /// Prints the dependency tree for a given node
    #[must_use]
    pub fn dump(&self, pid: Handle) -> String {
        let mut output = String::new();
        let mut visited = HashSet::new();
        self.dump_recursive(pid, "", true, &mut output, &mut visited);
        output
    }

    fn dump_recursive(
        &self,
        pid: Handle,
        prefix: &str,
        is_last: bool,
        output: &mut String,
        visited: &mut HashSet<Handle>,
    ) {
        // Get node info
        let Some(node) = self.get_node(pid) else {
            let _ = writeln!(output, "{prefix}├── [PID {pid:?} not found]");
            return;
        };

        // Format the current node line
        let connector = if is_last { "└── " } else { "├── " };
        let state_symbol = match node.state {
            NodeState::NotStarted => "⋯ not built",
            NodeState::Running => "⚙ running",
            NodeState::Terminated => "✓ built",
        };

        let _ = writeln!(output, "{prefix}{connector}{} [{state_symbol}]", node.idname);

        // Check for cycles
        if visited.contains(&pid) {
            let extension = if is_last { "    " } else { "│   " };
            let _ = writeln!(output, "{prefix}{extension}[circular reference]");
            return;
        }
        visited.insert(pid);

        // Get resolved dependencies (aliases are resolved to concrete nodes)
        let deps: Vec<Handle> = self.resolve_dependencies(pid).collect();
        if deps.is_empty() {
            visited.remove(&pid);
            return;
        }

        // Prepare prefix for children
        let child_prefix = format!("{}{}", prefix, if is_last { "    " } else { "│   " });

        // Recursively dump dependencies
        for (idx, &dep_pid) in deps.iter().enumerate() {
            let is_last_child = idx == deps.len() - 1;
            self.dump_recursive(dep_pid, &child_prefix, is_last_child, output, visited);
        }

        visited.remove(&pid);
    }
}

pub struct DependencyIterator<'a> {
    dag: &'a Dag,
    to_visit: Vec<Handle>,
    visited: HashSet<Handle>,
}

impl Iterator for DependencyIterator<'_> {
    type Item = Handle;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(pid) = self.to_visit.pop() {
            if self.visited.insert(pid) {
                if let Some(node) = self.dag.get_node(pid) {
                    match &node.kind {
                        NodeKind::Concrete => return Some(pid),
                        NodeKind::Alias => {
                            self.to_visit.extend(self.dag.get_direct_dependencies(pid));
                        }
                    }
                }
            }
        }
        None
    }
}

/// Owned variant of `DependencyIterator` that holds `Arc<Dag>`.
///
/// This allows the iterator to be stored in structs that need to own their data,
/// such as `MergeReader` which is moved during async operations.
pub struct OwnedDependencyIterator {
    dag: Arc<Dag>,
    to_visit: Vec<Handle>,
    visited: HashSet<Handle>,
}

impl OwnedDependencyIterator {
    /// Create a new owned dependency iterator for the given node.
    ///
    /// Resolves aliases and yields only concrete dependency nodes.
    #[must_use]
    pub fn new(dag: Arc<Dag>, pid: Handle) -> Self {
        let to_visit: Vec<Handle> = dag.get_direct_dependencies(pid).collect();
        Self {
            dag,
            to_visit,
            visited: HashSet::new(),
        }
    }
}

impl Iterator for OwnedDependencyIterator {
    type Item = Handle;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(pid) = self.to_visit.pop() {
            if self.visited.insert(pid) {
                if let Some(node) = self.dag.get_node(pid) {
                    match &node.kind {
                        NodeKind::Concrete => return Some(pid),
                        NodeKind::Alias => {
                            self.to_visit.extend(self.dag.get_direct_dependencies(pid));
                        }
                    }
                }
            }
        }
        None
    }
}
