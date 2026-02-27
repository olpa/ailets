mod sqlitekv;
mod stdin_source;

use ailetos::idgen::Handle;
use ailetos::Environment;
use sqlitekv::SqliteKV;
use tracing::info;

/// Build the data flow graph
///
/// This matches the Python version in example.py:
/// ```python
/// def build_flow(env: Environment) -> None:
///     val = env.dagops.add_value_node(...)
///     stdin = env.dagops.add_node("stdin", stdin_actor, [], ...)
///     foo = env.dagops.add_node("foo", copy_actor, [Dependency(stdin.name)], ...)
///     bar = env.dagops.add_node("bar", copy_actor, [Dependency(val.name), Dependency(foo.name)], ...)
///     baz = env.dagops.add_node("baz", copy_actor, [Dependency(bar.name)], ...)
///     env.dagops.alias(".end", baz.name)
/// ```
fn build_flow(env: &mut Environment<SqliteKV>) -> Handle {
    let val = env.add_value_node(
        "(mee too)".as_bytes().to_vec(),
        Some("Static text".to_string()),
    );
    let stdin = env.add_node("stdin".to_string(), &[], Some("Read from stdin".to_string()));
    let foo = env.add_node("cat".to_string(), &[stdin], Some("Copy".to_string()));
    let bar = env.add_node("cat".to_string(), &[val, foo], Some("Copy".to_string()));
    let baz = env.add_node("cat".to_string(), &[bar], Some("Copy".to_string()));
    let end = env.add_alias(".end".to_string(), baz);

    end
}

#[tokio::main]
async fn main() {
    // Initialize tracing subscriber
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // Create key-value store
    let _ = std::fs::remove_file("example.db");
    let kv = SqliteKV::new("example.db").expect("Failed to create SqliteKV");

    // Create environment
    let mut env = Environment::new(kv);

    // Register actors in the environment
    // Note: "value" nodes are handled specially by the Environment, no actor needed
    env.actor_registry.register("stdin", stdin_source::execute);
    env.actor_registry.register("cat", cat::execute);

    // Build the flow
    let end_node = build_flow(&mut env);

    // Print dependency tree
    info!("Dependency tree:\n{}", env.dag.dump(end_node));

    // TODO: Attach host stdout to the output actor

    // Run the system (matches Python: env.processes.run_nodes(node_iter))
    env.run(end_node).await;
}
