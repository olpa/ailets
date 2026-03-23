use std::sync::Arc;

use ailetos::dag::{Dag, DependsOn, For, NodeKind};
use ailetos::scheduler::{RunOptions, Scheduler};
use ailetos::IdGen;

fn create_linear_dag() -> (Dag, Vec<ailetos::Handle>) {
    // Create a linear DAG: node1 -> node2 -> node3 -> node4
    let idgen = Arc::new(IdGen::new());
    let mut dag = Dag::new(idgen);

    let node1 = dag.add_node("node1".to_string(), NodeKind::Concrete);
    let node2 = dag.add_node("node2".to_string(), NodeKind::Concrete);
    let node3 = dag.add_node("node3".to_string(), NodeKind::Concrete);
    let node4 = dag.add_node("node4".to_string(), NodeKind::Concrete);

    dag.add_dependency(For(node2), DependsOn(node1));
    dag.add_dependency(For(node3), DependsOn(node2));
    dag.add_dependency(For(node4), DependsOn(node3));

    (dag, vec![node1, node2, node3, node4])
}

#[test]
fn test_one_step_executes_only_first_node() {
    let (dag, nodes) = create_linear_dag();
    let [node1, _, _, node4] = [nodes[0], nodes[1], nodes[2], nodes[3]];

    let options = RunOptions {
        one_step: true,
        stop_before: None,
        stop_after: None,
    };

    let scheduler = Scheduler::with_options(&dag, node4, options);
    let executed: Vec<_> = scheduler.iter().collect();

    assert_eq!(executed.len(), 1, "one_step should execute only one node");
    assert_eq!(executed[0], node1, "Should execute the first node (node1)");
}

#[test]
fn test_stop_before_excludes_target_node() {
    let (dag, nodes) = create_linear_dag();
    let [node1, node2, node3, node4] = [nodes[0], nodes[1], nodes[2], nodes[3]];

    let options = RunOptions {
        one_step: false,
        stop_before: Some(node3),
        stop_after: None,
    };

    let scheduler = Scheduler::with_options(&dag, node4, options);
    let executed: Vec<_> = scheduler.iter().collect();

    assert_eq!(
        executed,
        vec![node1, node2],
        "stop_before should execute nodes before node3 but not node3 itself"
    );
}

#[test]
fn test_stop_after_includes_target_node() {
    let (dag, nodes) = create_linear_dag();
    let [node1, node2, _node3, node4] = [nodes[0], nodes[1], nodes[2], nodes[3]];

    let options = RunOptions {
        one_step: false,
        stop_before: None,
        stop_after: Some(node2),
    };

    let scheduler = Scheduler::with_options(&dag, node4, options);
    let executed: Vec<_> = scheduler.iter().collect();

    assert_eq!(
        executed,
        vec![node1, node2],
        "stop_after should execute through node2 and then stop"
    );
}
