use anyhow::Result;
use clap::{Parser, Subcommand};
use ken_api::AnthropicClient;
use ken_runtime::{Config, Session, TradingRuntime};
use rustyline::DefaultEditor;
use std::path::PathBuf;

const BANNER: &str = r#"
  ██╗  ██╗███████╗███╗   ██╗
  ██║ ██╔╝██╔════╝████╗  ██║
  █████╔╝ █████╗  ██╔██╗ ██║
  ██╔═██╗ ██╔══╝  ██║╚██╗██║
  ██║  ██╗███████╗██║ ╚████║
  ╚═╝  ╚═╝╚══════╝╚═╝  ╚═══╝"#;

#[derive(Parser)]
#[command(name = "ken", about = "Trading strategy design agent")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

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

#[derive(Subcommand)]
enum Command {
    /// Set up your Anthropic API key
    SetupToken,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    if let Some(Command::SetupToken) = cli.command {
        return run_setup_token();
    }

    let config = Config::load();
    let sessions_dir = sessions_dir();

    // One-shot mode
    if let Some(ref prompt) = cli.prompt {
        let session = Session::new();
        let mut runtime = TradingRuntime::new(config, session)?;
        let _response = runtime.run_turn(prompt)?;
        // Text already printed during streaming
        println!();
        runtime.session().save(&sessions_dir)?;
        return Ok(());
    }

    // Interactive mode
    print_banner();

    let session = if let Some(ref id) = cli.resume {
        let s = Session::load(&sessions_dir, id)?;
        println!("  Resumed session: {}", s.id);
        s
    } else {
        Session::new()
    };

    if config.api_key.is_empty() {
        println!("\n  No API key found. Let's get you set up.\n");
        run_setup_token()?;
        let config = Config::load();
        if config.api_key.is_empty() {
            println!("  No key configured. Set KEN_API_KEY or run `ken setup-token` later.\n");
            return Ok(());
        }
        let mut runtime = TradingRuntime::new(config, session)?;
        println!("\n  Session: {}\n", runtime.session().id);
        print_help_hint();
        repl(&mut runtime)?;
        save_and_report(&runtime, &sessions_dir, cli.show_tokens)?;
    } else {
        let mut runtime = TradingRuntime::new(config, session)?;
        println!("  Session: {}\n", runtime.session().id);
        print_help_hint();
        repl(&mut runtime)?;
        save_and_report(&runtime, &sessions_dir, cli.show_tokens)?;
    }

    Ok(())
}

fn print_banner() {
    println!("{BANNER}");
    println!(
        "  v{}  —  Trading Strategy Design Agent\n",
        env!("CARGO_PKG_VERSION")
    );
}

fn print_help_hint() {
    println!("  Type a trading setup, or try these commands:");
    println!("    /apikey         set or update your API key");
    println!("    /session        show current session info");
    println!("    /help           show all commands");
    println!("    exit            quit\n");
}

fn print_commands() {
    println!("\n  Commands:");
    println!("    /apikey             set or update your Anthropic API key");
    println!("    /session            show current session ID and token usage");
    println!("    /help               show this help");
    println!("    exit | quit         save session and quit\n");
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

                if input.starts_with('/') {
                    handle_command(input, runtime);
                    continue;
                }

                match runtime.run_turn(input) {
                    Ok(_response) => {
                        // Text is already printed in real-time during SSE streaming
                        // (via eprint! in client.rs), so we just print a newline separator.
                        println!();
                    }
                    Err(e) => {
                        eprintln!("\n  Error: {e}\n");
                    }
                }
            }
            Err(rustyline::error::ReadlineError::Interrupted) => {
                println!("  Use 'exit' to quit.");
            }
            Err(rustyline::error::ReadlineError::Eof) => {
                break;
            }
            Err(e) => {
                eprintln!("  Input error: {e}");
                break;
            }
        }
    }

    Ok(())
}

fn handle_command(input: &str, runtime: &TradingRuntime) {
    let parts: Vec<&str> = input.split_whitespace().collect();
    match parts.first().copied() {
        Some("/help") => print_commands(),
        Some("/apikey") => {
            if let Err(e) = run_setup_token() {
                eprintln!("\n  Failed to save API key: {e}\n");
            } else {
                println!("  Restart Ken to use the new key.\n");
            }
        }
        Some("/session") => {
            let s = runtime.session();
            println!("\n  Session:        {}", s.id);
            println!("  Created:        {}", s.created_at);
            println!("  Messages:       {}", s.messages.len());
            println!("  Input tokens:   {}", s.total_input_tokens);
            println!("  Output tokens:  {}", s.total_output_tokens);
            println!(
                "  Total tokens:   {}",
                s.total_input_tokens + s.total_output_tokens
            );
            if s.total_cache_read_tokens > 0 || s.total_cache_creation_tokens > 0 {
                println!("  Cache read:     {} (10x cheaper)", s.total_cache_read_tokens);
                println!("  Cache write:    {}", s.total_cache_creation_tokens);
            }
            println!();
        }
        _ => {
            println!("\n  Unknown command: {input}");
            println!("  Type /help for available commands.\n");
        }
    }
}

fn run_setup_token() -> Result<()> {
    println!("  Get your API key from: https://console.anthropic.com/settings/keys\n");

    let mut rl = DefaultEditor::new()?;
    let readline = rl.readline("  API key: ");
    match readline {
        Ok(line) => {
            let key = line.trim().to_string();
            if key.is_empty() {
                println!("  No key entered. Skipping.");
                return Ok(());
            }

            // Validate
            print!("  Validating... ");
            match AnthropicClient::validate_api_key(&key) {
                Ok(()) => println!("valid!\n"),
                Err(e) => {
                    println!("failed.\n");
                    eprintln!("  Warning: {e}\n");

                    let save_anyway = rl.readline("  Save anyway? [y/N]: ");
                    match save_anyway {
                        Ok(answer) if answer.trim().eq_ignore_ascii_case("y") => {}
                        _ => {
                            println!("  Key not saved.");
                            return Ok(());
                        }
                    }
                }
            }

            save_api_key(&key)?;
            println!("  API key saved to ~/.ken/config.json");
            Ok(())
        }
        Err(_) => Ok(()),
    }
}

fn save_api_key(key: &str) -> Result<()> {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let ken_dir = PathBuf::from(&home).join(".ken");
    std::fs::create_dir_all(&ken_dir)?;
    let config_path = ken_dir.join("config.json");

    let mut config: serde_json::Value = if let Ok(data) = std::fs::read_to_string(&config_path) {
        serde_json::from_str(&data).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };
    config["api_key"] = serde_json::Value::String(key.to_string());
    std::fs::write(&config_path, serde_json::to_string_pretty(&config)?)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&config_path, std::fs::Permissions::from_mode(0o600))?;
    }

    Ok(())
}

fn save_and_report(
    runtime: &TradingRuntime,
    sessions_dir: &PathBuf,
    show_tokens: bool,
) -> Result<()> {
    runtime.session().save(sessions_dir)?;

    if show_tokens {
        let s = runtime.session();
        let total = s.total_input_tokens + s.total_output_tokens;
        if total > 0 {
            println!(
                "\n  Session stats: {} input + {} output = {} tokens",
                s.total_input_tokens, s.total_output_tokens, total
            );
            if s.total_cache_read_tokens > 0 || s.total_cache_creation_tokens > 0 {
                println!(
                    "  Cache stats:   {} read (10x cheaper) + {} written",
                    s.total_cache_read_tokens, s.total_cache_creation_tokens
                );
            }
        }
    }

    println!("  Session saved. Goodbye!\n");
    Ok(())
}

fn sessions_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".ken").join("sessions")
}
