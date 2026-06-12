//! DAG Shell binary entry point.

use dagsh::shell_ui::{create_notification_sink, parse_args, print_usage, ShellHelper};
use dagsh::{make_tcl, DagShell, ShellControl};
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
    let mut tcl = make_tcl();

    println!("DAG Shell v0.1 (TCL)");
    println!("Type 'help' for available commands.\n");

    if let Some(script_path) = cli_args.load_script {
        println!("Loading {script_path}...\n");
        if let Err(e) = shell.cmd_source(&mut tcl, &[&script_path]) {
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
                if let Err(e) = rl.add_history_entry(line) {
                    eprintln!("warn: failed to add history entry: {e}");
                }
                match shell.execute(&mut tcl, line) {
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

    // shell must drop before _printer_rt: dropping shell closes the ChannelSink
    // sender, letting the spawn_blocking receiver task exit before the runtime shuts down.
    drop(shell);
    drop(printer_rt);
}
