use std::sync::Arc;

use actor_runtime::StdHandle;
use ailetos::actor_syscall::lifecycle_event::ActorLifecycleEvent;
use ailetos::dag::{DependsOn, For, NodeKind, NodeState};
use ailetos::environment::Environment;
use ailetos::idgen::Handle;
use ailetos::storage::{KVBuffers, MemKV};
use ailetos::suspension::SuspensionState;
use ailetos::{BlockingActorRuntime, IoBridge, EOWNERDEAD, EPIPE};
use tokio::sync::{mpsc, Notify};

fn make_test_components() -> (
    Arc<Environment>,
    Arc<IoBridge>,
    mpsc::UnboundedSender<ActorLifecycleEvent>,
    tokio::task::JoinHandle<Option<i32>>,
) {
    let kv: Arc<dyn KVBuffers> = Arc::new(MemKV::new());
    let env = Arc::new(Environment::new(kv));
    let notify = Arc::new(Notify::new());
    let (actor_done_tx, mut actor_done_rx) = mpsc::unbounded_channel::<ActorLifecycleEvent>();

    let bridge = Arc::new(IoBridge::new(Arc::clone(&env), notify));

    // Lifecycle handler: replies to Terminating/Terminated, captures exit_code
    let lifecycle_task = tokio::spawn(async move {
        let mut last_exit_code = None;
        while let Some(event) = actor_done_rx.recv().await {
            match event {
                ActorLifecycleEvent::Terminating { reply, .. } => {
                    let _ = reply.send(NodeState::Running);
                }
                ActorLifecycleEvent::Terminated {
                    exit_code, reply, ..
                } => {
                    let _ = reply.send(NodeState::Terminating);
                    last_exit_code = Some(exit_code);
                }
            }
        }
        last_exit_code
    });

    (env, bridge, actor_done_tx, lifecycle_task)
}

/// Add a dependency node and an actor node to the environment's DAG.
/// Returns (dep_handle, actor_handle).
fn add_dag_with_dep(env: &Environment) -> (Handle, Handle) {
    let mut dag = env.dag.write();
    let dep = dag.add_node("dep".into(), NodeKind::Concrete);
    let actor = dag.add_node("actor".into(), NodeKind::Concrete);
    dag.add_dependency(For(actor), DependsOn(dep));
    dag.set_state(dep, NodeState::Running);
    (dep, actor)
}

/// When aread() receives EPIPE from the bridge, get_errno() returns EPIPE
/// and mark_failed() uses EPIPE as the exit code (spec://errors#reader-to-actor).
#[tokio::test]
async fn test_reader_to_actor_epipe_propagation() {
    let (env, bridge, actor_done_tx, lifecycle_task) = make_test_components();
    let (dep_handle, actor_handle) = add_dag_with_dep(&env);
    let suspension = Arc::new(SuspensionState::new());

    // Realize dep's pipe and set EPIPE error so MergeReader sees it
    let (writer, _) = env
        .pipe_pool
        .touch_writer(dep_handle, StdHandle::Stdout as isize, &env.idgen)
        .await
        .unwrap();
    writer.set_error(EPIPE);
    writer.close();

    let runtime = Arc::new(BlockingActorRuntime::new(
        actor_handle,
        Arc::clone(&bridge),
        Arc::clone(&suspension),
        actor_done_tx,
    ));
    runtime.register_std_fds();

    let runtime_clone = Arc::clone(&runtime);
    let (read_result, errno_after_read) = tokio::task::spawn_blocking(move || {
        use actor_runtime::ActorRuntime;
        let mut buf = [0u8; 64];
        let n = runtime_clone.aread(0, &mut buf);
        let errno = runtime_clone.get_errno();
        (n, errno)
    })
    .await
    .unwrap();

    assert_eq!(read_result, -1, "aread should return -1 on error");
    assert_eq!(
        errno_after_read, EPIPE as isize,
        "get_errno should return EPIPE"
    );

    runtime.shutdown().await.unwrap();
    drop(runtime);
    drop(bridge);
    lifecycle_task.await.unwrap();
}

/// When mark_failed() is called after a read that returned EPIPE, the shutdown
/// carries EPIPE as the exit code.
#[tokio::test]
async fn test_mark_failed_uses_epipe_from_last_read() {
    let (env, bridge, actor_done_tx, lifecycle_task) = make_test_components();
    let (dep_handle, actor_handle) = add_dag_with_dep(&env);
    let suspension = Arc::new(SuspensionState::new());

    let (writer, _) = env
        .pipe_pool
        .touch_writer(dep_handle, StdHandle::Stdout as isize, &env.idgen)
        .await
        .unwrap();
    writer.set_error(EPIPE);
    writer.close();

    let runtime = Arc::new(BlockingActorRuntime::new(
        actor_handle,
        Arc::clone(&bridge),
        Arc::clone(&suspension),
        actor_done_tx,
    ));
    runtime.register_std_fds();

    let runtime_clone = Arc::clone(&runtime);
    tokio::task::spawn_blocking(move || {
        use actor_runtime::ActorRuntime;
        let mut buf = [0u8; 64];
        runtime_clone.aread(0, &mut buf);
        runtime_clone.mark_failed();
    })
    .await
    .unwrap();

    // Explicitly shutdown to flush buffers and notify executor
    runtime.shutdown().await.unwrap();
    drop(runtime);
    drop(bridge);
    let exit_code = lifecycle_task.await.unwrap();
    assert_eq!(exit_code, Some(EPIPE), "exit code should be EPIPE");
}

/// When mark_failed() is called with no prior read error, exit code is EOWNERDEAD.
#[tokio::test]
async fn test_mark_failed_uses_eownerdead_without_read_error() {
    let (env, bridge, actor_done_tx, lifecycle_task) = make_test_components();
    let actor_handle = Handle::new(env.idgen.get_next());
    let suspension = Arc::new(SuspensionState::new());

    let runtime = Arc::new(BlockingActorRuntime::new(
        actor_handle,
        Arc::clone(&bridge),
        suspension,
        actor_done_tx,
    ));

    let runtime_clone = Arc::clone(&runtime);
    tokio::task::spawn_blocking(move || {
        runtime_clone.mark_failed();
    })
    .await
    .unwrap();

    // Explicitly shutdown to flush buffers and notify executor
    runtime.shutdown().await.unwrap();
    drop(runtime);
    drop(bridge);
    let exit_code = lifecycle_task.await.unwrap();
    assert_eq!(
        exit_code,
        Some(EOWNERDEAD),
        "exit code should be EOWNERDEAD"
    );
}
