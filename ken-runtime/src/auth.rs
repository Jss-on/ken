use crate::config::Config;
use anthropic_auth::{OAuthClient, OAuthConfig, OAuthMode, TokenSet};
use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, Write};
use std::path::PathBuf;

/// How the credential was resolved — determines the HTTP auth header.
#[derive(Debug, Clone, PartialEq)]
pub enum CredentialSource {
    /// Raw API key from env var or config file
    ApiKey,
    /// OAuth access token from Ken's credentials
    OAuthToken,
    /// OAuth access token borrowed from Claude Code
    ClaudeCodeToken,
}

/// A resolved credential ready for API use.
#[derive(Debug, Clone)]
pub struct ResolvedCredential {
    pub token: String,
    pub source: CredentialSource,
}

/// Stored OAuth credentials (compatible with anthropic-auth TokenSet).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthCredentials {
    pub access_token: String,
    pub refresh_token: String,
    /// Unix timestamp in seconds when the access token expires.
    pub expires_at: u64,
    #[serde(default)]
    pub scopes: Vec<String>,
}

impl From<TokenSet> for OAuthCredentials {
    fn from(ts: TokenSet) -> Self {
        Self {
            access_token: ts.access_token,
            refresh_token: ts.refresh_token,
            expires_at: ts.expires_at,
            scopes: Vec::new(),
        }
    }
}

/// Wrapper for Claude Code's credential file format.
#[derive(Debug, Deserialize)]
struct ClaudeCredentialFile {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: Option<ClaudeOAuthEntry>,
}

#[derive(Debug, Deserialize)]
struct ClaudeOAuthEntry {
    #[serde(rename = "accessToken")]
    access_token: String,
    #[serde(rename = "refreshToken")]
    refresh_token: String,
    /// Claude Code stores this in milliseconds.
    #[serde(rename = "expiresAt")]
    expires_at: u64,
    #[serde(default)]
    scopes: Vec<String>,
}

impl From<ClaudeOAuthEntry> for OAuthCredentials {
    fn from(entry: ClaudeOAuthEntry) -> Self {
        Self {
            access_token: entry.access_token,
            refresh_token: entry.refresh_token,
            // Convert milliseconds to seconds
            expires_at: entry.expires_at / 1000,
            scopes: entry.scopes,
        }
    }
}

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

fn ken_dir() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(|h| PathBuf::from(h).join(".ken"))
}

fn credentials_path() -> Option<PathBuf> {
    ken_dir().map(|d| d.join("credentials.json"))
}

fn claude_credentials_path() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(|h| PathBuf::from(h).join(".claude").join(".credentials.json"))
}

// ---------------------------------------------------------------------------
// Token persistence
// ---------------------------------------------------------------------------

pub fn save_credentials(creds: &OAuthCredentials) -> anyhow::Result<()> {
    let path = credentials_path().context("cannot determine home directory")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(creds)?;
    std::fs::write(&path, json)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }

    Ok(())
}

fn load_ken_credentials() -> Option<OAuthCredentials> {
    let path = credentials_path()?;
    let data = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

fn load_claude_credentials() -> Option<OAuthCredentials> {
    let path = claude_credentials_path()?;
    let data = std::fs::read_to_string(path).ok()?;
    let file: ClaudeCredentialFile = serde_json::from_str(&data).ok()?;
    file.claude_ai_oauth.map(OAuthCredentials::from)
}

pub fn clear_credentials() -> anyhow::Result<()> {
    if let Some(path) = credentials_path()
        && path.exists()
    {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Token refresh
// ---------------------------------------------------------------------------

fn refresh_if_expired(creds: &OAuthCredentials) -> anyhow::Result<OAuthCredentials> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Refresh 60 seconds before expiry
    if creds.expires_at > now + 60 {
        return Ok(creds.clone());
    }

    eprintln!("[ken] OAuth token expired, refreshing...");
    let oauth = OAuthClient::new(OAuthConfig::default())
        .map_err(|e| anyhow::anyhow!("OAuth client init: {e}"))?;

    let new_tokens = oauth
        .refresh_token(&creds.refresh_token)
        .map_err(|e| anyhow::anyhow!("token refresh failed: {e}"))?;

    let mut new_creds = OAuthCredentials::from(new_tokens);
    new_creds.scopes.clone_from(&creds.scopes);

    save_credentials(&new_creds)?;
    eprintln!("[ken] Token refreshed successfully.");
    Ok(new_creds)
}

// ---------------------------------------------------------------------------
// Credential resolution
// ---------------------------------------------------------------------------

/// Resolve the best available credential.
///
/// Priority:
/// 1. `config.api_key` (covers KEN_API_KEY env + all config file layers)
/// 2. Ken OAuth token (`~/.ken/credentials.json`)
/// 3. Claude Code OAuth token (`~/.claude/.credentials.json`, if opted in)
/// 4. Error with guidance
pub fn resolve_credential(config: &Config) -> anyhow::Result<ResolvedCredential> {
    // 1. API key from config hierarchy (env var has highest priority in Config::load)
    if !config.api_key.is_empty() {
        return Ok(ResolvedCredential {
            token: config.api_key.clone(),
            source: CredentialSource::ApiKey,
        });
    }

    // 2. Ken OAuth credentials
    if let Some(creds) = load_ken_credentials() {
        let creds = refresh_if_expired(&creds)?;
        return Ok(ResolvedCredential {
            token: creds.access_token,
            source: CredentialSource::OAuthToken,
        });
    }

    // 3. Claude Code credentials (opt-in)
    if config.use_claude_credentials
        && let Some(creds) = load_claude_credentials()
    {
        let creds = refresh_if_expired(&creds)?;
        return Ok(ResolvedCredential {
            token: creds.access_token,
            source: CredentialSource::ClaudeCodeToken,
        });
    }

    bail!(
        "No credentials found.\n\n\
         Quick setup:\n  \
         ken login              # authenticate with Claude Pro/Max\n  \
         ken login --console    # authenticate via Anthropic Console\n  \
         export KEN_API_KEY=... # or set API key directly\n  \
         # or create ~/.ken/config.json with {{\"api_key\": \"...\"}}"
    )
}

// ---------------------------------------------------------------------------
// Login / Logout
// ---------------------------------------------------------------------------

/// Run the interactive OAuth PKCE login flow.
pub fn run_login(mode: OAuthMode) -> anyhow::Result<()> {
    let mode_name = match mode {
        OAuthMode::Max => "Claude Pro/Max",
        OAuthMode::Console => "Anthropic Console",
    };
    eprintln!("[ken] Starting {mode_name} OAuth login...");

    let oauth = OAuthClient::new(OAuthConfig::default())
        .map_err(|e| anyhow::anyhow!("OAuth client init: {e}"))?;

    let flow = oauth
        .start_flow(mode)
        .map_err(|e| anyhow::anyhow!("OAuth flow start: {e}"))?;

    // Open browser
    eprintln!("\nOpening browser for authorization...");
    eprintln!("If the browser doesn't open, visit this URL:\n");
    eprintln!("  {}\n", flow.authorization_url);

    if let Err(e) = anthropic_auth::open_browser(&flow.authorization_url) {
        eprintln!("Could not open browser: {e}");
        eprintln!("Please open the URL above manually.");
    }

    // Wait for the callback on localhost:1455
    let callback_data = wait_for_callback()?;

    eprintln!("[ken] Exchanging authorization code...");
    let tokens = oauth
        .exchange_code(&callback_data, &flow.state, &flow.verifier)
        .map_err(|e| anyhow::anyhow!("token exchange failed: {e}"))?;

    let creds = OAuthCredentials::from(tokens);
    save_credentials(&creds)?;

    eprintln!("[ken] Login successful! Credentials saved to ~/.ken/credentials.json");

    if mode == OAuthMode::Console {
        eprintln!("[ken] Tip: Console mode tokens can also be used to create API keys.");
    }

    Ok(())
}

/// Clear stored OAuth credentials.
pub fn run_logout() -> anyhow::Result<()> {
    clear_credentials()?;
    eprintln!("[ken] Logged out. Credentials removed.");
    Ok(())
}

// ---------------------------------------------------------------------------
// Local callback server (sync, no tokio needed)
// ---------------------------------------------------------------------------

/// Listen on localhost:1455 for the OAuth redirect and extract the code+state.
fn wait_for_callback() -> anyhow::Result<String> {
    let listener = std::net::TcpListener::bind("127.0.0.1:1455")
        .context("Failed to bind localhost:1455 for OAuth callback")?;

    eprintln!("Waiting for authorization (listening on http://localhost:1455/callback)...");

    let (stream, _) = listener.accept().context("Failed to accept callback")?;
    let mut reader = std::io::BufReader::new(&stream);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;

    // Parse GET /callback?code=...&state=... HTTP/1.1
    let code_with_state = parse_callback_url(&request_line)?;

    // Send a friendly HTML response
    let body = concat!(
        "<!DOCTYPE html><html><body style=\"font-family:system-ui;display:flex;",
        "justify-content:center;align-items:center;height:80vh;\">",
        "<div style=\"text-align:center;\">",
        "<h1>Authorized!</h1>",
        "<p>You can close this tab and return to Ken.</p>",
        "</div></body></html>"
    );

    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len(),
    );

    let mut writer = stream;
    writer.write_all(response.as_bytes())?;
    writer.flush()?;

    Ok(code_with_state)
}

/// Extract the code#state from a callback URL in an HTTP request line.
fn parse_callback_url(request_line: &str) -> anyhow::Result<String> {
    // "GET /callback?code=ABC&state=XYZ HTTP/1.1\r\n"
    let path = request_line
        .split_whitespace()
        .nth(1)
        .context("malformed HTTP request")?;

    let full_url = format!("http://localhost{path}");
    let url = url::Url::parse(&full_url).context("malformed callback URL")?;

    let code = url
        .query_pairs()
        .find(|(k, _)| k == "code")
        .map(|(_, v)| v.to_string())
        .context("missing 'code' parameter in callback")?;

    let state = url
        .query_pairs()
        .find(|(k, _)| k == "state")
        .map(|(_, v)| v.to_string());

    match state {
        Some(s) => Ok(format!("{code}#{s}")),
        None => Ok(code),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_callback_with_code_and_state() {
        let line = "GET /callback?code=abc123&state=xyz789 HTTP/1.1\r\n";
        let result = parse_callback_url(line).unwrap();
        assert_eq!(result, "abc123#xyz789");
    }

    #[test]
    fn parse_callback_with_code_only() {
        let line = "GET /callback?code=abc123 HTTP/1.1\r\n";
        let result = parse_callback_url(line).unwrap();
        assert_eq!(result, "abc123");
    }

    #[test]
    fn parse_callback_missing_code() {
        let line = "GET /callback?state=xyz HTTP/1.1\r\n";
        assert!(parse_callback_url(line).is_err());
    }

    #[test]
    fn credential_from_api_key() {
        let config = Config {
            api_key: "sk-test-key".to_string(),
            ..Config::default()
        };
        let cred = resolve_credential(&config).unwrap();
        assert_eq!(cred.source, CredentialSource::ApiKey);
        assert_eq!(cred.token, "sk-test-key");
    }

    #[test]
    fn credential_empty_api_key_no_oauth_fails() {
        let config = Config::default();
        let result = resolve_credential(&config);
        assert!(result.is_err());
    }

    #[test]
    fn claude_credentials_millis_to_secs() {
        let entry = ClaudeOAuthEntry {
            access_token: "tok".into(),
            refresh_token: "ref".into(),
            expires_at: 1748658860401, // millis
            scopes: vec![],
        };
        let creds = OAuthCredentials::from(entry);
        assert_eq!(creds.expires_at, 1748658860); // seconds
    }
}
