use ailetos::dag::*;

// --------------------------------------------------------------------
// Node Creation and Basic Operations
//

#[test]
fn test_create_empty_dag() {
    let dag = Dag::new();
    assert!(dag.get_node(1).is_none());
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
    let alias_pid = dag.add_node("alias".to_string(), NodeKind::Alias);
    dag.add_dependency(alias_pid, pid1).unwrap();
    dag.add_dependency(alias_pid, pid2).unwrap();

    let node = dag.get_node(alias_pid).unwrap();
    assert_eq!(node.idname, "alias");
    assert_eq!(node.kind, NodeKind::Alias);
    let targets = dag.get_dependencies(alias_pid);
    assert_eq!(targets, &[pid1, pid2]);
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

// --------------------------------------------------------------------
// Forward Dependencies
//

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

// --------------------------------------------------------------------
// Reverse Dependencies
//

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

// --------------------------------------------------------------------
// Dependency Iterator (Alias Resolution)
//
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
    let alias_pid = dag.add_node("alias".to_string(), NodeKind::Alias);
    dag.add_dependency(alias_pid, pid1).unwrap();
    dag.add_dependency(alias_pid, pid2).unwrap();
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
    let alias1 = dag.add_node("alias1".to_string(), NodeKind::Alias);
    dag.add_dependency(alias1, pid1).unwrap();
    dag.add_dependency(alias1, pid2).unwrap();
    let alias2 = dag.add_node("alias2".to_string(), NodeKind::Alias);
    dag.add_dependency(alias2, alias1).unwrap();
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
    let alias_pid = dag.add_node("alias".to_string(), NodeKind::Alias);
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
    let alias1 = dag.add_node("alias1".to_string(), NodeKind::Alias);
    let alias2 = dag.add_node("alias2".to_string(), NodeKind::Alias);
    dag.add_dependency(alias2, alias1).unwrap();
    // Create circular reference
    dag.add_dependency(alias1, alias2).unwrap();

    let pid = dag.add_node("node".to_string(), NodeKind::Concrete);
    dag.add_dependency(pid, alias1).unwrap();

    let resolved: Vec<PID> = dag.resolve_dependencies(pid).collect();
    // Should not infinite loop, returns empty due to deduplication
    assert_eq!(resolved.len(), 0);
}

// --------------------------------------------------------------------
// Complex Scenarios
//
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
    let alias = dag.add_node("alias".to_string(), NodeKind::Alias);
    dag.add_dependency(alias, pid1).unwrap();
    dag.add_dependency(alias, pid2).unwrap();
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
    let alias1 = dag.add_node("alias1".to_string(), NodeKind::Alias);
    dag.add_dependency(alias1, pid1).unwrap();
    let alias2 = dag.add_node("alias2".to_string(), NodeKind::Alias);
    dag.add_dependency(alias2, alias1).unwrap();
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

    // Verify all nodes exist
    for pid in &pids {
        assert!(dag.get_node(*pid).is_some());
    }
    assert_eq!(dag.get_dependencies(pids[50]).len(), 1);
    assert_eq!(dag.get_dependents(pids[50]).len(), 1);
}

// --------------------------------------------------------------------
// Dump Function Tests
//
#[test]
fn test_dump_single_node() {
    let mut dag = Dag::new();
    let pid = dag.add_node("root".to_string(), NodeKind::Concrete);

    let output = dag.dump(pid);
    assert!(output.contains("root"));
    assert!(output.contains("⋯ not built"));
}

#[test]
fn test_dump_linear_chain() {
    let mut dag = Dag::new();
    let pid1 = dag.add_node("node1".to_string(), NodeKind::Concrete);
    let pid2 = dag.add_node("node2".to_string(), NodeKind::Concrete);
    let pid3 = dag.add_node("node3".to_string(), NodeKind::Concrete);

    dag.add_dependency(pid1, pid2).unwrap();
    dag.add_dependency(pid2, pid3).unwrap();

    let output = dag.dump(pid1);
    assert!(output.contains("node1"));
    assert!(output.contains("node2"));
    assert!(output.contains("node3"));
    assert!(output.contains("└──"));
}

#[test]
fn test_dump_multiple_dependencies() {
    let mut dag = Dag::new();
    let root = dag.add_node("root".to_string(), NodeKind::Concrete);
    let dep1 = dag.add_node("dep1".to_string(), NodeKind::Concrete);
    let dep2 = dag.add_node("dep2".to_string(), NodeKind::Concrete);

    dag.add_dependency(root, dep1).unwrap();
    dag.add_dependency(root, dep2).unwrap();

    let output = dag.dump(root);
    assert!(output.contains("root"));
    assert!(output.contains("dep1"));
    assert!(output.contains("dep2"));
    assert!(output.contains("├──"));
    assert!(output.contains("└──"));
}

#[test]
fn test_dump_different_states() {
    let mut dag = Dag::new();
    let root = dag.add_node("root".to_string(), NodeKind::Concrete);
    let running = dag.add_node("running_node".to_string(), NodeKind::Concrete);
    let finished = dag.add_node("finished_node".to_string(), NodeKind::Concrete);

    dag.set_state(running, NodeState::Running).unwrap();
    dag.set_state(finished, NodeState::Terminated).unwrap();

    dag.add_dependency(root, running).unwrap();
    dag.add_dependency(root, finished).unwrap();

    let output = dag.dump(root);
    assert!(output.contains("⋯ not built")); // root
    assert!(output.contains("⚙ running"));
    assert!(output.contains("✓ built"));
}

#[test]
fn test_dump_diamond_structure() {
    let mut dag = Dag::new();
    let a = dag.add_node("A".to_string(), NodeKind::Concrete);
    let b = dag.add_node("B".to_string(), NodeKind::Concrete);
    let c = dag.add_node("C".to_string(), NodeKind::Concrete);
    let d = dag.add_node("D".to_string(), NodeKind::Concrete);

    dag.add_dependency(a, b).unwrap();
    dag.add_dependency(a, c).unwrap();
    dag.add_dependency(b, d).unwrap();
    dag.add_dependency(c, d).unwrap();

    let output = dag.dump(a);
    // D should appear twice (once under B, once under C)
    let d_count = output.matches("D").count();
    assert_eq!(d_count, 2);
}

#[test]
fn test_dump_nonexistent_node() {
    let dag = Dag::new();
    let output = dag.dump(999);
    assert!(output.contains("not found"));
}

#[test]
fn test_dump_node_with_no_dependencies() {
    let mut dag = Dag::new();
    let pid = dag.add_node("lonely_node".to_string(), NodeKind::Concrete);

    let output = dag.dump(pid);
    assert!(output.contains("lonely_node"));
    // Should just show the node itself, no dependencies
    assert_eq!(output.lines().count(), 1);
}

#[test]
fn test_dump_deep_tree() {
    let mut dag = Dag::new();
    let mut current = dag.add_node("level0".to_string(), NodeKind::Concrete);

    for i in 1..=5 {
        let next = dag.add_node(format!("level{}", i), NodeKind::Concrete);
        dag.add_dependency(current, next).unwrap();
        current = next;
    }

    let output = dag.dump(1); // Start from level0
    assert!(output.contains("level0"));
    assert!(output.contains("level5"));
    // Linear chain uses └── for each level
    assert!(output.contains("└──"));
}

#[test]
fn test_dump_with_alias_resolution() {
    let mut dag = Dag::new();
    let node1 = dag.add_node("node1".to_string(), NodeKind::Concrete);
    let node2 = dag.add_node("node2".to_string(), NodeKind::Concrete);
    let alias = dag.add_node("alias_name".to_string(), NodeKind::Alias);
    dag.add_dependency(alias, node1).unwrap();
    dag.add_dependency(alias, node2).unwrap();
    let root = dag.add_node("root".to_string(), NodeKind::Concrete);

    dag.add_dependency(root, alias).unwrap();

    let output = dag.dump(root);
    // Aliases should be resolved, so we see concrete nodes, not the alias
    assert!(output.contains("node1"));
    assert!(output.contains("node2"));
    assert!(!output.contains("alias_name")); // Alias name should NOT appear
}
