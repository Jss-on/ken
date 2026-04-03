# Changelog

All notable changes to Ken will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-04-03

### Features

- Initial scaffold: Rust workspace with 4 crates (ken-cli, ken-api, ken-runtime, ken-tools)
- Anthropic Messages API client with streaming SSE and retry/backoff
- Agentic loop: tool dispatch for 13 builtin tools (Python subprocess, LLM-reasoning, Rust-native)
- CLI REPL (rustyline) and one-shot mode with session persistence
- Config hierarchy: env vars > local > project > user defaults
- Pine Script v6 heuristic linter
- Python bridge for compute tools (backtest, fetch_ohlcv, analyze_indicator, risk_analysis, render_chart)

### Bug Fixes

- Path traversal protection on save_strategy/load_strategy
- Propagate HTTP client construction errors instead of panicking
- Surface malformed tool JSON parse errors instead of silently defaulting to null

[Unreleased]: https://github.com/Jss-on/ken/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/Jss-on/ken/releases/tag/v0.1.0
