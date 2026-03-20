//! Example: A simple DAG flow that reads from stdin and pipes through cat actors.
//!
//! This demonstrates:
//! - Creating an Environment with SqliteKV storage
//! - Registering custom actors (stdin source and cat)
//! - Building a DAG flow with value nodes and processing nodes
//! - Running the flow and attaching stdout

#![allow(clippy::expect_used, clippy::panic)]

use std::io::IsTerminal;
use std::sync::Arc;

use actor_io::{error_kind_to_str, AReader, AWriter};
use ailetos::idgen::Handle;
use ailetos::{Environment, SqliteKV};
use embedded_io::Write;
use std::io::Read;

/// Stdin source actor: reads from OS stdin and writes to actor stdout
fn stdin_actor<'a>(_reader: AReader<'a>, mut writer: AWriter<'a>) -> Result<(), String> {
    let mut stdin = std::io::stdin();
    let mut buffer = [0u8; 8192];

    loop {
        match stdin.read(&mut buffer) {
            Ok(0) => break, // EOF
            Ok(n) => {
                let Some(data) = buffer.get(..n) else {
                    return Err("Buffer slice out of bounds".to_string());
                };
                if let Err(e) = writer.write_all(data) {
                    let error_msg = error_kind_to_str(e);
                    return Err(format!("Failed to write: {error_msg}"));
                }
            }
            Err(e) => {
                return Err(format!("Failed to read from stdin: {e}"));
            }
        }
    }

    Ok(())
}

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
    #[allow(clippy::disallowed_names)]
    let foo = env.add_node("cat".to_string(), &[stdin], Some("Copy.foo".to_string()));
    #[allow(clippy::disallowed_names)]
    let bar = env.add_node("cat".to_string(), &[val, foo], Some("Copy.bar".to_string()));
    #[allow(clippy::disallowed_names)]
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
    env.actor_registry.register("stdin", stdin_actor);
    env.actor_registry.register("cat", cat::execute);

    // Build the flow
    let end_node = build_flow(&mut env);

    // Print dependency tree (with colors if stdout is a terminal)
    let tree = if std::io::stdout().is_terminal() {
        env.dag.read().dump_colored(end_node)
    } else {
        env.dag.read().dump(end_node)
    };
    print!("{tree}");

    // Attach the last actor's stdout to host stdout
    env.attach_stdout(env.resolve(end_node));

    // Run the system
    env.run(end_node).await;

    // Drop environment to release KV reference
    drop(env);

    // Shutdown the KV store
    Arc::try_unwrap(kv)
        .unwrap_or_else(|_| panic!("KV still has other references"))
        .shutdown()
        .await
        .expect("Failed to shutdown KV");
}
