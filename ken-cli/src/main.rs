use anyhow::Result;
use clap::Parser;
use ken_runtime::{Config, Session, TradingRuntime, auth};
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
    let sessions_dir = sessions_dir();

    // One-shot mode: skip banner, resolve creds, run, done
    if let Some(ref prompt) = cli.prompt {
        let session = Session::new();
        let mut runtime = TradingRuntime::new(config, session)?;
        let response = runtime.run_turn(prompt)?;
        println!("{response}");
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

    // Check if we have credentials before entering the REPL
    if config.api_key.is_empty() && !has_oauth_credentials(&config) {
        println!("\n  No credentials found. Let's get you set up.\n");
        run_interactive_login()?;
        // Reload config in case login created something
        let config = Config::load();
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
    println!("    /login          authenticate with Anthropic OAuth");
    println!("    /logout         clear stored credentials");
    println!("    /session        show current session info");
    println!("    /help           show all commands");
    println!("    exit            quit\n");
}

fn print_commands() {
    println!("\n  Commands:");
    println!("    /login              authenticate via OAuth (Claude Pro/Max)");
    println!("    /login console      authenticate via Anthropic Console");
    println!("    /logout             clear stored OAuth credentials");
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

                // Handle slash commands
                if input.starts_with('/') {
                    handle_command(input, runtime);
                    continue;
                }

                match runtime.run_turn(input) {
                    Ok(response) => {
                        if !response.is_empty() {
                            println!("\n{response}\n");
                        }
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
        Some("/login") => {
            let mode = if parts.get(1) == Some(&"console") {
                anthropic_auth::OAuthMode::Console
            } else {
                anthropic_auth::OAuthMode::Max
            };
            if let Err(e) = auth::run_login(mode) {
                eprintln!("\n  Login failed: {e}\n");
            }
        }
        Some("/logout") => {
            if let Err(e) = auth::run_logout() {
                eprintln!("\n  Logout failed: {e}\n");
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
                "  Total tokens:   {}\n",
                s.total_input_tokens + s.total_output_tokens
            );
        }
        _ => {
            println!("\n  Unknown command: {input}");
            println!("  Type /help for available commands.\n");
        }
    }
}

fn run_interactive_login() -> Result<()> {
    println!("  Choose authentication method:\n");
    println!("    1) Claude Pro/Max (OAuth — recommended)");
    println!("    2) Anthropic Console (OAuth)");
    println!("    3) Skip (set KEN_API_KEY later)\n");

    let mut rl = DefaultEditor::new()?;
    loop {
        let readline = rl.readline("  Choice [1/2/3]: ");
        match readline {
            Ok(line) => match line.trim() {
                "1" => return auth::run_login(anthropic_auth::OAuthMode::Max),
                "2" => return auth::run_login(anthropic_auth::OAuthMode::Console),
                "3" => {
                    println!("\n  Skipped. Set KEN_API_KEY or run /login inside the REPL later.\n");
                    return Ok(());
                }
                _ => println!("  Please enter 1, 2, or 3."),
            },
            Err(_) => return Ok(()),
        }
    }
}

fn has_oauth_credentials(config: &Config) -> bool {
    // Check Ken credentials
    let home = std::env::var("HOME").ok();
    if let Some(ref h) = home {
        let ken_creds = PathBuf::from(h).join(".ken").join("credentials.json");
        if ken_creds.exists() {
            return true;
        }
    }
    // Check Claude Code credentials if opted in
    if config.use_claude_credentials
        && let Some(ref h) = home
    {
        let claude_creds = PathBuf::from(h).join(".claude").join(".credentials.json");
        if claude_creds.exists() {
            return true;
        }
    }
    false
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
        }
    }

    println!("  Session saved. Goodbye!\n");
    Ok(())
}

fn sessions_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".ken").join("sessions")
}
