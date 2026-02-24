use ailetos::idgen::Handle;
use ailetos::Environment;
use cli::sqlitekv::SqliteKV;
use tracing::info;

/// Minimal test with just one value node
fn build_flow(env: &mut Environment<SqliteKV>) -> Handle {
    // Single value node
    let val = env.add_value_node(
        "Hello, World!\n".as_bytes().to_vec(),
        Some("Single value node".to_string()),
    );

    val
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // Create KV store
    let _ = std::fs::remove_file("deb.db");
    let kv = SqliteKV::new("deb.db").expect("Failed to create SqliteKV");

    // Create environment
    let mut env = Environment::new(kv);

    // No actors to register - only value node

    // Build the flow
    let end_node = build_flow(&mut env);

    // Print dependency tree
    info!("Dependency tree:\n{}", env.dag.dump(end_node));

    // Run the system
    env.run(end_node).await;

    info!("Program completed");
}
