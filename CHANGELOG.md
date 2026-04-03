# Changelog

All notable changes to Ken will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2026-04-03

### Highlights

Complete authentication overhaul — Ken now supports multiple credential sources
with automatic detection and validation, plus a redesigned interactive CLI.

### Features

- **auth**: `ken setup-token` CLI subcommand — paste a token from `claude setup-token`
  or an API key; auto-detects credential type (API key vs bearer token) and validates
  against the Anthropic API before saving
- **auth**: `/setup-token` and `/apikey` REPL slash commands for in-session credential setup
- **auth**: OAuth 2.0 PKCE login/logout (`ken login`, `ken logout`) with support for
  both Max (Claude Pro/Max subscription) and Console (Anthropic Console) modes
- **auth**: Automatic token refresh on expiry with 60-second buffer
- **auth**: Read Claude Code credentials (`~/.claude/.credentials.json`) via
  `use_claude_credentials` config option
- **auth**: Credential resolution priority chain: `KEN_API_KEY` env > config file >
  Ken OAuth token > setup token > Claude Code token (opt-in)
- **auth**: 401 retry with automatic credential refresh in the agentic loop
- **auth**: Headless server support — paste OAuth authorization codes via stdin
  when browser redirect is unavailable
- **api**: `ApiCredential` enum supporting both `x-api-key` and `Authorization: Bearer`
  headers; `validate_credential()` for pre-save token verification
- **api**: Surface detailed error messages from 401 API responses instead of generic
  "authentication failed"
- **cli**: Restructured from flat flags to clap subcommands (`setup-token`, `login`,
  `logout`) while preserving bare `ken` as the REPL default
- **cli**: ASCII art banner, interactive first-run setup wizard with 5 auth options,
  and comprehensive `/help` command
- **cli**: Version string uses `CARGO_PKG_VERSION` for single-source-of-truth versioning

### Changed

- **cli**: First-run auth menu now leads with token/API key entry (reliable) and marks
  OAuth as experimental
- **auth**: "No credentials" error message updated with `ken setup-token` guidance

### Known Limitations

- Anthropic's Messages API currently rejects OAuth bearer tokens with "OAuth
  authentication is currently not supported." The OAuth infrastructure remains in
  place for future use. Use API keys or `setup-token` for now.

## [0.1.0] - 2026-04-03

### Highlights

Initial release — a working agentic trading strategy designer with CLI REPL,
streaming API, and 13 builtin tools.

### Features

- Rust workspace with 4 crates: `ken-cli`, `ken-api`, `ken-runtime`, `ken-tools`
- Anthropic Messages API client with streaming SSE parsing, retry with exponential
  backoff, and tool_use content block handling
- Agentic loop (`TradingRuntime::run_turn`) — send API request, parse tool calls,
  execute tools, feed results back, repeat until EndTurn or 20 iterations
- 13 builtin tools split across 3 execution modes:
  - **Python subprocess**: `backtest_strategy`, `fetch_ohlcv`, `analyze_indicator`,
    `risk_analysis`, `render_chart`
  - **LLM-reasoning**: `describe_setup`, `propose_entry_rules`, `propose_sl_tp`,
    `generate_pine`, `register_indicator`
  - **Rust-native**: `lint_pine`, `save_strategy`, `load_strategy`, `compare_strategies`
- CLI REPL via rustyline with readline history, plus one-shot mode (`-p "prompt"`)
- Session persistence to `~/.ken/sessions/` with resume support (`--resume <id>`)
- Config hierarchy: `~/.ken/config.json` < `./ken.json` < `./ken.local.json` < `KEN_API_KEY` env
- Pine Script v6 heuristic linter
- Python bridge subprocess executor with JSON stdin/stdout protocol and 30s timeout
- Domain types: `Candle`, `Signal`, `StrategyDefinition`, `BacktestResult`, etc.

### Bug Fixes

- Path traversal protection on `save_strategy` / `load_strategy` — reject `..` in
  strategy names
- Propagate HTTP client construction errors via `Result` instead of panicking
- Surface malformed tool JSON parse errors instead of silently defaulting to null

[Unreleased]: https://github.com/Jss-on/ken/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/Jss-on/ken/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/Jss-on/ken/releases/tag/v0.1.0
