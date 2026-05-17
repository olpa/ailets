//! DAG Shell binary entry point.

use dagsh::DagShell;
use rustyline::config::Configurer;
use rustyline::error::ReadlineError;
use rustyline::Editor;

fn print_usage() {
    println!("Usage: dagsh [OPTIONS]");
    println!();
    println!("Options:");
    println!("  -l, --load <file>   Load script file on startup, then continue interactively");
    println!("  -h, --help          Show this help");
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args: Vec<String> = std::env::args().collect();

    let mut load_script: Option<String> = None;
    let mut i = 1;
    while i < args.len() {
        let Some(arg) = args.get(i) else { break };
        match arg.as_str() {
            "-h" | "--help" => {
                print_usage();
                return;
            }
            "-l" | "--load" => {
                let Some(path) = args.get(i + 1) else {
                    eprintln!("Error: --load requires a file argument");
                    std::process::exit(1);
                };
                load_script = Some(path.clone());
                i += 2;
            }
            a if a.starts_with('-') => {
                eprintln!("Unknown option: {a}");
                print_usage();
                std::process::exit(1);
            }
            a => {
                eprintln!("Unexpected argument: {a}");
                print_usage();
                std::process::exit(1);
            }
        }
    }

    let mut shell = DagShell::new();
    let Ok(mut rl) = Editor::<(), rustyline::history::DefaultHistory>::new() else {
        eprintln!("Failed to create editor");
        std::process::exit(1);
    };
    let _ = rl.set_max_history_size(1000);

    println!("DAG Shell v0.1");
    println!("Type 'help' for available commands.\n");

    if let Some(script_path) = load_script {
        println!("Loading {script_path}...\n");
        if let Err(e) = shell.cmd_source(&[&script_path]) {
            println!("Error: {e}");
        }
        println!();
    }

    loop {
        match rl.readline("dagsh> ") {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let _ = rl.add_history_entry(line);
                match shell.execute(line) {
                    Ok(true) => {}
                    Ok(false) => {
                        println!("Goodbye!");
                        break;
                    }
                    Err(e) => println!("Error: {e}"),
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("^C");
            }
            Err(ReadlineError::Eof) => {
                println!("Goodbye!");
                break;
            }
            Err(err) => {
                println!("Error: {err:?}");
                break;
            }
        }
    }
}
