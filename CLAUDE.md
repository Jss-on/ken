# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is Ken

Ken is an AI-powered trading strategy design agent. It runs as a CLI REPL (or one-shot) that lets traders describe chart setups in natural language, then iteratively designs, backtests, and exports strategies as Pine Script v6 for TradingView. Under the hood it's an agentic loop: Rust orchestrator talks to Claude API with tool use, executing tools either as LLM-reasoning (describe_setup, propose_entry_rules, propose_sl_tp, generate_pine, register_indicator) or as Python subprocesses via stdin/stdout JSON (fetch_ohlcv, backtest_strategy, analyze_indicator, risk_analysis, render_chart).

## Build and Run

```bash
cargo build                    # build all crates
cargo run                      # start REPL
cargo run -- -p "describe my setup"  # one-shot mode
cargo run -- --resume <session-id>   # resume session
```

Requires `KEN_API_KEY` env var (Anthropic API key) or `~/.ken/config.json` with `{"api_key": "..."}`.

Python tools need: `pip install -r requirements.txt` (ccxt, pandas, numpy).

## Architecture

Rust workspace with 4 crates:

- **ken-cli** (`ken-cli/src/main.rs`) — Binary entry point. Clap CLI with REPL (rustyline) and one-shot mode. Sessions saved to `~/.ken/sessions/`.
- **ken-api** (`ken-api/src/`) — Anthropic Messages API client. Handles streaming SSE parsing, retry with exponential backoff, tool_use content blocks. Types: `ApiRequest`, `AssistantResponse`, `ToolCall`, `StreamEvent`.
- **ken-runtime** (`ken-runtime/src/`) — Core agentic loop in `runtime.rs::TradingRuntime::run_turn()`. Loops: send API request → parse tool calls → execute tools → add results to conversation → repeat until EndTurn or 20 iterations. Config loads from hierarchy: `~/.ken/config.json` < `./ken.json` < `./ken.local.json` < env vars.
- **ken-tools** (`ken-tools/src/`) — Tool registry (13 builtin tools with JSON Schema definitions), domain types (Candle, Signal, StrategyDefinition, BacktestResult, etc.), and `PythonBridge` subprocess executor.

Python compute tools live in `ken_tools/` (not a Rust crate — a sibling directory). They read JSON from stdin, write JSON to stdout, 30s timeout.

## Tool Dispatch Pattern

Tools split into three categories in `runtime.rs::execute_tool()`:
1. **Python subprocess**: `backtest_strategy`, `fetch_ohlcv`, `analyze_indicator`, `risk_analysis`, `render_chart` — delegated via `PythonBridge`
2. **LLM-reasoning**: `describe_setup`, `propose_entry_rules`, `propose_sl_tp`, `generate_pine`, `register_indicator` — input echoed back; the LLM's own response is the real output
3. **Rust-native**: `lint_pine` (heuristic linter), `save_strategy`/`load_strategy` (filesystem), `compare_strategies` (Python)

## Config Hierarchy

Priority (highest wins): `KEN_API_KEY` env > `./ken.local.json` > `./ken.json` > `~/.ken/config.json` > defaults.

## Git Workflow & Versioning

**Branching model:**
- `main` — stable, tagged releases only. Protected.
- `dev` — integration branch. PRs merge here first.
- `feat/*`, `fix/*`, `refactor/*` — short-lived feature branches off `dev`.
- Release flow: `dev` → PR to `main` → merge → tag → CI builds release artifacts.

**Commit messages:** Conventional Commits required. Format: `<type>[scope][!]: <description>`
Types: `feat`, `fix`, `refactor`, `docs`, `test`, `chore`, `perf`, `ci`, `build`

**Versioning:** Semver via `workspace.package.version` in root `Cargo.toml`. All 4 crates share one version.

**Release process:**
```bash
# Option 1: Manual
./scripts/version-bump.sh 0.2.0    # bump version
# edit CHANGELOG.md
git add Cargo.toml Cargo.lock CHANGELOG.md
git commit -m "chore: release v0.2.0"
git tag v0.2.0
git push origin main --tags

# Option 2: Automated
./scripts/release.sh 0.2.0         # does all of the above
./scripts/release.sh --dry-run 0.3.0  # preview only
```

**Pre-releases:** Use semver pre-release suffixes:
- `0.2.0-alpha.1` — early testing, unstable API
- `0.2.0-beta.1` — feature-complete, may have bugs
- `0.2.0-rc.1` — release candidate, final testing

Tags matching `v*-alpha*`, `v*-beta*`, `v*-rc*` are auto-marked as pre-releases on GitHub.

**Changelog:** `CHANGELOG.md` updated manually or via `git-cliff`. Config in `cliff.toml`.

**CI:** On push/PR to `main`/`dev`: cargo check, clippy, fmt, test, Python tests. On tag push: build release binaries + create GitHub Release.

## gstack

Use the `/browse` skill from gstack for all web browsing. Never use `mcp__claude-in-chrome__*` tools.
