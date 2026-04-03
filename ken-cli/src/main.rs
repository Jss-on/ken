use anyhow::Result;
use clap::Parser;
use ken_runtime::{Config, Session, TradingRuntime};
use rustyline::DefaultEditor;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "ken", about = "Trading strategy design agent")]
struct Cli {
    /// One-shot prompt (skip REPL)
    #[arg(short, long)]
    prompt: Option<String>,

    /// Resume a previous session by ID
    #[arg(short, long)]
    resume: Option<String>,

    /// Show token usage at end of session
    #[arg(long, default_value_t = true)]
    show_tokens: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = Config::load();

    if let Err(e) = config.validate() {
        eprintln!("Configuration error: {e}");
        eprintln!();
        eprintln!("Quick setup:");
        eprintln!("  export KEN_API_KEY=your-anthropic-api-key");
        eprintln!("  # or create ~/.ken/config.json with: {{\"api_key\": \"your-key\"}}");
        std::process::exit(1);
    }

    let session = if let Some(ref id) = cli.resume {
        let sessions_dir = sessions_dir();
        Session::load(&sessions_dir, id)?
    } else {
        Session::new()
    };

    let mut runtime = TradingRuntime::new(config, session)?;

    println!("Ken v0.1.0 — Trading Strategy Design Agent");
    println!("Session: {}", runtime.session().id);
    println!("Type your setup description, or 'exit' to quit.\n");

    if let Some(prompt) = cli.prompt {
        // One-shot mode
        let response = runtime.run_turn(&prompt)?;
        println!("{response}");
    } else {
        // REPL mode
        repl(&mut runtime)?;
    }

    // Save session
    let sessions_dir = sessions_dir();
    runtime.session().save(&sessions_dir)?;

    // Show token usage
    if cli.show_tokens {
        let s = runtime.session();
        println!(
            "\n--- Session stats ---\nInput tokens: {}\nOutput tokens: {}\nTotal: {}",
            s.total_input_tokens,
            s.total_output_tokens,
            s.total_input_tokens + s.total_output_tokens
        );
    }

    Ok(())
}

fn repl(runtime: &mut TradingRuntime) -> Result<()> {
    let mut rl = DefaultEditor::new()?;

    loop {
        let readline = rl.readline("ken> ");
        match readline {
            Ok(line) => {
                let input = line.trim();
                if input.is_empty() {
                    continue;
                }
                if input == "exit" || input == "quit" {
                    break;
                }

                let _ = rl.add_history_entry(input);

                match runtime.run_turn(input) {
                    Ok(response) => {
                        if !response.is_empty() {
                            println!("\n{response}\n");
                        }
                    }
                    Err(e) => {
                        eprintln!("Error: {e}");
                    }
                }
            }
            Err(rustyline::error::ReadlineError::Interrupted) => {
                println!("Use 'exit' to quit.");
            }
            Err(rustyline::error::ReadlineError::Eof) => {
                break;
            }
            Err(e) => {
                eprintln!("Input error: {e}");
                break;
            }
        }
    }

    Ok(())
}

fn sessions_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".ken").join("sessions")
}
