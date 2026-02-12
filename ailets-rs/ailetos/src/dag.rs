use std::collections::HashSet;

pub type PID = i64;

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
    pub pid: PID,
    pub idname: String,
    pub kind: NodeKind,
    pub state: NodeState,
}

#[derive(Debug)]
pub struct Dag {
    nodes: Vec<Node>,
    // Forward dependencies: "What does X depend on?"
    deps: Vec<(PID, Vec<PID>)>,
    // Reverse dependencies: "What depends on X?"
    reverse_deps: Vec<(PID, Vec<PID>)>,
    next_pid: PID,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DagError {
    NodeNotFound(PID),
}

impl Dag {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            deps: Vec::new(),
            reverse_deps: Vec::new(),
            next_pid: 1,
        }
    }

    pub fn add_node(&mut self, idname: String, kind: NodeKind) -> PID {
        let pid = self.next_pid;
        self.next_pid += 1;

        self.nodes.push(Node {
            pid,
            idname,
            kind,
            state: NodeState::NotStarted,
        });

        pid
    }

    pub fn get_node(&self, pid: PID) -> Option<&Node> {
        self.nodes.iter().find(|n| n.pid == pid)
    }

    pub fn get_node_mut(&mut self, pid: PID) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| n.pid == pid)
    }

    pub fn set_state(&mut self, pid: PID, state: NodeState) -> Result<(), DagError> {
        if let Some(node) = self.get_node_mut(pid) {
            node.state = state;
            Ok(())
        } else {
            Err(DagError::NodeNotFound(pid))
        }
    }

    pub fn add_dependency(&mut self, from: PID, to: PID) -> Result<(), DagError> {
        // Validate both nodes exist
        if self.get_node(from).is_none() {
            return Err(DagError::NodeNotFound(from));
        }
        if self.get_node(to).is_none() {
            return Err(DagError::NodeNotFound(to));
        }

        // Update forward deps
        if let Some((_, deps)) = self.deps.iter_mut().find(|(p, _)| *p == from) {
            deps.push(to);
        } else {
            self.deps.push((from, vec![to]));
        }

        // Update reverse deps
        if let Some((_, rdeps)) = self.reverse_deps.iter_mut().find(|(p, _)| *p == to) {
            rdeps.push(from);
        } else {
            self.reverse_deps.push((to, vec![from]));
        }

        Ok(())
    }

    pub fn get_dependencies(&self, pid: PID) -> &[PID] {
        self.deps
            .iter()
            .find(|(p, _)| *p == pid)
            .map(|(_, deps)| deps.as_slice())
            .unwrap_or(&[])
    }

    pub fn get_dependents(&self, pid: PID) -> &[PID] {
        self.reverse_deps
            .iter()
            .find(|(p, _)| *p == pid)
            .map(|(_, rdeps)| rdeps.as_slice())
            .unwrap_or(&[])
    }

    pub fn resolve_dependencies(&self, pid: PID) -> DependencyIterator<'_> {
        let to_visit = self.get_dependencies(pid).to_vec();

        DependencyIterator {
            dag: self,
            to_visit,
            visited: HashSet::new(),
        }
    }

    /// Prints the dependency tree for a given node
    pub fn dump(&self, pid: PID) -> String {
        let mut output = String::new();
        let mut visited = HashSet::new();
        self.dump_recursive(pid, "", true, &mut output, &mut visited);
        output
    }

    fn dump_recursive(
        &self,
        pid: PID,
        prefix: &str,
        is_last: bool,
        output: &mut String,
        visited: &mut HashSet<PID>,
    ) {
        // Get node info
        let node = match self.get_node(pid) {
            Some(n) => n,
            None => {
                output.push_str(&format!("{}├── [PID {} not found]\n", prefix, pid));
                return;
            }
        };

        // Format the current node line
        let connector = if is_last { "└── " } else { "├── " };
        let state_symbol = match node.state {
            NodeState::NotStarted => "⋯ not built",
            NodeState::Running => "⚙ running",
            NodeState::Terminated => "✓ built",
        };

        output.push_str(&format!(
            "{}{}{} [{}]\n",
            prefix, connector, node.idname, state_symbol
        ));

        // Check for cycles
        if visited.contains(&pid) {
            let extension = if is_last { "    " } else { "│   " };
            output.push_str(&format!("{}{}[circular reference]\n", prefix, extension));
            return;
        }
        visited.insert(pid);

        // Get resolved dependencies (aliases are resolved to concrete nodes)
        let deps: Vec<PID> = self.resolve_dependencies(pid).collect();
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


impl Default for Dag {
    fn default() -> Self {
        Self::new()
    }
}

pub struct DependencyIterator<'a> {
    dag: &'a Dag,
    to_visit: Vec<PID>,
    visited: HashSet<PID>,
}

impl<'a> Iterator for DependencyIterator<'a> {
    type Item = PID;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(pid) = self.to_visit.pop() {
            if self.visited.insert(pid) {
                if let Some(node) = self.dag.get_node(pid) {
                    match &node.kind {
                        NodeKind::Concrete => return Some(pid),
                        NodeKind::Alias => {
                            self.to_visit.extend(self.dag.get_dependencies(pid));
                        }
                    }
                }
            }
        }
        None
    }
}
