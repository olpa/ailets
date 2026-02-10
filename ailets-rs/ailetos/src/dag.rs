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
    Alias { targets: Vec<PID> },
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
                        NodeKind::Alias { targets } => {
                            self.to_visit.extend(targets);
                        }
                    }
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 1. Node Creation and Basic Operations
    #[test]
    fn test_create_empty_dag() {
        let dag = Dag::new();
        assert_eq!(dag.nodes.len(), 0);
    }

    #[test]
    fn test_add_concrete_node() {
        let mut dag = Dag::new();
        let pid = dag.add_node("test_node".to_string(), NodeKind::Concrete);
        assert_eq!(pid, 1);
        let node = dag.get_node(pid).unwrap();
        assert_eq!(node.idname, "test_node");
        assert_eq!(node.kind, NodeKind::Concrete);
        assert_eq!(node.state, NodeState::NotStarted);
    }

    #[test]
    fn test_add_alias_node() {
        let mut dag = Dag::new();
        let pid1 = dag.add_node("node1".to_string(), NodeKind::Concrete);
        let pid2 = dag.add_node("node2".to_string(), NodeKind::Concrete);
        let alias_pid = dag.add_node(
            "alias".to_string(),
            NodeKind::Alias {
                targets: vec![pid1, pid2],
            },
        );

        let node = dag.get_node(alias_pid).unwrap();
        assert_eq!(node.idname, "alias");
        match &node.kind {
            NodeKind::Alias { targets } => {
                assert_eq!(targets, &vec![pid1, pid2]);
            }
            _ => panic!("Expected alias node"),
        }
    }

    #[test]
    fn test_get_existing_node() {
        let mut dag = Dag::new();
        let pid = dag.add_node("test".to_string(), NodeKind::Concrete);
        assert!(dag.get_node(pid).is_some());
    }

    #[test]
    fn test_get_nonexistent_node() {
        let dag = Dag::new();
        assert!(dag.get_node(999).is_none());
    }

    #[test]
    fn test_pid_uniqueness() {
        let mut dag = Dag::new();
        let pid1 = dag.add_node("node1".to_string(), NodeKind::Concrete);
        let pid2 = dag.add_node("node2".to_string(), NodeKind::Concrete);
        let pid3 = dag.add_node("node3".to_string(), NodeKind::Concrete);

        assert_ne!(pid1, pid2);
        assert_ne!(pid2, pid3);
        assert_ne!(pid1, pid3);
        assert_eq!(pid1, 1);
        assert_eq!(pid2, 2);
        assert_eq!(pid3, 3);
    }

    #[test]
    fn test_multiple_nodes_with_same_idname() {
        let mut dag = Dag::new();
        let pid1 = dag.add_node("same_name".to_string(), NodeKind::Concrete);
        let pid2 = dag.add_node("same_name".to_string(), NodeKind::Concrete);

        assert_ne!(pid1, pid2);
        assert_eq!(dag.get_node(pid1).unwrap().idname, "same_name");
        assert_eq!(dag.get_node(pid2).unwrap().idname, "same_name");
    }

    // 2. Forward Dependencies
    #[test]
    fn test_add_single_dependency() {
        let mut dag = Dag::new();
        let pid1 = dag.add_node("node1".to_string(), NodeKind::Concrete);
        let pid2 = dag.add_node("node2".to_string(), NodeKind::Concrete);

        assert!(dag.add_dependency(pid1, pid2).is_ok());
        let deps = dag.get_dependencies(pid1);
        assert_eq!(deps, &[pid2]);
    }

    #[test]
    fn test_add_multiple_dependencies() {
        let mut dag = Dag::new();
        let pid1 = dag.add_node("node1".to_string(), NodeKind::Concrete);
        let pid2 = dag.add_node("node2".to_string(), NodeKind::Concrete);
        let pid3 = dag.add_node("node3".to_string(), NodeKind::Concrete);

        assert!(dag.add_dependency(pid1, pid2).is_ok());
        assert!(dag.add_dependency(pid1, pid3).is_ok());

        let deps = dag.get_dependencies(pid1);
        assert_eq!(deps.len(), 2);
        assert!(deps.contains(&pid2));
        assert!(deps.contains(&pid3));
    }

    #[test]
    fn test_get_dependencies_empty() {
        let mut dag = Dag::new();
        let pid = dag.add_node("node".to_string(), NodeKind::Concrete);

        let deps = dag.get_dependencies(pid);
        assert_eq!(deps.len(), 0);
    }

    #[test]
    fn test_get_dependencies_nonexistent_node() {
        let dag = Dag::new();
        let deps = dag.get_dependencies(999);
        assert_eq!(deps.len(), 0);
    }

    #[test]
    fn test_add_dependency_from_nonexistent_node() {
        let mut dag = Dag::new();
        let pid = dag.add_node("node".to_string(), NodeKind::Concrete);

        let result = dag.add_dependency(999, pid);
        assert_eq!(result, Err(DagError::NodeNotFound(999)));
    }

    #[test]
    fn test_add_dependency_to_nonexistent_node() {
        let mut dag = Dag::new();
        let pid = dag.add_node("node".to_string(), NodeKind::Concrete);

        let result = dag.add_dependency(pid, 999);
        assert_eq!(result, Err(DagError::NodeNotFound(999)));
    }

    // 3. Reverse Dependencies
    #[test]
    fn test_get_dependents_empty() {
        let mut dag = Dag::new();
        let pid = dag.add_node("node".to_string(), NodeKind::Concrete);

        let dependents = dag.get_dependents(pid);
        assert_eq!(dependents.len(), 0);
    }

    #[test]
    fn test_get_dependents_single() {
        let mut dag = Dag::new();
        let pid1 = dag.add_node("node1".to_string(), NodeKind::Concrete);
        let pid2 = dag.add_node("node2".to_string(), NodeKind::Concrete);

        dag.add_dependency(pid1, pid2).unwrap();

        let dependents = dag.get_dependents(pid2);
        assert_eq!(dependents, &[pid1]);
    }

    #[test]
    fn test_get_dependents_multiple() {
        let mut dag = Dag::new();
        let pid1 = dag.add_node("node1".to_string(), NodeKind::Concrete);
        let pid2 = dag.add_node("node2".to_string(), NodeKind::Concrete);
        let pid3 = dag.add_node("node3".to_string(), NodeKind::Concrete);

        dag.add_dependency(pid1, pid3).unwrap();
        dag.add_dependency(pid2, pid3).unwrap();

        let dependents = dag.get_dependents(pid3);
        assert_eq!(dependents.len(), 2);
        assert!(dependents.contains(&pid1));
        assert!(dependents.contains(&pid2));
    }

    #[test]
    fn test_dependency_creates_reverse() {
        let mut dag = Dag::new();
        let pid1 = dag.add_node("node1".to_string(), NodeKind::Concrete);
        let pid2 = dag.add_node("node2".to_string(), NodeKind::Concrete);

        dag.add_dependency(pid1, pid2).unwrap();

        assert_eq!(dag.get_dependencies(pid1), &[pid2]);
        assert_eq!(dag.get_dependents(pid2), &[pid1]);
    }

    #[test]
    fn test_get_dependents_nonexistent_node() {
        let dag = Dag::new();
        let dependents = dag.get_dependents(999);
        assert_eq!(dependents.len(), 0);
    }

    // 4. Dependency Iterator (Alias Resolution)
    #[test]
    fn test_resolve_concrete_node() {
        let mut dag = Dag::new();
        let pid1 = dag.add_node("node1".to_string(), NodeKind::Concrete);
        let pid2 = dag.add_node("node2".to_string(), NodeKind::Concrete);

        dag.add_dependency(pid1, pid2).unwrap();

        let resolved: Vec<PID> = dag.resolve_dependencies(pid1).collect();
        assert_eq!(resolved, vec![pid2]);
    }

    #[test]
    fn test_resolve_single_alias() {
        let mut dag = Dag::new();
        let pid1 = dag.add_node("node1".to_string(), NodeKind::Concrete);
        let pid2 = dag.add_node("node2".to_string(), NodeKind::Concrete);
        let alias_pid = dag.add_node(
            "alias".to_string(),
            NodeKind::Alias {
                targets: vec![pid1, pid2],
            },
        );
        let pid3 = dag.add_node("node3".to_string(), NodeKind::Concrete);

        dag.add_dependency(pid3, alias_pid).unwrap();

        let resolved: Vec<PID> = dag.resolve_dependencies(pid3).collect();
        assert_eq!(resolved.len(), 2);
        assert!(resolved.contains(&pid1));
        assert!(resolved.contains(&pid2));
    }

    #[test]
    fn test_resolve_nested_alias() {
        let mut dag = Dag::new();
        let pid1 = dag.add_node("node1".to_string(), NodeKind::Concrete);
        let pid2 = dag.add_node("node2".to_string(), NodeKind::Concrete);
        let alias1 = dag.add_node(
            "alias1".to_string(),
            NodeKind::Alias {
                targets: vec![pid1, pid2],
            },
        );
        let alias2 = dag.add_node(
            "alias2".to_string(),
            NodeKind::Alias {
                targets: vec![alias1],
            },
        );
        let pid3 = dag.add_node("node3".to_string(), NodeKind::Concrete);

        dag.add_dependency(pid3, alias2).unwrap();

        let resolved: Vec<PID> = dag.resolve_dependencies(pid3).collect();
        assert_eq!(resolved.len(), 2);
        assert!(resolved.contains(&pid1));
        assert!(resolved.contains(&pid2));
    }

    #[test]
    fn test_resolve_with_duplicates() {
        let mut dag = Dag::new();
        let pid1 = dag.add_node("node1".to_string(), NodeKind::Concrete);
        let pid2 = dag.add_node("node2".to_string(), NodeKind::Concrete);
        let pid3 = dag.add_node("node3".to_string(), NodeKind::Concrete);

        dag.add_dependency(pid3, pid1).unwrap();
        dag.add_dependency(pid3, pid2).unwrap();
        dag.add_dependency(pid3, pid1).unwrap(); // Duplicate

        let resolved: Vec<PID> = dag.resolve_dependencies(pid3).collect();
        assert_eq!(resolved.len(), 2); // Should deduplicate
        assert!(resolved.contains(&pid1));
        assert!(resolved.contains(&pid2));
    }

    #[test]
    fn test_resolve_empty_alias() {
        let mut dag = Dag::new();
        let alias_pid = dag.add_node(
            "alias".to_string(),
            NodeKind::Alias { targets: vec![] },
        );
        let pid = dag.add_node("node".to_string(), NodeKind::Concrete);

        dag.add_dependency(pid, alias_pid).unwrap();

        let resolved: Vec<PID> = dag.resolve_dependencies(pid).collect();
        assert_eq!(resolved.len(), 0);
    }

    #[test]
    fn test_resolve_nonexistent_node() {
        let dag = Dag::new();
        let resolved: Vec<PID> = dag.resolve_dependencies(999).collect();
        assert_eq!(resolved.len(), 0);
    }

    #[test]
    fn test_resolve_circular_alias() {
        let mut dag = Dag::new();
        let alias1 = dag.add_node(
            "alias1".to_string(),
            NodeKind::Alias { targets: vec![] },
        );
        let alias2 = dag.add_node(
            "alias2".to_string(),
            NodeKind::Alias { targets: vec![alias1] },
        );

        // Manually create circular reference
        if let Some(node) = dag.get_node_mut(alias1) {
            if let NodeKind::Alias { targets } = &mut node.kind {
                targets.push(alias2);
            }
        }

        let pid = dag.add_node("node".to_string(), NodeKind::Concrete);
        dag.add_dependency(pid, alias1).unwrap();

        let resolved: Vec<PID> = dag.resolve_dependencies(pid).collect();
        // Should not infinite loop, returns empty due to deduplication
        assert_eq!(resolved.len(), 0);
    }

    // 5. Complex Scenarios
    #[test]
    fn test_diamond_dependency() {
        let mut dag = Dag::new();
        let d = dag.add_node("D".to_string(), NodeKind::Concrete);
        let b = dag.add_node("B".to_string(), NodeKind::Concrete);
        let c = dag.add_node("C".to_string(), NodeKind::Concrete);
        let a = dag.add_node("A".to_string(), NodeKind::Concrete);

        dag.add_dependency(b, d).unwrap();
        dag.add_dependency(c, d).unwrap();
        dag.add_dependency(a, b).unwrap();
        dag.add_dependency(a, c).unwrap();

        let deps_a = dag.get_dependencies(a);
        assert_eq!(deps_a.len(), 2);
        assert!(deps_a.contains(&b));
        assert!(deps_a.contains(&c));

        let dependents_d = dag.get_dependents(d);
        assert_eq!(dependents_d.len(), 2);
        assert!(dependents_d.contains(&b));
        assert!(dependents_d.contains(&c));
    }

    #[test]
    fn test_concrete_node_depends_on_alias() {
        let mut dag = Dag::new();
        let pid1 = dag.add_node("node1".to_string(), NodeKind::Concrete);
        let pid2 = dag.add_node("node2".to_string(), NodeKind::Concrete);
        let alias = dag.add_node(
            "alias".to_string(),
            NodeKind::Alias {
                targets: vec![pid1, pid2],
            },
        );
        let pid3 = dag.add_node("node3".to_string(), NodeKind::Concrete);

        assert!(dag.add_dependency(pid3, alias).is_ok());

        let deps = dag.get_dependencies(pid3);
        assert_eq!(deps, &[alias]);

        let resolved: Vec<PID> = dag.resolve_dependencies(pid3).collect();
        assert_eq!(resolved.len(), 2);
        assert!(resolved.contains(&pid1));
        assert!(resolved.contains(&pid2));
    }

    #[test]
    fn test_alias_depends_on_alias() {
        let mut dag = Dag::new();
        let pid1 = dag.add_node("node1".to_string(), NodeKind::Concrete);
        let alias1 = dag.add_node(
            "alias1".to_string(),
            NodeKind::Alias {
                targets: vec![pid1],
            },
        );
        let alias2 = dag.add_node(
            "alias2".to_string(),
            NodeKind::Alias {
                targets: vec![alias1],
            },
        );
        let pid2 = dag.add_node("node2".to_string(), NodeKind::Concrete);

        assert!(dag.add_dependency(pid2, alias2).is_ok());

        let resolved: Vec<PID> = dag.resolve_dependencies(pid2).collect();
        assert_eq!(resolved, vec![pid1]);
    }

    #[test]
    fn test_dag_with_many_nodes() {
        let mut dag = Dag::new();
        let mut pids = Vec::new();

        // Create 100 nodes
        for i in 0..100 {
            let pid = dag.add_node(format!("node{}", i), NodeKind::Concrete);
            pids.push(pid);
        }

        // Add dependencies: each node depends on the previous
        for i in 1..100 {
            dag.add_dependency(pids[i], pids[i - 1]).unwrap();
        }

        // Verify
        assert_eq!(dag.nodes.len(), 100);
        assert_eq!(dag.get_dependencies(pids[50]).len(), 1);
        assert_eq!(dag.get_dependents(pids[50]).len(), 1);
    }
}
