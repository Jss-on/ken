use crate::auth;
use crate::config::Config;
use crate::session::Session;
use ken_api::AnthropicClient;
use ken_api::types::*;
use ken_tools::{PythonBridge, ToolExecutor, ToolRegistry};
use std::path::{Path, PathBuf};

/// Number of most-recent tool results to keep intact; older ones get cleared.
const MICROCOMPACT_KEEP_RECENT: usize = 4;
/// Placeholder for cleared tool results.
const CLEARED_TOOL_RESULT: &str = "[Old tool result cleared to reduce context size]";
/// Max automatic continuations when response is truncated (StopReason::MaxTokens).
const MAX_CONTINUATIONS: usize = 3;

const SYSTEM_PROMPT: &str = r#"You are Ken, a trading strategy design agent. Your job is to help traders formalize their intuitions into systematic trading strategies.

You help with:
- Analyzing indicator setups described by the trader
- Proposing entry rules, stop loss placement, and take profit targets
- Backtesting strategy variations against historical data
- Generating Pine Script v6 code for TradingView
- Comparing multiple strategy approaches side-by-side

You have access to trading tools. Use them to analyze data, run backtests, and generate code. Always explain your reasoning and present multiple options when proposing SL/TP strategies.

When the trader describes a setup, first use describe_setup to parse it, then analyze_indicator to check signals, then propose_entry_rules and propose_sl_tp to suggest the strategy rules.

Target output is always Pine Script v6 for TradingView.

## Code Output Rules
- When generating Pine Script code, output the COMPLETE code in a single ```pine code block. Never say "I'll generate the code" and defer to a later message — generate it immediately in the same response.
- If code is long, that is fine. Output the full implementation. Do not truncate or summarize sections with comments like "// ... rest of the code".
- After generating Pine Script, offer to save it using write_file.

## Tool Error Handling
- If a tool returns an error (is_error: true), tell the trader exactly what failed and why.
- If a Python tool fails due to missing dependencies, tell the trader to run `pip install -r requirements.txt`.
- Never pretend a tool succeeded when it returned an error.
- If fetch_ohlcv or backtest_strategy fails, acknowledge the failure and continue with what you can do (e.g., generate Pine Script without backtest results).

## File Handling
- When the trader shares Pine Script or indicator code, use write_file to save it for reference (e.g., to `scripts/indicator_name.pine`).
- When generating Pine Script, save it with write_file after outputting it.
- Use read_file to read files the trader references.
- Use list_files to show available saved scripts or strategies."#;

/// Core agentic loop — adapted from claw-code's ConversationRuntime.
///
/// ```text
///   Input (trader prompt)
///     ↓
///   [API Client] → Stream assistant response
///     ↓
///   Parse tool calls
///     ↓
///   [ToolExecutor] → Execute tool
///     ↓
///   Add tool result to conversation
///     ↓
///   Loop until stop (EndTurn) or max iterations
/// ```
pub struct TradingRuntime {
    client: AnthropicClient,
    registry: ToolRegistry,
    session: Session,
    config: Config,
    python_bridge: PythonBridge,
    max_iterations: usize,
}

impl TradingRuntime {
    pub fn new(config: Config, session: Session) -> anyhow::Result<Self> {
        let api_key = auth::resolve_api_key(&config)?;
        let client = AnthropicClient::new(api_key)?.with_model(&config.model);
        let registry = ToolRegistry::new();

        // Locate Python tools directory relative to the binary
        let python_dir = find_python_dir();

        // Check Python dependencies at startup (non-blocking warning)
        check_python_deps();

        Ok(Self {
            client,
            registry,
            session,
            config,
            python_bridge: PythonBridge::new(&python_dir),
            max_iterations: 20,
        })
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    pub fn session_mut(&mut self) -> &mut Session {
        &mut self.session
    }

    /// Run a single conversation turn: trader input → (possibly multiple) LLM responses + tool calls.
    pub fn run_turn(&mut self, input: &str) -> anyhow::Result<String> {
        self.session.add_user_message(input);

        let mut final_text = String::new();
        let mut iterations = 0;
        let mut continuation_count = 0;

        loop {
            if iterations >= self.max_iterations {
                final_text.push_str("\n[Max iterations reached — stopping]");
                break;
            }
            iterations += 1;

            let request = self.build_request();
            let response = self
                .client
                .send(&request)
                .map_err(|e| anyhow::anyhow!("{e}"))?;

            self.session
                .track_tokens(response.input_tokens, response.output_tokens);
            self.session.track_cache_tokens(
                response.cache_read_input_tokens,
                response.cache_creation_input_tokens,
            );

            // Build assistant content blocks for session
            let mut tool_blocks = Vec::new();
            for tc in &response.tool_calls {
                tool_blocks.push(ContentBlock::ToolUse {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                    input: tc.input.clone(),
                    cache_control: None,
                });
            }
            self.session
                .add_assistant_message(&response.text, tool_blocks);

            if !response.text.is_empty() {
                final_text.push_str(&response.text);
            }

            match response.stop_reason {
                StopReason::ToolUse if !response.tool_calls.is_empty() => {
                    // Execute each tool call and add results
                    for tc in &response.tool_calls {
                        let result = self.execute_tool(&tc.name, &tc.input);
                        match result {
                            Ok(output) => {
                                self.session.add_tool_result(&tc.id, &output, false);
                            }
                            Err(e) => {
                                let err_msg = format!("Tool error: {e}");
                                // Surface tool errors to the user immediately
                                eprintln!("\n  [Tool Error] {}: {e}\n", tc.name);
                                self.session.add_tool_result(&tc.id, &err_msg, true);
                            }
                        }
                    }
                    // Continue the loop for the next API call
                }
                StopReason::MaxTokens if continuation_count < MAX_CONTINUATIONS => {
                    // Response was truncated — inject a continuation prompt
                    continuation_count += 1;
                    println!("\n[continuing... {continuation_count}/{MAX_CONTINUATIONS}]");
                    self.session.add_user_message(
                        "[System: Your response was truncated. Continue exactly from where you left off. If you were generating code, continue the code block.]"
                    );
                    // Continue the loop for continuation
                }
                _ => break,
            }
        }

        Ok(final_text)
    }

    fn build_request(&self) -> ApiRequest {
        let mut system_text = SYSTEM_PROMPT.to_string();

        // Add active strategy context if loaded
        if let Some(ref strategy_id) = self.session.active_strategy_id {
            system_text.push_str(&format!("\n\nActive strategy: {strategy_id}"));
        }

        // System prompt as blocks with cache_control — the system prompt is
        // stable across turns, so caching it saves ~90% on those tokens.
        let system = vec![SystemBlock {
            block_type: "text".to_string(),
            text: system_text,
            cache_control: Some(CacheControl::ephemeral()),
        }];

        // Tool definitions with cache_control on the last tool —
        // tools are stable across turns, so they should be cached too.
        let mut tools = self.registry.api_definitions();
        if let Some(last) = tools.last_mut() {
            last.cache_control = Some(CacheControl::ephemeral());
        }

        // Prepare messages with cache breakpoints and microcompact
        let messages = self.prepare_messages();

        ApiRequest {
            model: self.config.model.clone(),
            max_tokens: self.config.max_tokens,
            system,
            messages,
            tools: Some(tools),
            stream: true,
        }
    }

    /// Prepare messages for the API call:
    /// 1. Clear old tool results (microcompact) to reduce context size
    /// 2. Add cache breakpoints on the last user message
    fn prepare_messages(&self) -> Vec<Message> {
        let mut messages = self.session.messages.clone();

        // --- Microcompact: clear old tool results ---
        // Find all ToolResult block indices, keep only the N most recent
        let tool_result_indices: Vec<usize> = messages
            .iter()
            .enumerate()
            .filter(|(_, m)| {
                m.content
                    .iter()
                    .any(|b| matches!(b, ContentBlock::ToolResult { .. }))
            })
            .map(|(i, _)| i)
            .collect();

        if tool_result_indices.len() > MICROCOMPACT_KEEP_RECENT {
            let clear_count = tool_result_indices.len() - MICROCOMPACT_KEEP_RECENT;
            for &idx in &tool_result_indices[..clear_count] {
                for block in &mut messages[idx].content {
                    if let ContentBlock::ToolResult { content, .. } = block {
                        if content.len() > CLEARED_TOOL_RESULT.len() {
                            *content = CLEARED_TOOL_RESULT.to_string();
                        }
                    }
                }
            }
        }

        // --- Cache breakpoints: mark the last user message ---
        // This creates a cache boundary so the prefix (system + tools + history)
        // is cached and only the new user message causes a cache write.
        if let Some(last_user_idx) = messages
            .iter()
            .rposition(|m| matches!(m.role, Role::User))
        {
            if let Some(last_block) = messages[last_user_idx].content.last_mut() {
                match last_block {
                    ContentBlock::Text { cache_control, .. }
                    | ContentBlock::ToolResult { cache_control, .. } => {
                        *cache_control = Some(CacheControl::ephemeral());
                    }
                    ContentBlock::ToolUse { cache_control, .. } => {
                        *cache_control = Some(CacheControl::ephemeral());
                    }
                }
            }
        }

        messages
    }

    fn execute_tool(
        &mut self,
        name: &str,
        input: &serde_json::Value,
    ) -> Result<String, ken_tools::registry::ToolError> {
        // Check tool exists
        if self.registry.get(name).is_none() {
            return Err(ken_tools::registry::ToolError::UnknownTool(
                name.to_string(),
            ));
        }

        // Tools that delegate to Python subprocess
        match name {
            "backtest_strategy" | "fetch_ohlcv" | "analyze_indicator" | "risk_analysis"
            | "render_chart" => {
                let result = self.python_bridge.call(&format!("{name}.py"), input)?;
                Ok(serde_json::to_string_pretty(&result).unwrap_or_default())
            }

            // LLM-reasoning tools — the LLM itself produces the output in its text response.
            // Return a short acknowledgment so the LLM knows the tool was "executed"
            // without echoing the full input (which causes it to repeat itself).
            "describe_setup"
            | "propose_entry_rules"
            | "propose_sl_tp"
            | "generate_pine"
            | "register_indicator" => {
                Ok(format!(r#"{{"status": "ok", "tool": "{name}", "message": "Acknowledged. Continue with your analysis."}}"#))
            }

            "lint_pine" => {
                // Basic heuristic lint — runs locally in Rust
                let code = input["code"].as_str().unwrap_or("");
                let issues = lint_pine_script(code);
                if issues.is_empty() {
                    Ok(r#"{"status": "pass", "issues": []}"#.to_string())
                } else {
                    Ok(serde_json::json!({"status": "fail", "issues": issues}).to_string())
                }
            }

            "save_strategy" => {
                let strategy = &input["strategy"];
                let name_str = input["name"].as_str().unwrap_or("unnamed");
                let name_str = validate_strategy_name(name_str)?;
                std::fs::create_dir_all("strategies")
                    .map_err(|e| ken_tools::registry::ToolError::ExecutionFailed(e.to_string()))?;
                let path = format!("strategies/{name_str}.json");
                std::fs::write(
                    &path,
                    serde_json::to_string_pretty(strategy).unwrap_or_default(),
                )
                .map_err(|e| ken_tools::registry::ToolError::ExecutionFailed(e.to_string()))?;
                Ok(format!(r#"{{"saved": "{path}"}}"#))
            }

            "load_strategy" => {
                let name_str = input["name"].as_str().unwrap_or("");
                let name_str = validate_strategy_name(name_str)?;
                let path = format!("strategies/{name_str}.json");
                let data = std::fs::read_to_string(&path)
                    .map_err(|_| ken_tools::registry::ToolError::DataNotFound(path))?;
                Ok(data)
            }

            "compare_strategies" => {
                // Delegates to Python for multiple backtests
                self.python_bridge
                    .call("compare_strategies.py", input)
                    .map(|v| serde_json::to_string_pretty(&v).unwrap_or_default())
            }

            // --- File tools ---
            "read_file" => {
                let path = input["path"].as_str().unwrap_or("");
                let safe_path = validate_file_path(path)?;
                let content = std::fs::read_to_string(&safe_path).map_err(|e| {
                    ken_tools::registry::ToolError::ExecutionFailed(format!(
                        "Failed to read {}: {e}",
                        safe_path.display()
                    ))
                })?;
                Ok(content)
            }

            "write_file" => {
                let path = input["path"].as_str().unwrap_or("");
                let content = input["content"].as_str().unwrap_or("");
                let safe_path = validate_file_path(path)?;
                if let Some(parent) = safe_path.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| {
                        ken_tools::registry::ToolError::ExecutionFailed(format!(
                            "Failed to create directory: {e}"
                        ))
                    })?;
                }
                let bytes = content.len();
                std::fs::write(&safe_path, content).map_err(|e| {
                    ken_tools::registry::ToolError::ExecutionFailed(format!(
                        "Failed to write {}: {e}",
                        safe_path.display()
                    ))
                })?;
                Ok(format!(
                    r#"{{"written": "{}", "bytes": {bytes}}}"#,
                    safe_path.display()
                ))
            }

            "list_files" => {
                let directory = input["directory"].as_str().unwrap_or(".");
                let extension = input["extension"].as_str();
                let safe_dir = validate_file_path(directory)?;
                let entries = std::fs::read_dir(&safe_dir).map_err(|e| {
                    ken_tools::registry::ToolError::ExecutionFailed(format!(
                        "Failed to list {}: {e}",
                        safe_dir.display()
                    ))
                })?;
                let mut files = Vec::new();
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if let Some(ext) = extension {
                        if !name.ends_with(ext) {
                            continue;
                        }
                    }
                    let meta = entry.metadata();
                    let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
                    let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
                    files.push(serde_json::json!({
                        "name": name,
                        "size": size,
                        "is_dir": is_dir,
                    }));
                }
                Ok(serde_json::to_string_pretty(&files).unwrap_or_default())
            }

            _ => Err(ken_tools::registry::ToolError::UnknownTool(
                name.to_string(),
            )),
        }
    }
}

/// Heuristic Pine Script v6 linter
fn lint_pine_script(code: &str) -> Vec<String> {
    let mut issues = Vec::new();

    // Check for strategy() or indicator() declaration
    if !code.contains("strategy(") && !code.contains("indicator(") {
        issues.push("Missing strategy() or indicator() declaration".to_string());
    }

    // Check bracket matching
    let open_parens = code.matches('(').count();
    let close_parens = code.matches(')').count();
    if open_parens != close_parens {
        issues.push(format!(
            "Mismatched parentheses: {open_parens} open, {close_parens} close"
        ));
    }

    let open_brackets = code.matches('[').count();
    let close_brackets = code.matches(']').count();
    if open_brackets != close_brackets {
        issues.push(format!(
            "Mismatched brackets: {open_brackets} open, {close_brackets} close"
        ));
    }

    // Check for deprecated v4/v5 functions (should be v6)
    let deprecated = [
        (
            "study(",
            "Use indicator() instead of study() in Pine Script v6",
        ),
        (
            "security(",
            "Use request.security() instead of security() in v6",
        ),
        ("input.int(", "Check: input.int() syntax may differ in v6"),
    ];
    for (pattern, msg) in &deprecated {
        if code.contains(pattern) {
            issues.push(msg.to_string());
        }
    }

    issues
}

/// Reject strategy names that could escape the `strategies/` directory.
fn validate_strategy_name(name: &str) -> Result<&str, ken_tools::registry::ToolError> {
    if name.is_empty()
        || name.contains('/')
        || name.contains('\\')
        || name.contains("..")
        || name.contains('\0')
    {
        return Err(ken_tools::registry::ToolError::Validation(
            "Strategy name must not be empty or contain path separators".into(),
        ));
    }
    Ok(name)
}

/// Validate and resolve a file path, rejecting path traversal and out-of-bounds access.
/// Allowed: paths within CWD or ~/.ken/
fn validate_file_path(path: &str) -> Result<PathBuf, ken_tools::registry::ToolError> {
    if path.is_empty() || path.contains('\0') {
        return Err(ken_tools::registry::ToolError::Validation(
            "Path must not be empty or contain null bytes".into(),
        ));
    }

    // Reject obviously dangerous patterns before resolving
    let sensitive = [".env", "credentials", "config.json", "id_rsa", "id_ed25519"];
    let basename = Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    for s in &sensitive {
        if basename == *s {
            return Err(ken_tools::registry::ToolError::Validation(format!(
                "Access to sensitive file '{basename}' is not allowed"
            )));
        }
    }

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let resolved = cwd.join(path);

    // Canonicalize what exists — for new files, canonicalize the parent
    let canonical = if resolved.exists() {
        resolved
            .canonicalize()
            .map_err(|e| ken_tools::registry::ToolError::Validation(e.to_string()))?
    } else if let Some(parent) = resolved.parent() {
        if parent.exists() {
            let canon_parent = parent
                .canonicalize()
                .map_err(|e| ken_tools::registry::ToolError::Validation(e.to_string()))?;
            canon_parent.join(resolved.file_name().unwrap_or_default())
        } else {
            resolved.clone()
        }
    } else {
        resolved.clone()
    };

    // Must be within CWD or ~/.ken/
    let canon_cwd = cwd.canonicalize().unwrap_or(cwd);
    let home_ken = std::env::var("HOME")
        .map(|h| PathBuf::from(h).join(".ken"))
        .ok();

    let in_cwd = canonical.starts_with(&canon_cwd);
    let in_ken_dir = home_ken
        .as_ref()
        .map(|k| canonical.starts_with(k))
        .unwrap_or(false);

    if !in_cwd && !in_ken_dir {
        return Err(ken_tools::registry::ToolError::Validation(format!(
            "Path '{}' is outside the allowed directories",
            path
        )));
    }

    Ok(canonical)
}

/// Check Python dependencies at startup, warn if missing.
fn check_python_deps() {
    let result = std::process::Command::new("python3")
        .args(["-c", "import ccxt; import pandas; import numpy"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    match result {
        Ok(status) if !status.success() => {
            eprintln!(
                "  Warning: Python dependencies not fully installed. \
                 Tools like fetch_ohlcv and backtest_strategy may fail.\n  \
                 Fix: pip install -r requirements.txt\n"
            );
        }
        Err(_) => {
            eprintln!(
                "  Warning: python3 not found. Python-based tools will not work.\n"
            );
        }
        _ => {}
    }
}

fn find_python_dir() -> String {
    // Look for ken_tools directory relative to CWD or binary location
    for candidate in &["ken_tools", "../ken_tools", "./ken_tools"] {
        if std::path::Path::new(candidate).is_dir() {
            return candidate.to_string();
        }
    }
    "ken_tools".to_string()
}

// ToolExecutor trait impl — not currently needed since runtime handles dispatch directly,
// but keeping for future plugin support
impl ToolExecutor for TradingRuntime {
    fn execute(
        &mut self,
        tool_name: &str,
        input: &serde_json::Value,
    ) -> Result<String, ken_tools::registry::ToolError> {
        self.execute_tool(tool_name, input)
    }
}
