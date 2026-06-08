use std::sync::Arc;

use actor_runtime::StdHandle;
use ailetos::dag::{Dag, DependsOn, For, NodeKind, NodeState};
use ailetos::idgen::IdGen;
use ailetos::pipe::PipePool;
use ailetos::storage::MemKV;
use ailetos::EOWNERDEAD;
use ailetos::{is_ready_to_spawn, SpawnReadiness};

fn make_dag() -> (Dag, Arc<IdGen>) {
    let id_gen = Arc::new(IdGen::new());
    let dag = Dag::new(Arc::clone(&id_gen));
    (dag, id_gen)
}

fn make_pool(id_gen: &Arc<IdGen>) -> (PipePool, Arc<IdGen>) {
    let kv: Arc<dyn ailetos::KVBuffers> = Arc::new(MemKV::new());
    let pool = PipePool::new(Arc::clone(&kv));
    (pool, Arc::clone(id_gen))
}

// Node with no deps is always ready
#[tokio::test]
async fn no_deps_is_ready() {
    let (mut dag, id_gen) = make_dag();
    let (pool, _) = make_pool(&id_gen);

    let node = dag.add_node("a".into(), NodeKind::Concrete);

    assert!(matches!(
        is_ready_to_spawn(node, &dag, &pool),
        SpawnReadiness::Ready
    ));
}

// NotStarted dep blocks spawn
#[tokio::test]
async fn not_started_dep_blocks() {
    let (mut dag, id_gen) = make_dag();
    let (pool, _) = make_pool(&id_gen);

    let dep = dag.add_node("dep".into(), NodeKind::Concrete);
    let node = dag.add_node("node".into(), NodeKind::Concrete);
    dag.add_dependency(For(node), DependsOn(dep));
    // dep stays NotStarted

    assert!(!matches!(
        is_ready_to_spawn(node, &dag, &pool),
        SpawnReadiness::Ready
    ));
}

// Running dep with realized pipe → ready
#[tokio::test]
async fn running_dep_with_output_is_ready() {
    let (mut dag, id_gen) = make_dag();
    let (pool, pool_id_gen) = make_pool(&id_gen);

    let dep = dag.add_node("dep".into(), NodeKind::Concrete);
    let node = dag.add_node("node".into(), NodeKind::Concrete);
    dag.add_dependency(For(node), DependsOn(dep));
    dag.set_state(dep, NodeState::Running);
    pool.touch_writer(dep, StdHandle::Stdout as isize, &pool_id_gen)
        .await
        .unwrap();

    assert!(matches!(
        is_ready_to_spawn(node, &dag, &pool),
        SpawnReadiness::Ready
    ));
}

// Terminated dep with no pipe → neutral (skip) → exhausted → ready
#[tokio::test]
async fn terminated_dep_no_output_skips_to_start() {
    let (mut dag, id_gen) = make_dag();
    let (pool, _) = make_pool(&id_gen);

    let dep = dag.add_node("dep".into(), NodeKind::Concrete);
    let node = dag.add_node("node".into(), NodeKind::Concrete);
    dag.add_dependency(For(node), DependsOn(dep));
    dag.set_state(dep, NodeState::Terminated);
    // no pipe realized

    assert!(matches!(
        is_ready_to_spawn(node, &dag, &pool),
        SpawnReadiness::Ready
    ));
}

// Running dep without realized pipe → not ready
#[tokio::test]
async fn running_dep_no_output_yields_not_ready() {
    let (mut dag, id_gen) = make_dag();
    let (pool, _) = make_pool(&id_gen);

    let dep = dag.add_node("dep".into(), NodeKind::Concrete);
    let node = dag.add_node("node".into(), NodeKind::Concrete);
    dag.add_dependency(For(node), DependsOn(dep));
    dag.set_state(dep, NodeState::Running);
    // no pipe realized

    assert!(!matches!(
        is_ready_to_spawn(node, &dag, &pool),
        SpawnReadiness::Ready
    ));
}

// Terminated dep with non-zero exit code and no pipe → don't start
#[tokio::test]
async fn terminated_dep_with_error_no_output_blocks() {
    let (mut dag, id_gen) = make_dag();
    let (pool, _) = make_pool(&id_gen);

    let dep = dag.add_node("dep".into(), NodeKind::Concrete);
    let node = dag.add_node("node".into(), NodeKind::Concrete);
    dag.add_dependency(For(node), DependsOn(dep));
    dag.set_state(dep, NodeState::Terminated);
    dag.set_exit_code(dep, EOWNERDEAD);

    assert!(!matches!(
        is_ready_to_spawn(node, &dag, &pool),
        SpawnReadiness::Ready
    ));
}

// Terminated dep with non-zero exit code even with realized pipe → don't start
#[tokio::test]
async fn terminated_dep_with_error_and_output_blocks() {
    let (mut dag, id_gen) = make_dag();
    let (pool, pool_id_gen) = make_pool(&id_gen);

    let dep = dag.add_node("dep".into(), NodeKind::Concrete);
    let node = dag.add_node("node".into(), NodeKind::Concrete);
    dag.add_dependency(For(node), DependsOn(dep));
    dag.set_state(dep, NodeState::Terminated);
    dag.set_exit_code(dep, EOWNERDEAD);
    pool.touch_writer(dep, StdHandle::Stdout as isize, &pool_id_gen)
        .await
        .unwrap();

    assert!(!matches!(
        is_ready_to_spawn(node, &dag, &pool),
        SpawnReadiness::Ready
    ));
}

// Terminated dep via alias → FailedDependency carries the concrete dep handle
#[tokio::test]
async fn terminated_dep_via_alias_is_failed_dependency() {
    let (mut dag, id_gen) = make_dag();
    let (pool, _) = make_pool(&id_gen);

    let dep = dag.add_node("dep".into(), NodeKind::Concrete);
    let alias = dag.add_node("alias".into(), NodeKind::Alias);
    let node = dag.add_node("node".into(), NodeKind::Concrete);
    dag.add_dependency(For(alias), DependsOn(dep));
    dag.add_dependency(For(node), DependsOn(alias));
    dag.set_state(dep, NodeState::Terminated);
    dag.set_exit_code(dep, EOWNERDEAD);

    assert!(matches!(
        is_ready_to_spawn(node, &dag, &pool),
        SpawnReadiness::FailedDependency(h) if h == dep
    ));
}

// Terminated dep (no pipe) then NotStarted dep → don't start
#[tokio::test]
async fn skip_then_not_started_blocks() {
    let (mut dag, id_gen) = make_dag();
    let (pool, _) = make_pool(&id_gen);

    let dep_terminated = dag.add_node("dep_t".into(), NodeKind::Concrete);
    let dep_pending = dag.add_node("dep_p".into(), NodeKind::Concrete);
    let node = dag.add_node("node".into(), NodeKind::Concrete);
    dag.add_dependency(For(node), DependsOn(dep_terminated));
    dag.add_dependency(For(node), DependsOn(dep_pending));
    dag.set_state(dep_terminated, NodeState::Terminated);
    // dep_pending stays NotStarted, no pipes realized

    assert!(!matches!(
        is_ready_to_spawn(node, &dag, &pool),
        SpawnReadiness::Ready
    ));
}
