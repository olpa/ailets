use std::collections::{HashSet, VecDeque};
use std::fmt::Write;
use std::sync::Arc;

use parking_lot::RwLock;

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
    Terminating,
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
    pub explain: Option<String>,
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
        self.add_node_with_explain(idname, kind, None)
    }

    pub fn add_node_with_explain(
        &mut self,
        idname: String,
        kind: NodeKind,
        explain: Option<String>,
    ) -> Handle {
        let pid = Handle::new(self.idgen.get_next());

        self.nodes.push(Node {
            pid,
            idname,
            kind,
            state: NodeState::NotStarted,
            explain,
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
        let to_visit: VecDeque<Handle> = self.get_direct_dependencies(pid).collect();

        DependencyIterator {
            dag: self,
            to_visit,
            visited: HashSet::new(),
        }
    }

    /// Prints the dependency tree for a given node (no colors)
    ///
    /// If the starting node is an alias, it is skipped and its resolved
    /// dependencies are printed as root nodes instead.
    #[must_use]
    pub fn dump(&self, pid: Handle) -> String {
        self.dump_impl(pid, false)
    }

    /// Prints the dependency tree for a given node with ANSI colors
    ///
    /// If the starting node is an alias, it is skipped and its resolved
    /// dependencies are printed as root nodes instead.
    #[must_use]
    pub fn dump_colored(&self, pid: Handle) -> String {
        self.dump_impl(pid, true)
    }

    fn dump_impl(&self, pid: Handle, use_colors: bool) -> String {
        let mut output = String::new();
        let mut visited = HashSet::new();
        let mut printed = HashSet::new();

        // If starting from an alias, skip it and dump its resolved dependencies
        if let Some(node) = self.get_node(pid) {
            if node.kind == NodeKind::Alias {
                let deps: Vec<Handle> = self.resolve_dependencies(pid).collect();
                for (idx, &dep_pid) in deps.iter().enumerate() {
                    let is_last = idx == deps.len() - 1;
                    self.dump_recursive(
                        dep_pid,
                        "",
                        is_last,
                        true,
                        use_colors,
                        &mut output,
                        &mut visited,
                        &mut printed,
                    );
                }
                return output;
            }
        }

        self.dump_recursive(
            pid,
            "",
            true,
            true,
            use_colors,
            &mut output,
            &mut visited,
            &mut printed,
        );
        output
    }

    #[allow(clippy::too_many_arguments)]
    fn dump_recursive(
        &self,
        pid: Handle,
        prefix: &str,
        is_last: bool,
        is_root: bool,
        use_colors: bool,
        output: &mut String,
        visited: &mut HashSet<Handle>,
        printed: &mut HashSet<Handle>,
    ) {
        // ANSI color codes
        const GREEN: &str = "\x1b[32m";
        const YELLOW: &str = "\x1b[33m";
        const MAGENTA: &str = "\x1b[35m";
        const RESET: &str = "\x1b[0m";

        // Get node info
        let Some(node) = self.get_node(pid) else {
            let _ = writeln!(output, "{prefix}├── [PID {pid:?} not found]");
            return;
        };

        // Check for cycles BEFORE printing the node
        // This way we can still show the node but mark it as circular
        let is_circular = visited.contains(&pid);

        // Check if node was already printed with its dependencies
        let has_deps = self.get_direct_dependencies(pid).next().is_some();
        let already_printed = printed.contains(&pid) && has_deps;

        // Format the current node line (root nodes have no connector)
        let connector = if is_root {
            ""
        } else if is_last {
            "└── "
        } else {
            "├── "
        };

        let state_symbol = if use_colors {
            match node.state {
                NodeState::NotStarted => format!("{YELLOW}⋯ not built{RESET}"),
                NodeState::Running => format!("{MAGENTA}⚙ running{RESET}"),
                NodeState::Terminating => format!("{MAGENTA}⏳ terminating{RESET}"),
                NodeState::Terminated => format!("{GREEN}✓ built{RESET}"),
            }
        } else {
            match node.state {
                NodeState::NotStarted => "⋯ not built".to_string(),
                NodeState::Running => "⚙ running".to_string(),
                NodeState::Terminating => "⏳ terminating".to_string(),
                NodeState::Terminated => "✓ built".to_string(),
            }
        };

        let explain_suffix = node
            .explain
            .as_ref()
            .map(|e| format!(" # {e}"))
            .unwrap_or_default();

        let circular_suffix = if is_circular {
            " [circular reference]"
        } else if already_printed {
            " [see above]"
        } else {
            ""
        };

        let _ = writeln!(
            output,
            "{prefix}{connector}{}.{} [{state_symbol}]{explain_suffix}{circular_suffix}",
            node.idname,
            node.pid.id()
        );

        // If circular or already printed with deps, stop recursing here
        if is_circular || already_printed {
            return;
        }
        visited.insert(pid);

        // Get direct dependencies (not resolved) to handle cycles better
        let deps: Vec<Handle> = self.get_direct_dependencies(pid).collect();
        if deps.is_empty() {
            visited.remove(&pid);
            return;
        }

        // Mark this node as printed before recursing into children
        printed.insert(pid);

        // Prepare prefix for children (root nodes have no prefix extension)
        let child_prefix = if is_root {
            String::new()
        } else {
            format!("{}{}", prefix, if is_last { "    " } else { "│   " })
        };

        // Recursively dump dependencies
        for (idx, &dep_pid) in deps.iter().enumerate() {
            let is_last_child = idx == deps.len() - 1;

            // Check if this dependency is an alias - if so, resolve and recurse into targets
            if let Some(dep_node) = self.get_node(dep_pid) {
                if dep_node.kind == NodeKind::Alias {
                    // For aliases, expand to their targets
                    let alias_targets: Vec<Handle> =
                        self.get_direct_dependencies(dep_pid).collect();
                    for (alias_idx, &target_pid) in alias_targets.iter().enumerate() {
                        let is_last_target = alias_idx == alias_targets.len() - 1 && is_last_child;
                        self.dump_recursive(
                            target_pid,
                            &child_prefix,
                            is_last_target,
                            false,
                            use_colors,
                            output,
                            visited,
                            printed,
                        );
                    }
                    continue;
                }
            }

            self.dump_recursive(
                dep_pid,
                &child_prefix,
                is_last_child,
                false,
                use_colors,
                output,
                visited,
                printed,
            );
        }

        visited.remove(&pid);
    }
}

pub struct DependencyIterator<'a> {
    dag: &'a Dag,
    to_visit: VecDeque<Handle>,
    visited: HashSet<Handle>,
}

impl Iterator for DependencyIterator<'_> {
    type Item = Handle;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(pid) = self.to_visit.pop_front() {
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

/// Owned variant of `DependencyIterator` that holds `Arc<RwLock<Dag>>`.
///
/// This allows the iterator to be stored in structs that need to own their data,
/// such as `MergeReader` which is moved during async operations.
pub struct OwnedDependencyIterator {
    dag: Arc<RwLock<Dag>>,
    to_visit: VecDeque<Handle>,
    visited: HashSet<Handle>,
}

impl OwnedDependencyIterator {
    /// Create a new owned dependency iterator for the given node.
    ///
    /// Resolves aliases and yields only concrete dependency nodes.
    #[must_use]
    pub fn new(dag: Arc<RwLock<Dag>>, pid: Handle) -> Self {
        let to_visit: VecDeque<Handle> = dag.read().get_direct_dependencies(pid).collect();
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
        while let Some(pid) = self.to_visit.pop_front() {
            if self.visited.insert(pid) {
                let dag = self.dag.read();
                if let Some(node) = dag.get_node(pid) {
                    match &node.kind {
                        NodeKind::Concrete => return Some(pid),
                        NodeKind::Alias => {
                            self.to_visit.extend(dag.get_direct_dependencies(pid));
                        }
                    }
                }
            }
        }
        None
    }
}
