use std::sync::Arc;

use ailetos::dag::*;
use ailetos::{Handle, IdGen};

// --------------------------------------------------------------------
// Colored Dump Tests
//

#[test]
fn test_dump_colored_shows_ansi_codes() {
    let idgen = Arc::new(IdGen::new());
    let mut dag = Dag::new(idgen);
    let built = dag.add_node("built_node".to_string(), NodeKind::Concrete);
    dag.set_state(built, NodeState::Terminated);

    let output = dag.dump_colored(built);
    // Should contain green ANSI code for built status
    assert!(
        output.contains("\x1b[32m"),
        "Should contain green ANSI code, got: {:?}",
        output
    );
    assert!(output.contains("\x1b[0m"), "Should contain reset ANSI code");
}

#[test]
fn test_dump_colored_different_states() {
    let idgen = Arc::new(IdGen::new());
    let mut dag = Dag::new(idgen);

    let root = dag.add_node("root".to_string(), NodeKind::Concrete);
    let running = dag.add_node("running_node".to_string(), NodeKind::Concrete);
    let not_built = dag.add_node("pending_node".to_string(), NodeKind::Concrete);

    dag.set_state(running, NodeState::Running);
    // not_built stays NotStarted

    dag.add_dependency(For(root), DependsOn(running));
    dag.add_dependency(For(root), DependsOn(not_built));

    let output = dag.dump_colored(root);
    // Yellow for not built
    assert!(
        output.contains("\x1b[33m"),
        "Should contain yellow for not built"
    );
    // Magenta for running
    assert!(
        output.contains("\x1b[35m"),
        "Should contain magenta for running"
    );
}

// --------------------------------------------------------------------
// Node Creation and Basic Operations
//

#[test]
fn test_create_empty_dag() {
    let idgen = Arc::new(IdGen::new());
    let dag = Dag::new(idgen);
    assert!(dag.get_node(Handle::new(1)).is_none());
}

#[test]
fn test_add_concrete_node() {
    let idgen = Arc::new(IdGen::new());
    let mut dag = Dag::new(idgen);
    let pid = dag.add_node("test_node".to_string(), NodeKind::Concrete);
    assert_eq!(pid.id(), 1);
    let node = dag.get_node(pid).unwrap();
    assert_eq!(node.idname, "test_node");
    assert_eq!(node.kind, NodeKind::Concrete);
    assert_eq!(node.state, NodeState::NotStarted);
}

#[test]
fn test_add_alias_node() {
    let idgen = Arc::new(IdGen::new());
    let mut dag = Dag::new(idgen);
    let pid1 = dag.add_node("node1".to_string(), NodeKind::Concrete);
    let pid2 = dag.add_node("node2".to_string(), NodeKind::Concrete);
    let alias_pid = dag.add_node("alias".to_string(), NodeKind::Alias);
    dag.add_dependency(For(alias_pid), DependsOn(pid1));
    dag.add_dependency(For(alias_pid), DependsOn(pid2));

    let node = dag.get_node(alias_pid).unwrap();
    assert_eq!(node.idname, "alias");
    assert_eq!(node.kind, NodeKind::Alias);
    let targets: Vec<Handle> = dag.get_direct_dependencies(alias_pid).collect();
    assert_eq!(targets, vec![pid1, pid2]);
}

#[test]
fn test_get_existing_node() {
    let idgen = Arc::new(IdGen::new());
    let mut dag = Dag::new(idgen);
    let pid = dag.add_node("test".to_string(), NodeKind::Concrete);
    assert!(dag.get_node(pid).is_some());
}

#[test]
fn test_get_nonexistent_node() {
    let idgen = Arc::new(IdGen::new());
    let dag = Dag::new(idgen);
    assert!(dag.get_node(Handle::new(999)).is_none());
}

#[test]
fn test_pid_uniqueness() {
    let idgen = Arc::new(IdGen::new());
    let mut dag = Dag::new(idgen);
    let pid1 = dag.add_node("node1".to_string(), NodeKind::Concrete);
    let pid2 = dag.add_node("node2".to_string(), NodeKind::Concrete);
    let pid3 = dag.add_node("node3".to_string(), NodeKind::Concrete);

    assert_ne!(pid1, pid2);
    assert_ne!(pid2, pid3);
    assert_ne!(pid1, pid3);
}

#[test]
fn test_multiple_nodes_with_same_idname() {
    let idgen = Arc::new(IdGen::new());
    let mut dag = Dag::new(idgen);
    let pid1 = dag.add_node("same_name".to_string(), NodeKind::Concrete);
    let pid2 = dag.add_node("same_name".to_string(), NodeKind::Concrete);

    assert_ne!(pid1, pid2);
    assert_eq!(dag.get_node(pid1).unwrap().idname, "same_name");
    assert_eq!(dag.get_node(pid2).unwrap().idname, "same_name");
}

// --------------------------------------------------------------------
// Direct Dependencies
//

#[test]
fn test_add_single_dependency() {
    let idgen = Arc::new(IdGen::new());
    let mut dag = Dag::new(idgen);
    let pid1 = dag.add_node("node1".to_string(), NodeKind::Concrete);
    let pid2 = dag.add_node("node2".to_string(), NodeKind::Concrete);

    dag.add_dependency(For(pid1), DependsOn(pid2));
    let deps: Vec<Handle> = dag.get_direct_dependencies(pid1).collect();
    assert_eq!(deps, vec![pid2]);
}

#[test]
fn test_add_multiple_dependencies() {
    let idgen = Arc::new(IdGen::new());
    let mut dag = Dag::new(idgen);
    let pid1 = dag.add_node("node1".to_string(), NodeKind::Concrete);
    let pid2 = dag.add_node("node2".to_string(), NodeKind::Concrete);
    let pid3 = dag.add_node("node3".to_string(), NodeKind::Concrete);

    dag.add_dependency(For(pid1), DependsOn(pid2));
    dag.add_dependency(For(pid1), DependsOn(pid3));

    let deps: Vec<Handle> = dag.get_direct_dependencies(pid1).collect();
    assert_eq!(deps.len(), 2);
    assert!(deps.contains(&pid2));
    assert!(deps.contains(&pid3));
}

#[test]
fn test_get_direct_dependencies_empty() {
    let idgen = Arc::new(IdGen::new());
    let mut dag = Dag::new(idgen);
    let pid = dag.add_node("node".to_string(), NodeKind::Concrete);

    assert_eq!(dag.get_direct_dependencies(pid).count(), 0);
}

#[test]
fn test_get_direct_dependencies_nonexistent_node() {
    let idgen = Arc::new(IdGen::new());
    let dag = Dag::new(idgen);
    assert_eq!(dag.get_direct_dependencies(Handle::new(999)).count(), 0);
}

// --------------------------------------------------------------------
// Dependents (Reverse Lookup)
//

#[test]
fn test_get_direct_dependents_empty() {
    let idgen = Arc::new(IdGen::new());
    let mut dag = Dag::new(idgen);
    let pid = dag.add_node("node".to_string(), NodeKind::Concrete);

    assert_eq!(dag.get_direct_dependents(pid).count(), 0);
}

#[test]
fn test_get_direct_dependents_single() {
    let idgen = Arc::new(IdGen::new());
    let mut dag = Dag::new(idgen);
    let pid1 = dag.add_node("node1".to_string(), NodeKind::Concrete);
    let pid2 = dag.add_node("node2".to_string(), NodeKind::Concrete);

    dag.add_dependency(For(pid1), DependsOn(pid2));

    let dependents: Vec<Handle> = dag.get_direct_dependents(pid2).collect();
    assert_eq!(dependents, vec![pid1]);
}

#[test]
fn test_get_direct_dependents_multiple() {
    let idgen = Arc::new(IdGen::new());
    let mut dag = Dag::new(idgen);
    let pid1 = dag.add_node("node1".to_string(), NodeKind::Concrete);
    let pid2 = dag.add_node("node2".to_string(), NodeKind::Concrete);
    let pid3 = dag.add_node("node3".to_string(), NodeKind::Concrete);

    dag.add_dependency(For(pid1), DependsOn(pid3));
    dag.add_dependency(For(pid2), DependsOn(pid3));

    let dependents: Vec<Handle> = dag.get_direct_dependents(pid3).collect();
    assert_eq!(dependents.len(), 2);
    assert!(dependents.contains(&pid1));
    assert!(dependents.contains(&pid2));
}

#[test]
fn test_dependency_creates_reverse() {
    let idgen = Arc::new(IdGen::new());
    let mut dag = Dag::new(idgen);
    let pid1 = dag.add_node("node1".to_string(), NodeKind::Concrete);
    let pid2 = dag.add_node("node2".to_string(), NodeKind::Concrete);

    dag.add_dependency(For(pid1), DependsOn(pid2));

    let deps: Vec<Handle> = dag.get_direct_dependencies(pid1).collect();
    let dependents: Vec<Handle> = dag.get_direct_dependents(pid2).collect();
    assert_eq!(deps, vec![pid2]);
    assert_eq!(dependents, vec![pid1]);
}

#[test]
fn test_get_direct_dependents_nonexistent_node() {
    let idgen = Arc::new(IdGen::new());
    let dag = Dag::new(idgen);
    assert_eq!(dag.get_direct_dependents(Handle::new(999)).count(), 0);
}

// --------------------------------------------------------------------
// Alias Resolution
//
#[test]
fn test_resolve_concrete_node() {
    let idgen = Arc::new(IdGen::new());
    let mut dag = Dag::new(idgen);
    let pid1 = dag.add_node("node1".to_string(), NodeKind::Concrete);
    let pid2 = dag.add_node("node2".to_string(), NodeKind::Concrete);

    dag.add_dependency(For(pid1), DependsOn(pid2));

    let resolved: Vec<Handle> = dag.resolve_dependencies(pid1).collect();
    assert_eq!(resolved, vec![pid2]);
}

#[test]
fn test_resolve_single_alias() {
    let idgen = Arc::new(IdGen::new());
    let mut dag = Dag::new(idgen);
    let pid1 = dag.add_node("node1".to_string(), NodeKind::Concrete);
    let pid2 = dag.add_node("node2".to_string(), NodeKind::Concrete);
    let alias_pid = dag.add_node("alias".to_string(), NodeKind::Alias);
    dag.add_dependency(For(alias_pid), DependsOn(pid1));
    dag.add_dependency(For(alias_pid), DependsOn(pid2));
    let pid3 = dag.add_node("node3".to_string(), NodeKind::Concrete);

    dag.add_dependency(For(pid3), DependsOn(alias_pid));

    let resolved: Vec<Handle> = dag.resolve_dependencies(pid3).collect();
    assert_eq!(resolved.len(), 2);
    assert!(resolved.contains(&pid1));
    assert!(resolved.contains(&pid2));
}

#[test]
fn test_resolve_nested_alias() {
    let idgen = Arc::new(IdGen::new());
    let mut dag = Dag::new(idgen);
    let pid1 = dag.add_node("node1".to_string(), NodeKind::Concrete);
    let pid2 = dag.add_node("node2".to_string(), NodeKind::Concrete);
    let alias1 = dag.add_node("alias1".to_string(), NodeKind::Alias);
    dag.add_dependency(For(alias1), DependsOn(pid1));
    dag.add_dependency(For(alias1), DependsOn(pid2));
    let alias2 = dag.add_node("alias2".to_string(), NodeKind::Alias);
    dag.add_dependency(For(alias2), DependsOn(alias1));
    let pid3 = dag.add_node("node3".to_string(), NodeKind::Concrete);

    dag.add_dependency(For(pid3), DependsOn(alias2));

    let resolved: Vec<Handle> = dag.resolve_dependencies(pid3).collect();
    assert_eq!(resolved.len(), 2);
    assert!(resolved.contains(&pid1));
    assert!(resolved.contains(&pid2));
}

#[test]
fn test_resolve_with_duplicates() {
    let idgen = Arc::new(IdGen::new());
    let mut dag = Dag::new(idgen);
    let pid1 = dag.add_node("node1".to_string(), NodeKind::Concrete);
    let pid2 = dag.add_node("node2".to_string(), NodeKind::Concrete);
    let pid3 = dag.add_node("node3".to_string(), NodeKind::Concrete);

    dag.add_dependency(For(pid3), DependsOn(pid1));
    dag.add_dependency(For(pid3), DependsOn(pid2));
    dag.add_dependency(For(pid3), DependsOn(pid1)); // Duplicate

    let resolved: Vec<Handle> = dag.resolve_dependencies(pid3).collect();
    assert_eq!(resolved.len(), 2); // Should deduplicate
    assert!(resolved.contains(&pid1));
    assert!(resolved.contains(&pid2));
}

#[test]
fn test_resolve_empty_alias() {
    let idgen = Arc::new(IdGen::new());
    let mut dag = Dag::new(idgen);
    let alias_pid = dag.add_node("alias".to_string(), NodeKind::Alias);
    let pid = dag.add_node("node".to_string(), NodeKind::Concrete);

    dag.add_dependency(For(pid), DependsOn(alias_pid));

    let resolved: Vec<Handle> = dag.resolve_dependencies(pid).collect();
    assert_eq!(resolved.len(), 0);
}

#[test]
fn test_resolve_nonexistent_node() {
    let idgen = Arc::new(IdGen::new());
    let dag = Dag::new(idgen);
    let resolved: Vec<Handle> = dag.resolve_dependencies(Handle::new(999)).collect();
    assert_eq!(resolved.len(), 0);
}

#[test]
fn test_resolve_circular_alias() {
    let idgen = Arc::new(IdGen::new());
    let mut dag = Dag::new(idgen);
    let alias1 = dag.add_node("alias1".to_string(), NodeKind::Alias);
    let alias2 = dag.add_node("alias2".to_string(), NodeKind::Alias);
    dag.add_dependency(For(alias2), DependsOn(alias1));
    // Create circular reference
    dag.add_dependency(For(alias1), DependsOn(alias2));

    let pid = dag.add_node("node".to_string(), NodeKind::Concrete);
    dag.add_dependency(For(pid), DependsOn(alias1));

    let resolved: Vec<Handle> = dag.resolve_dependencies(pid).collect();
    // Should not infinite loop, returns empty due to deduplication
    assert_eq!(resolved.len(), 0);
}

// --------------------------------------------------------------------
// Complex Scenarios
//
// Diamond dependency graph:
//
//     A
//    / \
//   B   C
//    \ /
//     D
//
// A depends on B and C; B and C both depend on D
#[test]
fn test_diamond_dependency() {
    let idgen = Arc::new(IdGen::new());
    let mut dag = Dag::new(idgen);
    let d = dag.add_node("D".to_string(), NodeKind::Concrete);
    let b = dag.add_node("B".to_string(), NodeKind::Concrete);
    let c = dag.add_node("C".to_string(), NodeKind::Concrete);
    let a = dag.add_node("A".to_string(), NodeKind::Concrete);

    dag.add_dependency(For(b), DependsOn(d));
    dag.add_dependency(For(c), DependsOn(d));
    dag.add_dependency(For(a), DependsOn(b));
    dag.add_dependency(For(a), DependsOn(c));

    let deps_a: Vec<Handle> = dag.get_direct_dependencies(a).collect();
    assert_eq!(deps_a.len(), 2);
    assert!(deps_a.contains(&b));
    assert!(deps_a.contains(&c));

    let dependents_d: Vec<Handle> = dag.get_direct_dependents(d).collect();
    assert_eq!(dependents_d.len(), 2);
    assert!(dependents_d.contains(&b));
    assert!(dependents_d.contains(&c));
}

#[test]
fn test_concrete_node_depends_on_alias() {
    let idgen = Arc::new(IdGen::new());
    let mut dag = Dag::new(idgen);
    let pid1 = dag.add_node("node1".to_string(), NodeKind::Concrete);
    let pid2 = dag.add_node("node2".to_string(), NodeKind::Concrete);
    let alias = dag.add_node("alias".to_string(), NodeKind::Alias);
    dag.add_dependency(For(alias), DependsOn(pid1));
    dag.add_dependency(For(alias), DependsOn(pid2));
    let pid3 = dag.add_node("node3".to_string(), NodeKind::Concrete);

    dag.add_dependency(For(pid3), DependsOn(alias));

    let deps: Vec<Handle> = dag.get_direct_dependencies(pid3).collect();
    assert_eq!(deps, vec![alias]);

    let resolved: Vec<Handle> = dag.resolve_dependencies(pid3).collect();
    assert_eq!(resolved.len(), 2);
    assert!(resolved.contains(&pid1));
    assert!(resolved.contains(&pid2));
}

#[test]
fn test_alias_depends_on_alias() {
    let idgen = Arc::new(IdGen::new());
    let mut dag = Dag::new(idgen);
    let pid1 = dag.add_node("node1".to_string(), NodeKind::Concrete);
    let alias1 = dag.add_node("alias1".to_string(), NodeKind::Alias);
    dag.add_dependency(For(alias1), DependsOn(pid1));
    let alias2 = dag.add_node("alias2".to_string(), NodeKind::Alias);
    dag.add_dependency(For(alias2), DependsOn(alias1));
    let pid2 = dag.add_node("node2".to_string(), NodeKind::Concrete);

    dag.add_dependency(For(pid2), DependsOn(alias2));

    let resolved: Vec<Handle> = dag.resolve_dependencies(pid2).collect();
    assert_eq!(resolved, vec![pid1]);
}

#[test]
fn test_dag_with_many_nodes() {
    let idgen = Arc::new(IdGen::new());
    let mut dag = Dag::new(idgen);
    let mut pids = Vec::new();

    // Create 100 nodes
    for i in 0..100 {
        let pid = dag.add_node(format!("node{}", i), NodeKind::Concrete);
        pids.push(pid);
    }

    // Add dependencies: each node depends on the previous
    for i in 1..100 {
        dag.add_dependency(For(pids[i]), DependsOn(pids[i - 1]));
    }

    // Verify all nodes exist
    for pid in &pids {
        assert!(dag.get_node(*pid).is_some());
    }
    assert_eq!(dag.get_direct_dependencies(pids[50]).count(), 1);
    assert_eq!(dag.get_direct_dependents(pids[50]).count(), 1);
}

// --------------------------------------------------------------------
// Dump Function Tests
//
#[test]
fn test_dump_single_node() {
    let idgen = Arc::new(IdGen::new());
    let mut dag = Dag::new(idgen);
    let pid = dag.add_node("root".to_string(), NodeKind::Concrete);

    let output = dag.dump(pid);
    assert!(output.contains("root"));
    assert!(output.contains("⋯ not built"));
    // Root node should not have tree connector prefix
    assert!(
        output.starts_with("root."),
        "Root should not have └── prefix, got: {}",
        output
    );
}

#[test]
fn test_dump_linear_chain() {
    let idgen = Arc::new(IdGen::new());
    let mut dag = Dag::new(idgen);
    let pid1 = dag.add_node("node1".to_string(), NodeKind::Concrete);
    let pid2 = dag.add_node("node2".to_string(), NodeKind::Concrete);
    let pid3 = dag.add_node("node3".to_string(), NodeKind::Concrete);

    dag.add_dependency(For(pid1), DependsOn(pid2));
    dag.add_dependency(For(pid2), DependsOn(pid3));

    let output = dag.dump(pid1);
    assert!(output.contains("node1"));
    assert!(output.contains("node2"));
    assert!(output.contains("node3"));
    assert!(output.contains("└──"));
}

#[test]
fn test_dump_multiple_dependencies() {
    let idgen = Arc::new(IdGen::new());
    let mut dag = Dag::new(idgen);
    let root = dag.add_node("root".to_string(), NodeKind::Concrete);
    let dep1 = dag.add_node("dep1".to_string(), NodeKind::Concrete);
    let dep2 = dag.add_node("dep2".to_string(), NodeKind::Concrete);

    dag.add_dependency(For(root), DependsOn(dep1));
    dag.add_dependency(For(root), DependsOn(dep2));

    let output = dag.dump(root);
    assert!(output.contains("root"));
    assert!(output.contains("dep1"));
    assert!(output.contains("dep2"));
    assert!(output.contains("├──"));
    assert!(output.contains("└──"));
}

#[test]
fn test_dump_different_states() {
    let idgen = Arc::new(IdGen::new());
    let mut dag = Dag::new(idgen);
    let root = dag.add_node("root".to_string(), NodeKind::Concrete);
    let running = dag.add_node("running_node".to_string(), NodeKind::Concrete);
    let finished = dag.add_node("finished_node".to_string(), NodeKind::Concrete);

    dag.set_state(running, NodeState::Running);
    dag.set_state(finished, NodeState::Terminated);

    dag.add_dependency(For(root), DependsOn(running));
    dag.add_dependency(For(root), DependsOn(finished));

    let output = dag.dump(root);
    assert!(output.contains("⋯ not built")); // root
    assert!(output.contains("⚙ running"));
    assert!(output.contains("✓ built"));
}

#[test]
fn test_dump_diamond_structure() {
    let idgen = Arc::new(IdGen::new());
    let mut dag = Dag::new(idgen);
    let a = dag.add_node("A".to_string(), NodeKind::Concrete);
    let b = dag.add_node("B".to_string(), NodeKind::Concrete);
    let c = dag.add_node("C".to_string(), NodeKind::Concrete);
    let d = dag.add_node("D".to_string(), NodeKind::Concrete);

    dag.add_dependency(For(a), DependsOn(b));
    dag.add_dependency(For(a), DependsOn(c));
    dag.add_dependency(For(b), DependsOn(d));
    dag.add_dependency(For(c), DependsOn(d));

    let output = dag.dump(a);
    // D should appear twice (once under B, once under C)
    let d_count = output.matches("D").count();
    assert_eq!(d_count, 2);
}

#[test]
fn test_dump_nonexistent_node() {
    let idgen = Arc::new(IdGen::new());
    let dag = Dag::new(idgen);
    let output = dag.dump(Handle::new(999));
    assert!(output.contains("not found"));
}

#[test]
fn test_dump_node_with_no_dependencies() {
    let idgen = Arc::new(IdGen::new());
    let mut dag = Dag::new(idgen);
    let pid = dag.add_node("lonely_node".to_string(), NodeKind::Concrete);

    let output = dag.dump(pid);
    assert!(output.contains("lonely_node"));
    // Should just show the node itself, no dependencies
    assert_eq!(output.lines().count(), 1);
}

#[test]
fn test_dump_deep_tree() {
    let idgen = Arc::new(IdGen::new());
    let mut dag = Dag::new(idgen);
    let level0 = dag.add_node("level0".to_string(), NodeKind::Concrete);
    let mut current = level0;

    for i in 1..=5 {
        let next = dag.add_node(format!("level{}", i), NodeKind::Concrete);
        dag.add_dependency(For(current), DependsOn(next));
        current = next;
    }

    let output = dag.dump(level0);
    assert!(output.contains("level0"));
    assert!(output.contains("level5"));
    // Linear chain uses └── for each level
    assert!(output.contains("└──"));
}

#[test]
fn test_dump_with_alias_resolution() {
    let idgen = Arc::new(IdGen::new());
    let mut dag = Dag::new(idgen);
    let node1 = dag.add_node("node1".to_string(), NodeKind::Concrete);
    let node2 = dag.add_node("node2".to_string(), NodeKind::Concrete);
    let alias = dag.add_node("alias_name".to_string(), NodeKind::Alias);
    dag.add_dependency(For(alias), DependsOn(node1));
    dag.add_dependency(For(alias), DependsOn(node2));
    let root = dag.add_node("root".to_string(), NodeKind::Concrete);

    dag.add_dependency(For(root), DependsOn(alias));

    let output = dag.dump(root);
    // Aliases should be resolved, so we see concrete nodes, not the alias
    assert!(output.contains("node1"));
    assert!(output.contains("node2"));
    assert!(!output.contains("alias_name")); // Alias name should NOT appear
}

#[test]
fn test_dump_starting_from_alias_skips_alias() {
    let idgen = Arc::new(IdGen::new());
    let mut dag = Dag::new(idgen);
    let node1 = dag.add_node("node1".to_string(), NodeKind::Concrete);
    let alias = dag.add_node(".end".to_string(), NodeKind::Alias);
    dag.add_dependency(For(alias), DependsOn(node1));

    let output = dag.dump(alias);
    // When starting from an alias, the alias itself should not be printed
    assert!(!output.contains(".end"), "Alias should not appear in dump");
    assert!(output.contains("node1"), "Target node should appear");
    // Should have exactly one line (just node1)
    assert_eq!(
        output.lines().count(),
        1,
        "Should only have the concrete node"
    );
}

#[test]
fn test_dump_starting_from_alias_with_multiple_targets() {
    let idgen = Arc::new(IdGen::new());
    let mut dag = Dag::new(idgen);
    let node1 = dag.add_node("node1".to_string(), NodeKind::Concrete);
    let node2 = dag.add_node("node2".to_string(), NodeKind::Concrete);
    let alias = dag.add_node(".end".to_string(), NodeKind::Alias);
    dag.add_dependency(For(alias), DependsOn(node1));
    dag.add_dependency(For(alias), DependsOn(node2));

    let output = dag.dump(alias);
    // When starting from an alias with multiple targets, all targets should appear
    assert!(!output.contains(".end"), "Alias should not appear in dump");
    assert!(output.contains("node1"), "First target should appear");
    assert!(output.contains("node2"), "Second target should appear");
    // Root nodes should not have tree connectors
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines.len(), 2, "Should have two root nodes");
    assert!(
        lines[0].starts_with("node"),
        "First line should start with node name, got: {}",
        lines[0]
    );
    assert!(
        lines[1].starts_with("node"),
        "Second line should start with node name, got: {}",
        lines[1]
    );
}

// --------------------------------------------------------------------
// Shared Dependency Tests
//

#[test]
fn test_dump_shared_dependency_with_children_shows_reference() {
    // When a node with dependencies appears multiple times in the tree,
    // the second occurrence should show a reference instead of expanding again
    let idgen = Arc::new(IdGen::new());
    let mut dag = Dag::new(idgen);

    let leaf = dag.add_node("leaf".to_string(), NodeKind::Concrete);
    let shared = dag.add_node("shared".to_string(), NodeKind::Concrete);
    dag.add_dependency(For(shared), DependsOn(leaf));

    let branch1 = dag.add_node("branch1".to_string(), NodeKind::Concrete);
    let branch2 = dag.add_node("branch2".to_string(), NodeKind::Concrete);
    dag.add_dependency(For(branch1), DependsOn(shared));
    dag.add_dependency(For(branch2), DependsOn(shared));

    let root = dag.add_node("root".to_string(), NodeKind::Concrete);
    dag.add_dependency(For(root), DependsOn(branch1));
    dag.add_dependency(For(root), DependsOn(branch2));

    let output = dag.dump(root);

    // The first occurrence of "shared" should show its dependency on "leaf"
    // The second occurrence should show "[see above]" or similar
    assert!(output.contains("shared"), "Should contain shared node");
    assert!(output.contains("leaf"), "Should contain leaf node");

    // Count occurrences of "leaf" - should appear only once
    let leaf_count = output.matches("leaf").count();
    assert_eq!(leaf_count, 1, "Leaf should appear only once, not be duplicated. Output:\n{}", output);

    // The second occurrence of shared should have a reference marker
    let shared_count = output.matches("shared").count();
    assert_eq!(shared_count, 2, "Shared should appear twice");

    // Should contain some kind of reference marker on second occurrence
    assert!(output.contains("[see above]") || output.contains("[already shown]"),
            "Should contain reference marker. Output:\n{}", output);
}

#[test]
fn test_dump_shared_dependency_without_children_shows_node() {
    // When a leaf node (no dependencies) appears multiple times,
    // it's simpler to just show it each time
    let idgen = Arc::new(IdGen::new());
    let mut dag = Dag::new(idgen);

    let shared_leaf = dag.add_node("shared_leaf".to_string(), NodeKind::Concrete);

    let branch1 = dag.add_node("branch1".to_string(), NodeKind::Concrete);
    let branch2 = dag.add_node("branch2".to_string(), NodeKind::Concrete);
    dag.add_dependency(For(branch1), DependsOn(shared_leaf));
    dag.add_dependency(For(branch2), DependsOn(shared_leaf));

    let root = dag.add_node("root".to_string(), NodeKind::Concrete);
    dag.add_dependency(For(root), DependsOn(branch1));
    dag.add_dependency(For(root), DependsOn(branch2));

    let output = dag.dump(root);

    // Leaf node should appear twice (once under each branch)
    let leaf_count = output.matches("shared_leaf").count();
    assert_eq!(leaf_count, 2, "Leaf without dependencies should appear twice. Output:\n{}", output);

    // Should NOT contain reference markers for leaves
    assert!(!output.contains("[see above]"), "Should not have reference marker for leaves");
    assert!(!output.contains("[already shown]"), "Should not have reference marker for leaves");
}
