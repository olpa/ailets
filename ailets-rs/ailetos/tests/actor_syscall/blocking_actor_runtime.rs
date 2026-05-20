use std::sync::Arc;

use actor_runtime::StdHandle;
use ailetos::actor_syscall::lifecycle_event::ActorLifecycleEvent;
use ailetos::dag::{DependsOn, For, NodeKind, NodeState};
use ailetos::environment::Environment;
use ailetos::idgen::Handle;
use ailetos::storage::{KVBuffers, MemKV};
use ailetos::suspension::SuspensionState;
use ailetos::{
    AttachmentConfig, AttachmentManager, BlockingActorRuntime, IoBridge, EOWNERDEAD, EPIPE,
};
use parking_lot::RwLock;
use tokio::sync::{mpsc, oneshot, Notify};

/// Create a standalone attachment manager for testing
fn standalone_attachment_manager(config: AttachmentConfig) -> AttachmentManager {
    AttachmentManager::new(Arc::new(RwLock::new(config)))
}

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

    let attachment_manager = Arc::new(standalone_attachment_manager(AttachmentConfig::new()));
    let bridge = Arc::new(IoBridge::new(Arc::clone(&env), attachment_manager, notify));

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

/// When aread() receives EPIPE from the bridge, it returns Err(EPIPE).
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
    assert!(writer.close().is_ok());

    let runtime = BlockingActorRuntime::new(
        actor_handle,
        Arc::clone(&bridge),
        Arc::clone(&suspension),
        actor_done_tx,
    );
    runtime.register_std_fds();

    let (tx, rx) = oneshot::channel();
    tokio::task::spawn_blocking(move || {
        use actor_runtime::ActorRuntime;
        let mut buf = [0u8; 64];
        let result = runtime.aread(0, &mut buf);
        let _ = tx.send((result, runtime));
    });
    let (read_result, mut runtime) = rx.await.expect("channel closed");

    assert_eq!(read_result, Err(EPIPE), "aread should return Err(EPIPE)");

    runtime.shutdown().await.unwrap();
    drop(runtime);
    drop(bridge);
    lifecycle_task.await.unwrap();
}

/// When latch_errno() is called with an errno, the shutdown carries that errno
/// as the exit code.
#[tokio::test]
async fn test_latch_errno_with_errno() {
    let (env, bridge, actor_done_tx, lifecycle_task) = make_test_components();
    let (dep_handle, actor_handle) = add_dag_with_dep(&env);
    let suspension = Arc::new(SuspensionState::new());

    let (writer, _) = env
        .pipe_pool
        .touch_writer(dep_handle, StdHandle::Stdout as isize, &env.idgen)
        .await
        .unwrap();
    writer.set_error(EPIPE);
    assert!(writer.close().is_ok());

    let runtime = BlockingActorRuntime::new(
        actor_handle,
        Arc::clone(&bridge),
        Arc::clone(&suspension),
        actor_done_tx,
    );
    runtime.register_std_fds();

    let (tx, rx) = oneshot::channel();
    tokio::task::spawn_blocking(move || {
        use actor_runtime::ActorRuntime;
        let mut runtime = runtime;
        let mut buf = [0u8; 64];
        if let Err(errno) = runtime.aread(0, &mut buf) {
            runtime.latch_errno(errno);
        }
        let _ = tx.send(runtime);
    });
    let mut runtime = rx.await.expect("channel closed");

    // Explicitly shutdown to flush buffers and notify executor
    runtime.shutdown().await.unwrap();
    drop(runtime);
    drop(bridge);
    let exit_code = lifecycle_task.await.unwrap();
    assert_eq!(exit_code, Some(EPIPE), "exit code should be EPIPE");
}

/// When latch_errno() is called with EOWNERDEAD, exit code is EOWNERDEAD.
#[tokio::test]
async fn test_latch_errno_with_eownerdead() {
    let (env, bridge, actor_done_tx, lifecycle_task) = make_test_components();
    let actor_handle = Handle::new(env.idgen.get_next());
    let suspension = Arc::new(SuspensionState::new());

    let runtime =
        BlockingActorRuntime::new(actor_handle, Arc::clone(&bridge), suspension, actor_done_tx);

    let (tx, rx) = oneshot::channel();
    tokio::task::spawn_blocking(move || {
        let mut runtime = runtime;
        runtime.latch_errno(EOWNERDEAD);
        let _ = tx.send(runtime);
    });
    let mut runtime = rx.await.expect("channel closed");

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
