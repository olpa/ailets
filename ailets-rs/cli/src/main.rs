#![allow(clippy::expect_used, clippy::panic, clippy::disallowed_names)]

use std::io::IsTerminal;
use std::sync::Arc;

use ailetos::idgen::Handle;
use ailetos::{Environment, SqliteKV};
use cli::stdin_source;

fn build_flow(env: &mut Environment<SqliteKV>) -> Handle {
    let val = env.add_value_node(
        "(mee too)".as_bytes().to_vec(),
        Some("Static text".to_string()),
    );
    let stdin = env.add_node(
        "stdin".to_string(),
        &[],
        Some("Read from stdin".to_string()),
    );
    let foo = env.add_node("cat".to_string(), &[stdin], Some("Copy.foo".to_string()));
    let bar = env.add_node("cat".to_string(), &[val, foo], Some("Copy.bar".to_string()));
    let baz = env.add_node("cat".to_string(), &[bar], Some("Copy.baz".to_string()));

    env.add_alias(".end".to_string(), baz)
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
    let kv = Arc::new(SqliteKV::new("example.db").expect("Failed to create SqliteKV"));

    // Create environment
    let mut env = Environment::new(Arc::clone(&kv));

    // Register actors in the environment
    // Note: "value" nodes are handled specially by the Environment, no actor needed
    env.actor_registry.register("stdin", stdin_source::execute);
    env.actor_registry.register("cat", cat::execute);

    // Build the flow
    let end_node = build_flow(&mut env);

    // Print dependency tree (with colors if stdout is a terminal)
    let tree = if std::io::stdout().is_terminal() {
        env.dag.dump_colored(end_node)
    } else {
        env.dag.dump(end_node)
    };
    println!("Dependency tree:\n{tree}");

    // TODO: Attach host stdout to the output actor

    // Run the system (matches Python: env.processes.run_nodes(node_iter))
    env.run(end_node).await;

    // Shutdown the KV store
    Arc::try_unwrap(kv)
        .unwrap_or_else(|_| panic!("KV still has other references"))
        .shutdown()
        .await
        .expect("Failed to shutdown KV");
}
