use std::sync::Arc;

use ailetos::dag::{Dag, DependsOn, For, NodeKind, NodeState};
use ailetos::storage::MemKV;
use ailetos::traversal::{StopConditions, TopologicalOrderIter};
use ailetos::{Environment, Executor, ExecutorEvent, IdGen};
use tokio::sync::{mpsc, oneshot};

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

    let stop_conditions = StopConditions {
        one_step: true,
        stop_before: None,
        stop_after: None,
    };

    let executed: Vec<_> =
        TopologicalOrderIter::with_stop_conditions(&dag, node4, stop_conditions).collect();

    assert_eq!(executed.len(), 1, "one_step should execute only one node");
    assert_eq!(executed[0], node1, "Should execute the first node (node1)");
}

#[test]
fn test_stop_before_excludes_target_node() {
    let (dag, nodes) = create_linear_dag();
    let [node1, node2, node3, node4] = [nodes[0], nodes[1], nodes[2], nodes[3]];

    let stop_conditions = StopConditions {
        one_step: false,
        stop_before: Some(node3),
        stop_after: None,
    };

    let executed: Vec<_> =
        TopologicalOrderIter::with_stop_conditions(&dag, node4, stop_conditions).collect();

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

    let stop_conditions = StopConditions {
        one_step: false,
        stop_before: None,
        stop_after: Some(node2),
    };

    let executed: Vec<_> =
        TopologicalOrderIter::with_stop_conditions(&dag, node4, stop_conditions).collect();

    assert_eq!(
        executed,
        vec![node1, node2],
        "stop_after should execute through node2 and then stop"
    );
}

#[test]
fn test_one_step_yields_first_node_regardless_of_state() {
    let (mut dag, nodes) = create_linear_dag();
    let [node1, _node2, _node3, node4] = [nodes[0], nodes[1], nodes[2], nodes[3]];

    // Scheduler yields all nodes unfiltered; the spawn loop filters by state.
    dag.set_state(node1, NodeState::Terminated);

    let stop_conditions = StopConditions {
        one_step: true,
        stop_before: None,
        stop_after: None,
    };

    let executed: Vec<_> =
        TopologicalOrderIter::with_stop_conditions(&dag, node4, stop_conditions).collect();

    assert_eq!(executed.len(), 1, "one_step should yield only one node");
    assert_eq!(executed[0], node1, "yields first node regardless of state");
}

#[test]
fn test_scheduler_yields_suspended_nodes() {
    let (mut dag, nodes) = create_linear_dag();
    let [node1, node2, node3, node4] = [nodes[0], nodes[1], nodes[2], nodes[3]];

    // Mark node2 as running - scheduler should still yield it
    // Suspension is a runtime attribute (not a DAG state); the scheduler
    // yields all non-Terminated nodes regardless of runtime state.
    dag.set_state(node2, NodeState::Running);

    let executed: Vec<_> = TopologicalOrderIter::new(&dag, node4).collect();

    assert_eq!(
        executed,
        vec![node1, node2, node3, node4],
        "TopologicalOrderIter yields all nodes unfiltered; spawn loop decides whether to execute"
    );
}

#[test]
fn test_diamond_dag_valid_topological_order() {
    // Diamond: node1 <- node2, node3 <- node4
    //          (node4 depends on node2 and node3, both depend on node1)
    let idgen = Arc::new(IdGen::new());
    let mut dag = Dag::new(idgen);

    let node1 = dag.add_node("node1".to_string(), NodeKind::Concrete);
    let node2 = dag.add_node("node2".to_string(), NodeKind::Concrete);
    let node3 = dag.add_node("node3".to_string(), NodeKind::Concrete);
    let node4 = dag.add_node("node4".to_string(), NodeKind::Concrete);

    dag.add_dependency(For(node2), DependsOn(node1));
    dag.add_dependency(For(node3), DependsOn(node1));
    dag.add_dependency(For(node4), DependsOn(node2));
    dag.add_dependency(For(node4), DependsOn(node3));

    let order: Vec<_> = TopologicalOrderIter::new(&dag, node4).collect();

    assert_eq!(order.len(), 4, "all four nodes should be yielded");
    assert_eq!(order[3], node4, "node4 must be last");

    let pos: std::collections::HashMap<_, _> =
        order.iter().enumerate().map(|(i, &h)| (h, i)).collect();
    assert!(pos[&node1] < pos[&node2], "node1 must precede node2");
    assert!(pos[&node1] < pos[&node3], "node1 must precede node3");
    assert!(pos[&node2] < pos[&node4], "node2 must precede node4");
    assert!(pos[&node3] < pos[&node4], "node3 must precede node4");
}

// When C fails and B is transitively blocked (A -> B -> C), the executor must
// terminate and leave A and B as NotStarted (not cancel them with ECANCELED).
#[tokio::test]
async fn test_transitive_block_terminates_and_leaves_nodes_not_started() {
    let kv = Arc::new(MemKV::new());
    let env = Arc::new(Environment::new(kv));

    let c = env.add_node("failing".into(), &[], None);
    let b = env.add_node("noop".into(), &[c], None);
    let a = env.add_node("noop".into(), &[b], None);

    env.actor_registry
        .write()
        .register("failing", |_| Err("intentional failure".into()));
    env.actor_registry.write().register("noop", |_| Ok(()));

    let executor = Executor::start(Arc::clone(&env), None);
    executor.submit(a, StopConditions::default()).unwrap();
    executor.shutdown().await;

    let dag = env.dag.read();
    assert_eq!(
        dag.get_node(b).unwrap().state,
        NodeState::NotStarted,
        "b should remain NotStarted (eligible for incremental re-run)"
    );
    assert_eq!(
        dag.get_node(a).unwrap().state,
        NodeState::NotStarted,
        "a should remain NotStarted (eligible for incremental re-run)"
    );
}

#[tokio::test]
async fn run_jobs_finite_single_job() {
    let kv = Arc::new(MemKV::new());
    let env = Arc::new(Environment::new(kv));
    env.actor_registry.write().register("noop", |_| Ok(()));

    let target = env.add_node("noop".into(), &[], None);

    let executor = Executor::start(Arc::clone(&env), None);
    executor.submit(target, StopConditions::default()).unwrap();
    executor.shutdown().await;

    assert_eq!(
        env.dag.read().get_node(target).unwrap().state,
        NodeState::Terminated,
    );
}

#[tokio::test]
async fn run_jobs_infinite_processes_job_submitted_after_quiescence() {
    let kv = Arc::new(MemKV::new());
    let env = Arc::new(Environment::new(kv));
    env.actor_registry.write().register("noop", |_| Ok(()));

    let n1 = env.add_node("noop".into(), &[], None);
    let n2 = env.add_node("noop".into(), &[], None);

    let (ev_tx, mut ev_rx) = mpsc::unbounded_channel::<ExecutorEvent>();
    let executor = Executor::start(Arc::clone(&env), Some(ev_tx));
    executor.submit(n1, StopConditions::default()).unwrap();

    // Wait for n1's termination event — guaranteed once n1 finishes
    loop {
        match ev_rx.recv().await {
            Some(ExecutorEvent::NodeTerminated(h)) if h == n1 => break,
            Some(_) => continue,
            None => panic!("events channel closed before n1 terminated"),
        }
    }

    // Submit n2 then shut down — executor processes n2 before exiting.
    executor
        .submit(n2, StopConditions::default())
        .expect("channel open — executor not yet shut down");
    executor.shutdown().await;

    // If executor exited at quiescence, n2 was never picked up → NotStarted.
    // If executor waited on the channel, n2 was processed → Terminated.
    assert_eq!(
        env.dag.read().get_node(n2).unwrap().state,
        NodeState::Terminated,
    );
}

// n2 submitted while n1 is still running must be picked up without waiting for quiescence.
//
// ActorFn is a bare fn pointer (no closure captures). State is shared through
// local statics, which inner functions can access without capturing.
#[tokio::test]
async fn run_jobs_processes_job_submitted_while_actor_running() {
    use std::sync::Mutex;

    static SLOW_STARTED: Mutex<Option<oneshot::Sender<()>>> = Mutex::new(None);
    static SLOW_RELEASE: Mutex<Option<oneshot::Receiver<()>>> = Mutex::new(None);

    fn slow(_: &dyn actor_runtime::ActorRuntime) -> Result<(), String> {
        SLOW_STARTED.lock().unwrap().take().unwrap().send(()).ok();
        let rx = SLOW_RELEASE.lock().unwrap().take().unwrap();
        rx.blocking_recv().ok();
        Ok(())
    }

    let kv = Arc::new(MemKV::new());
    let env = Arc::new(Environment::new(kv));

    let (started_tx, started_rx) = oneshot::channel::<()>();
    let (release_tx, release_rx) = oneshot::channel::<()>();
    *SLOW_STARTED.lock().unwrap() = Some(started_tx);
    *SLOW_RELEASE.lock().unwrap() = Some(release_rx);

    env.actor_registry.write().register("step4_slow", slow);
    env.actor_registry.write().register("noop", |_| Ok(()));

    let n1 = env.add_node("step4_slow".into(), &[], None);
    let n2 = env.add_node("noop".into(), &[], None);

    let (ev_tx, mut ev_rx) = mpsc::unbounded_channel::<ExecutorEvent>();
    let executor = Executor::start(Arc::clone(&env), Some(ev_tx));

    executor
        .submit(n1, StopConditions::default())
        .expect("channel open");

    // Block until n1 is running
    started_rx.await.unwrap();

    // n1 is Running; submit n2
    executor
        .submit(n2, StopConditions::default())
        .expect("channel open");

    // Drain events until n2 terminates — must happen while n1 is still blocked
    loop {
        match ev_rx.recv().await {
            Some(ExecutorEvent::NodeTerminated(h)) if h == n2 => break,
            Some(_) => continue,
            None => panic!("events channel closed before n2 terminated"),
        }
    }

    // n1 must still be Running — we haven't released it yet
    assert_eq!(
        env.dag.read().get_node(n1).unwrap().state,
        NodeState::Running,
        "n1 must still be running when n2 terminates",
    );

    // Release n1 and wait for the executor to finish
    release_tx.send(()).unwrap();
    executor.shutdown().await;
}
