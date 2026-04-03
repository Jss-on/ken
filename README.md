# Ken

Trading strategy design agent — generates Pine Script v6 for TradingView.

## Architecture

Rust workspace with a Python tools package:

| Crate | Purpose |
|---|---|
| `ken-cli` | Interactive CLI |
| `ken-api` | API client and types |
| `ken-runtime` | Session and config management |
| `ken-tools` | Tool registry, domain logic, subprocess runner |

Python tooling (`ken_tools/`) provides backtesting, indicator analysis, risk metrics, and chart rendering.

## Getting Started

```bash
# Rust
cargo build

# Python tools
pip install -r requirements.txt
```

## License

MIT
