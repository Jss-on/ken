use anyhow::Result;
use clap::{Parser, Subcommand};
use ken_api::{AnthropicClient, ApiCredential};
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
    /// Set up a long-lived API token (from `claude setup-token`)
    SetupToken,

    /// Authenticate via OAuth (experimental)
    Login {
        /// Use Anthropic Console OAuth instead of Claude Pro/Max
        #[arg(long)]
        console: bool,
    },

    /// Clear stored OAuth credentials
    Logout,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Handle subcommands first
    if let Some(cmd) = cli.command {
        return match cmd {
            Command::SetupToken => run_setup_token(),
            Command::Login { console } => {
                let mode = if console {
                    anthropic_auth::OAuthMode::Console
                } else {
                    anthropic_auth::OAuthMode::Max
                };
                auth::run_login(mode)
            }
            Command::Logout => auth::run_logout(),
        };
    }

    // Default: REPL / one-shot mode
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
    if config.api_key.is_empty() && !has_any_credentials(&config) {
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
    println!("    /apikey         set or update your API key");
    println!("    /setup-token    paste a token from `claude setup-token`");
    println!("    /session        show current session info");
    println!("    /help           show all commands");
    println!("    exit            quit\n");
}

fn print_commands() {
    println!("\n  Commands:");
    println!("    /apikey             set or update your Anthropic API key");
    println!("    /setup-token        paste a token from `claude setup-token`");
    println!("    /login              authenticate via OAuth (experimental)");
    println!("    /login console      authenticate via Anthropic Console OAuth");
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
        Some("/apikey") => {
            if let Err(e) = prompt_api_key() {
                eprintln!("\n  Failed to save API key: {e}\n");
            } else {
                println!("\n  Restart Ken to use the new key.\n");
            }
        }
        Some("/setup-token") => {
            if let Err(e) = run_setup_token() {
                eprintln!("\n  Setup token failed: {e}\n");
            }
        }
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

fn run_setup_token() -> Result<()> {
    println!("\n  Paste a long-lived API token.");
    println!("  You can get one by running: claude setup-token");
    println!("  Or use an API key from: https://console.anthropic.com/settings/keys\n");

    let mut rl = DefaultEditor::new()?;
    let readline = rl.readline("  Token: ");
    match readline {
        Ok(line) => {
            let token = line.trim().to_string();
            if token.is_empty() {
                println!("  No token entered. Skipping.");
                return Ok(());
            }

            // Determine credential type: API keys start with "sk-ant-"
            let credential = if token.starts_with("sk-ant-api") || token.starts_with("sk-") {
                ApiCredential::ApiKey(token.clone())
            } else {
                ApiCredential::BearerToken(token.clone())
            };

            // Validate with a test API call
            print!("  Validating token... ");
            match AnthropicClient::validate_credential(&credential) {
                Ok(()) => {
                    println!("valid!\n");
                }
                Err(e) => {
                    println!("failed.\n");
                    let err_msg = e.to_string();
                    eprintln!("  Warning: {err_msg}\n");

                    // Ask whether to save anyway
                    let save_anyway = rl.readline("  Save anyway? [y/N]: ");
                    match save_anyway {
                        Ok(answer) if answer.trim().eq_ignore_ascii_case("y") => {}
                        _ => {
                            println!("  Token not saved.");
                            return Ok(());
                        }
                    }
                }
            }

            // Save based on credential type
            match &credential {
                ApiCredential::ApiKey(_) => {
                    save_api_key(&token)?;
                    println!("  API key saved to ~/.ken/config.json");
                }
                ApiCredential::BearerToken(_) => {
                    auth::save_setup_token(&token)?;
                    println!("  Token saved to ~/.ken/credentials.json");
                }
            }

            println!("  Restart Ken to use the new credentials.\n");
            Ok(())
        }
        Err(_) => Ok(()),
    }
}

fn run_interactive_login() -> Result<()> {
    println!("  Set up API access:\n");
    println!("    1) Paste a token (from `claude setup-token` or an API key)");
    println!("    2) Enter API key manually");
    println!("    3) Claude Pro/Max OAuth (experimental)");
    println!("    4) Anthropic Console OAuth (experimental)");
    println!("    5) Skip\n");

    let mut rl = DefaultEditor::new()?;
    loop {
        let readline = rl.readline("  Choice [1/2/3/4/5]: ");
        match readline {
            Ok(line) => match line.trim() {
                "1" => return run_setup_token(),
                "2" => return prompt_api_key(),
                "3" => return auth::run_login(anthropic_auth::OAuthMode::Max),
                "4" => return auth::run_login(anthropic_auth::OAuthMode::Console),
                "5" => {
                    println!(
                        "\n  Skipped. Run `ken setup-token` or set KEN_API_KEY later.\n"
                    );
                    return Ok(());
                }
                _ => println!("  Please enter 1, 2, 3, 4, or 5."),
            },
            Err(_) => return Ok(()),
        }
    }
}

fn prompt_api_key() -> Result<()> {
    println!("\n  Get your API key from: https://console.anthropic.com/settings/keys\n");

    let mut rl = DefaultEditor::new()?;
    let readline = rl.readline("  API key: ");
    match readline {
        Ok(line) => {
            let key = line.trim().to_string();
            if key.is_empty() {
                println!("  No key entered. Skipping.");
                return Ok(());
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

    // Load existing config or create new
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

fn has_any_credentials(config: &Config) -> bool {
    // Check Ken credentials file (OAuth or setup token)
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
