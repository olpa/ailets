use std::sync::Arc;

use actor_runtime::StdHandle;
use ailetos::dag::{Dag, DependsOn, For, NodeKind, NodeState};
use ailetos::idgen::IdGen;
use ailetos::is_ready_to_spawn;
use ailetos::pipe::PipePool;
use ailetos::storage::MemKV;
use ailetos::suspension::SuspensionState;

fn make_dag() -> (Dag, Arc<IdGen>) {
    let id_gen = Arc::new(IdGen::new());
    let dag = Dag::new(Arc::clone(&id_gen));
    (dag, id_gen)
}

fn make_pool(id_gen: &Arc<IdGen>) -> (PipePool<MemKV>, Arc<IdGen>) {
    let kv = Arc::new(MemKV::new());
    let pool = PipePool::new(Arc::clone(&kv));
    (pool, Arc::clone(id_gen))
}

// Test 1: node with no deps is always ready
#[tokio::test]
async fn no_deps_is_ready() {
    let (mut dag, id_gen) = make_dag();
    let (pool, _) = make_pool(&id_gen);
    let suspension = SuspensionState::new();

    let node = dag.add_node("a".into(), NodeKind::Concrete);

    assert!(is_ready_to_spawn(node, &dag, &pool, &suspension));
}

// Test 2: NotStarted dep blocks spawn
#[tokio::test]
async fn not_started_dep_blocks() {
    let (mut dag, id_gen) = make_dag();
    let (pool, _) = make_pool(&id_gen);
    let suspension = SuspensionState::new();

    let dep = dag.add_node("dep".into(), NodeKind::Concrete);
    let node = dag.add_node("node".into(), NodeKind::Concrete);
    dag.add_dependency(For(node), DependsOn(dep));
    // dep stays NotStarted

    assert!(!is_ready_to_spawn(node, &dag, &pool, &suspension));
}

// Test 3: Running dep with realized pipe → ready
#[tokio::test]
async fn running_dep_with_output_is_ready() {
    let (mut dag, id_gen) = make_dag();
    let (pool, pool_id_gen) = make_pool(&id_gen);
    let suspension = SuspensionState::new();

    let dep = dag.add_node("dep".into(), NodeKind::Concrete);
    let node = dag.add_node("node".into(), NodeKind::Concrete);
    dag.add_dependency(For(node), DependsOn(dep));
    dag.set_state(dep, NodeState::Running);
    pool.touch_writer(dep, StdHandle::Stdout, &pool_id_gen)
        .await
        .unwrap();

    assert!(is_ready_to_spawn(node, &dag, &pool, &suspension));
}

// Test 4: Terminated dep with no pipe → neutral (skip) → exhausted → ready
#[tokio::test]
async fn terminated_dep_no_output_skips_to_start() {
    let (mut dag, id_gen) = make_dag();
    let (pool, _) = make_pool(&id_gen);
    let suspension = SuspensionState::new();

    let dep = dag.add_node("dep".into(), NodeKind::Concrete);
    let node = dag.add_node("node".into(), NodeKind::Concrete);
    dag.add_dependency(For(node), DependsOn(dep));
    dag.set_state(dep, NodeState::Terminated);
    // no pipe realized

    assert!(is_ready_to_spawn(node, &dag, &pool, &suspension));
}

// Test 5: Terminated dep (no pipe) then NotStarted dep → don't start
#[tokio::test]
async fn skip_then_not_started_blocks() {
    let (mut dag, id_gen) = make_dag();
    let (pool, _) = make_pool(&id_gen);
    let suspension = SuspensionState::new();

    let dep_terminated = dag.add_node("dep_t".into(), NodeKind::Concrete);
    let dep_pending = dag.add_node("dep_p".into(), NodeKind::Concrete);
    let node = dag.add_node("node".into(), NodeKind::Concrete);
    dag.add_dependency(For(node), DependsOn(dep_terminated));
    dag.add_dependency(For(node), DependsOn(dep_pending));
    dag.set_state(dep_terminated, NodeState::Terminated);
    // dep_pending stays NotStarted, no pipes realized

    assert!(!is_ready_to_spawn(node, &dag, &pool, &suspension));
}

// Test 6: Running dep with realized pipe but suspended → don't start
#[tokio::test]
async fn suspended_dep_blocks() {
    let (mut dag, id_gen) = make_dag();
    let (pool, pool_id_gen) = make_pool(&id_gen);
    let suspension = SuspensionState::new();

    let dep = dag.add_node("dep".into(), NodeKind::Concrete);
    let node = dag.add_node("node".into(), NodeKind::Concrete);
    dag.add_dependency(For(node), DependsOn(dep));
    dag.set_state(dep, NodeState::Running);
    pool.touch_writer(dep, StdHandle::Stdout, &pool_id_gen)
        .await
        .unwrap();
    suspension.suspend(dep);

    assert!(!is_ready_to_spawn(node, &dag, &pool, &suspension));
}
