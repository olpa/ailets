mod scheduler;
mod sqlitekv;
mod stdin_source;
mod val;

use std::sync::Arc;

use actor_io::{AReader, AWriter};
use actor_runtime::StdHandle;
use ailetos::dag::{Dag, DependsOn, For, NodeKind};
use ailetos::idgen::{Handle, IdGen};
use ailetos::KVBuffers;
use ailetos::{IoRequest, StubActorRuntime, SystemRuntime};
use scheduler::Scheduler;
use sqlitekv::SqliteKV;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// Spawn actor tasks for each node in the system
fn spawn_actor_tasks(
    dag: &Dag,
    target: Handle,
    system_tx: mpsc::UnboundedSender<IoRequest>,
) -> Vec<tokio::task::JoinHandle<()>> {
    let scheduler = Scheduler::new(dag, target);
    let mut tasks = Vec::new();

    for node_handle in scheduler.iter() {
        let node = dag.get_node(node_handle).expect("node exists");
        let idname = node.idname.clone();
        debug!(node = ?node_handle, name = %idname, "spawning actor task");

        // Get dependencies for this node
        let dependencies: Vec<Handle> = dag.get_direct_dependencies(node_handle).collect();

        // Create runtime for this actor
        let runtime = StubActorRuntime::new(node_handle, system_tx.clone());

        let task = tokio::task::spawn_blocking(move || {
            debug!(node = ?node_handle, name = %idname, "task starting");

            // Request SystemRuntime to setup std handles before actor runs
            runtime.request_std_handles_setup(dependencies);

            // Create reader and writer unconditionally
            let areader = AReader::new_from_std(&runtime, StdHandle::Stdin);
            let awriter = AWriter::new_from_std(&runtime, StdHandle::Stdout);

            // Execute the appropriate actor based on idname
            let result = match idname.as_str() {
                "val" => val::execute(areader, awriter),
                "stdin" => stdin_source::execute(areader, awriter),
                _ => cat::execute(areader, awriter),
            };

            match result {
                Ok(()) => debug!(node = ?node_handle, name = %idname, "task completed"),
                Err(e) => warn!(node = ?node_handle, name = %idname, error = %e, "task error"),
            }

            // Close all handles after actor finishes
            runtime.close_all_handles();

            debug!(node = ?node_handle, name = %idname, "task done");
        });
        tasks.push(task);
    }

    tasks
}

/// Run the system: spawn system runtime and actor tasks, wait for completion
async fn run_system<K: KVBuffers + 'static>(
    system_runtime: SystemRuntime<K>,
    dag: &Dag,
    target: Handle,
) {
    // Get sender before moving system_runtime
    let system_tx = system_runtime.get_system_tx();

    // Spawn SystemRuntime task
    let system_task = tokio::spawn(async move {
        system_runtime.run().await;
    });

    // Spawn actor tasks
    let actor_tasks = spawn_actor_tasks(dag, target, system_tx);

    // Wait for system runtime
    if let Err(e) = system_task.await {
        warn!(error = %e, "SystemRuntime task failed");
    }

    // Wait for all actor tasks
    for task in actor_tasks {
        if let Err(e) = task.await {
            warn!(error = %e, "actor task failed");
        }
    }
}

fn build_flow(dag: &mut Dag) -> Handle {
    // val: value node (pre-filled with "(mee too)")
    let val = dag.add_node("val".into(), NodeKind::Concrete);

    // stdin: reads from stdin
    // TODO: implement actual OS stdin reading, currently simulated with pre-filled pipe
    let stdin = dag.add_node("stdin".into(), NodeKind::Concrete);

    // foo: copies from stdin
    let foo = dag.add_node("foo".into(), NodeKind::Concrete);
    dag.add_dependency(For(foo), DependsOn(stdin));

    // bar: copies from val
    // TODO: bar should also depend on foo, but multiple inputs are not yet supported.
    // For now, we only read from val and ignore foo.
    let bar = dag.add_node("bar".into(), NodeKind::Concrete);
    dag.add_dependency(For(bar), DependsOn(val));

    // baz: copies from bar
    let baz = dag.add_node("baz".into(), NodeKind::Concrete);
    dag.add_dependency(For(baz), DependsOn(bar));

    // .end alias to baz
    let end = dag.add_node(".end".into(), NodeKind::Alias);
    dag.add_dependency(For(end), DependsOn(baz));

    end
}

#[tokio::main]
async fn main() {
    // Initialize tracing subscriber
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
        )
        .init();

    // Create DAG and build flow
    let idgen = Arc::new(IdGen::new());
    let mut dag = Dag::new(Arc::clone(&idgen));
    let end_node = build_flow(&mut dag);

    // Print dependency tree
    info!("Dependency tree:\n{}", dag.dump(end_node));

    // Create key-value store for pipe buffers
    let _ = std::fs::remove_file("example.db");
    let kv = SqliteKV::new("example.db").expect("Failed to create SqliteKV");

    // Create system runtime
    let system_runtime = SystemRuntime::new(kv, idgen);

    // Run the system
    run_system(system_runtime, &dag, end_node).await;
}
