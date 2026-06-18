//! DAG Shell binary entry point.

use dagsh::prompt_nodes::StdinUsage;
use dagsh::shell_ui::{create_notification_sink, parse_args, print_usage, ShellHelper};
use dagsh::{make_interp, DagShell, ShellControl};
use rustyline::config::Configurer;
use rustyline::error::ReadlineError;
use rustyline::Editor;
use tokio::runtime::Runtime;

#[allow(clippy::expect_used)]
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

    let Ok(mut rl) = Editor::<ShellHelper, rustyline::history::DefaultHistory>::new() else {
        eprintln!("Failed to create editor");
        std::process::exit(1);
    };
    rl.set_helper(Some(ShellHelper));
    if let Err(e) = rl.set_max_history_size(1000) {
        eprintln!("warn: failed to set history size: {e}");
    }

    let printer_rt = Runtime::new().expect("failed to create printer runtime");
    let notification_sink = create_notification_sink(&mut rl, printer_rt.handle());
    let ailetos_rt = Runtime::new().expect("failed to create ailetos runtime");
    let mut shell =
        DagShell::new_with_sinks_and_rt(Box::new(dagsh::StdoutSink), notification_sink, ailetos_rt);
    let (mut interp, ctx) = make_interp();

    println!("DAG Shell v0.1 (TCL)");
    println!("Type 'help' for available commands.\n");

    // Stdin is only treated as data when the user explicitly writes `-` or `@-`.
    let stdin_usage = if cli_args.prompt_items.is_empty() {
        StdinUsage::DagShell
    } else {
        match shell.register_prompt_inputs(&cli_args.prompt_items) {
            Ok(usage) => usage,
            Err(e) => {
                eprintln!("Error building prompt nodes: {e}");
                std::process::exit(1);
            }
        }
    };

    // Run all load scripts in order.
    for script_path in &cli_args.load_scripts {
        println!("Loading {script_path}...");
        if let Err(e) = shell.cmd_source(&mut interp, ctx, &[script_path.as_str()]) {
            println!("Error: {e}");
        }
        println!();
    }

    // Exit without interactive shell when stdin is wired into a DAG file_value actor.
    if stdin_usage == StdinUsage::FileValue {
        drop(shell);
        drop(printer_rt);
        return;
    }

    // Interactive REPL.
    loop {
        match rl.readline("dagsh> ") {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if let Err(e) = rl.add_history_entry(line) {
                    eprintln!("warn: failed to add history entry: {e}");
                }
                match shell.execute(&mut interp, ctx, line) {
                    Ok(ShellControl::Continue) => {}
                    Ok(ShellControl::Exit) => {
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

    // shell must drop before printer_rt: dropping shell closes the ChannelSink
    // sender, letting the spawn_blocking receiver task exit before the runtime shuts down.
    drop(shell);
    drop(printer_rt);
}
