use std::sync::Arc;

use ailetos::suspension::SuspensionState;
use ailetos::system_runtime::{ChannelHandle, IoRequest};
use ailetos::{BlockingActorRuntime, Handle, IdGen, EPIPE, EOWNERDEAD};

/// When aread() receives EPIPE from the system runtime, get_errno() returns EPIPE
/// and mark_failed() uses EPIPE as the exit code (spec://errors#reader-to-actor).
#[tokio::test]
async fn test_reader_to_actor_epipe_propagation() {
    let id_gen = Arc::new(IdGen::new());
    let node_handle = Handle::new(id_gen.get_next());
    let (system_tx, mut system_rx) = tokio::sync::mpsc::unbounded_channel::<IoRequest>();
    let suspension = Arc::new(SuspensionState::new());

    let (runtime, _shutdown) =
        BlockingActorRuntime::new(node_handle, system_tx, Arc::clone(&suspension));
    runtime.register_std_fds();

    // Mock system runtime: MaterializeStdin → ChannelHandle, then Read → (-1, EPIPE)
    let io_task = tokio::spawn(async move {
        while let Some(req) = system_rx.recv().await {
            match req {
                IoRequest::MaterializeStdin { response, .. } => {
                    let _ = response.send(ChannelHandle(0));
                }
                IoRequest::Read { response, .. } => {
                    let _ = response.send((-1, EPIPE));
                    return;
                }
                _ => {}
            }
        }
    });

    // Run aread in a blocking context (it calls blocking_recv internally)
    let (read_result, errno_after_read) = tokio::task::spawn_blocking(move || {
        use actor_runtime::ActorRuntime;
        let mut buf = [0u8; 64];
        let n = runtime.aread(0, &mut buf);
        let errno = runtime.get_errno();
        // mark_failed uses last_read_errno: should use EPIPE, not EOWNERDEAD
        (n, errno)
    })
    .await
    .unwrap();

    assert_eq!(read_result, -1, "aread should return -1 on error");
    assert_eq!(errno_after_read, EPIPE as isize, "get_errno should return EPIPE");

    io_task.abort();
}

/// When mark_failed() is called after a read that returned EPIPE, the ActorShutdown
/// message carries EPIPE as the exit code.
#[tokio::test]
async fn test_mark_failed_uses_epipe_from_last_read() {
    let id_gen = Arc::new(IdGen::new());
    let node_handle = Handle::new(id_gen.get_next());
    let (system_tx, mut system_rx) = tokio::sync::mpsc::unbounded_channel::<IoRequest>();
    let suspension = Arc::new(SuspensionState::new());

    let (runtime, shutdown) =
        BlockingActorRuntime::new(node_handle, system_tx, Arc::clone(&suspension));
    runtime.register_std_fds();

    // Respond to MaterializeStdin + Read with EPIPE, then capture ActorShutdown exit_code
    let io_task = tokio::spawn(async move {
        let mut shutdown_exit_code = None;
        while let Some(req) = system_rx.recv().await {
            match req {
                IoRequest::MaterializeStdin { response, .. } => {
                    let _ = response.send(ChannelHandle(0));
                }
                IoRequest::Read { response, .. } => {
                    let _ = response.send((-1, EPIPE));
                }
                IoRequest::ActorShutdown { exit_code, .. } => {
                    shutdown_exit_code = Some(exit_code);
                    break;
                }
                _ => {}
            }
        }
        shutdown_exit_code
    });

    // Do a read that gets EPIPE, then fail the actor
    tokio::task::spawn_blocking(move || {
        use actor_runtime::ActorRuntime;
        let mut buf = [0u8; 64];
        runtime.aread(0, &mut buf);
        // actor "returns Err" → spawn_actor_task calls shutdown.mark_failed()
        shutdown.mark_failed();
        // drop(shutdown) fires do_shutdown → sends ActorShutdown
    })
    .await
    .unwrap();

    let exit_code = io_task.await.unwrap();
    assert_eq!(exit_code, Some(EPIPE), "ActorShutdown exit_code should be EPIPE");
}

/// When mark_failed() is called with no prior read error, exit code is EOWNERDEAD.
#[tokio::test]
async fn test_mark_failed_uses_eownerdead_without_read_error() {
    let id_gen = Arc::new(IdGen::new());
    let node_handle = Handle::new(id_gen.get_next());
    let (system_tx, mut system_rx) = tokio::sync::mpsc::unbounded_channel::<IoRequest>();
    let suspension = Arc::new(SuspensionState::new());

    let (_runtime, shutdown) =
        BlockingActorRuntime::new(node_handle, system_tx, suspension);

    let io_task = tokio::spawn(async move {
        let mut shutdown_exit_code = None;
        while let Some(req) = system_rx.recv().await {
            if let IoRequest::ActorShutdown { exit_code, .. } = req {
                shutdown_exit_code = Some(exit_code);
                break;
            }
        }
        shutdown_exit_code
    });

    tokio::task::spawn_blocking(move || {
        shutdown.mark_failed();
        // drop fires do_shutdown
    })
    .await
    .unwrap();

    let exit_code = io_task.await.unwrap();
    assert_eq!(exit_code, Some(EOWNERDEAD), "ActorShutdown exit_code should be EOWNERDEAD when no read error");
}
