//! DAG Shell binary entry point.

use dagsh::model_aliases::resolve_alias;
use dagsh::user_input::StdinUsage;
use dagsh::shell_ui::{create_notification_sink, parse_args, print_usage, ShellHelper};
use dagsh::{make_interp, DagShell, ShellControl};
use rustyline::config::Configurer;
use rustyline::error::ReadlineError;
use rustyline::Editor;
use tokio::runtime::Runtime;

struct LlmConfig {
    model: Option<String>,
    url: Option<String>,
    thinking: Option<String>,
    stream: Option<String>,
}

// Priority for URL: --llm-url flag > alias-derived > AILETS_LLM_URL env var.
fn resolve_llm_config(
    cli_model: Option<String>,
    cli_llm_url: Option<String>,
    cli_llm_thinking: Option<String>,
) -> LlmConfig {
    let model_input = cli_model.or_else(|| std::env::var("AILETS_MODEL").ok());
    let llm_url_env = std::env::var("AILETS_LLM_URL").ok();
    let llm_thinking = cli_llm_thinking.or_else(|| std::env::var("AILETS_LLM_THINKING").ok());
    let llm_stream = std::env::var("AILETS_LLM_STREAM").ok();

    let (effective_model, alias_url) = match model_input {
        Some(ref m) => match resolve_alias(m) {
            Some(r) => (
                r.model.map(str::to_string).or(model_input.clone()),
                Some(r.url.to_string()),
            ),
            None => (model_input.clone(), None),
        },
        None => (None, None),
    };

    LlmConfig {
        model: effective_model,
        url: cli_llm_url.or(alias_url).or(llm_url_env),
        thinking: llm_thinking,
        stream: llm_stream,
    }
}

fn run_repl(
    rl: &mut Editor<ShellHelper, rustyline::history::DefaultHistory>,
    mut shell: DagShell,
    interp: &mut molt::Interp,
    ctx: molt::types::ContextID,
) {
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
                match shell.execute(interp, ctx, line) {
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
    // shell must drop before printer_rt: dropping shell at end of this function,
    // before printer_rt drops in main, preserves the required ordering.
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
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

    let printer_rt = Runtime::new()?;
    let notification_sink = create_notification_sink(&mut rl, printer_rt.handle());
    let ailetos_rt = Runtime::new()?;
    let mut shell =
        DagShell::new_with_sinks_and_rt(Box::new(dagsh::StdoutSink), notification_sink, ailetos_rt);

    let llm_config = resolve_llm_config(cli_args.model, cli_args.llm_url, cli_args.llm_thinking);
    if let Some(m) = llm_config.model {
        shell.set_var("AILETS_MODEL", &m);
    }
    if let Some(u) = llm_config.url {
        shell.set_var("AILETS_LLM_URL", &u);
    }
    if let Some(t) = llm_config.thinking {
        shell.set_var("AILETS_LLM_THINKING", &t);
    }
    if let Some(s) = llm_config.stream {
        shell.set_var("AILETS_LLM_STREAM", &s);
    }

    let (mut interp, ctx) = make_interp();

    println!("DAG Shell v0.1 (TCL)");
    println!("Type 'help' for available commands.\n");

    // Stdin is only treated as data when the user explicitly writes `-` or `@-`.
    let stdin_usage = match shell.register_prompt_inputs(&cli_args.prompt_items) {
        Ok(usage) => usage,
        Err(e) => {
            eprintln!("Error building prompt nodes: {e}");
            std::process::exit(1);
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
        return Ok(());
    }

    run_repl(&mut rl, shell, &mut interp, ctx);
    // printer_rt drops here, after shell has already dropped inside run_repl.
    Ok(())
}
