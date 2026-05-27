//! DAG Shell binary entry point.

use dagsh::shell_ui::{create_notification_sink, parse_args, print_usage};
use dagsh::DagShell;
use tokio::runtime::Runtime;
use rustyline::config::Configurer;
use rustyline::error::ReadlineError;
use rustyline::Editor;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args: Vec<String> = std::env::args().collect();

    let cli_args = match parse_args(&args) {
        Ok(args) => args,
        Err(e) => {
            eprintln!("Error: {e}");
            print_usage();
            std::process::exit(1);
        }
    };

    let Ok(mut rl) = Editor::<(), rustyline::history::DefaultHistory>::new() else {
        eprintln!("Failed to create editor");
        std::process::exit(1);
    };
    let _ = rl.set_max_history_size(1000);

    let printer_rt = Runtime::new().expect("failed to create printer runtime");
    let notification_sink = create_notification_sink(&mut rl, printer_rt.handle());
    let ailetos_rt = Runtime::new().expect("failed to create ailetos runtime");
    let mut shell = DagShell::new_with_sinks_and_rt(Box::new(dagsh::StdoutSink), notification_sink, ailetos_rt);
    // Keep printer_rt alive for the process lifetime.
    let _printer_rt = printer_rt;

    println!("DAG Shell v0.1");
    println!("Type 'help' for available commands.\n");

    if let Some(script_path) = cli_args.load_script {
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

    // shell must drop before _printer_rt: dropping shell closes the ChannelSink
    // sender, letting the spawn_blocking receiver task exit before the runtime shuts down.
    drop(shell);
    drop(_printer_rt);
}
