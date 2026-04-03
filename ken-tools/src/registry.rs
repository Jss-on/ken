use ken_api::ToolDefinition;
use std::collections::HashMap;

/// Permission level required to execute a tool
#[derive(Debug, Clone, PartialEq)]
pub enum PermissionLevel {
    ReadOnly,
    WorkspaceWrite,
}

/// Static tool specification
#[derive(Debug, Clone)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub permission: PermissionLevel,
}

impl ToolSpec {
    pub fn to_api_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name.clone(),
            description: self.description.clone(),
            input_schema: self.input_schema.clone(),
            cache_control: None,
        }
    }
}

/// Trait for executing tools — unified interface for both LLM-reasoning
/// and compute tools (matching claw-code's ToolExecutor pattern)
pub trait ToolExecutor {
    fn execute(&mut self, tool_name: &str, input: &serde_json::Value) -> Result<String, ToolError>;
}

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("Unknown tool: {0}")]
    UnknownTool(String),
    #[error("Tool execution failed: {0}")]
    ExecutionFailed(String),
    #[error("Subprocess timeout after {0}s")]
    SubprocessTimeout(u64),
    #[error("Subprocess crashed: {0}")]
    SubprocessCrash(String),
    #[error("Tool '{tool}' requires '{dependency}'. Fix: {install_hint}")]
    MissingDependency {
        tool: String,
        dependency: String,
        install_hint: String,
    },
    #[error("Validation error: {0}")]
    Validation(String),
    #[error("Data not found: {0}")]
    DataNotFound(String),
    #[error("Insufficient data: need {needed} candles, have {have}")]
    InsufficientData { needed: usize, have: usize },
}

/// Central tool registry (builtin + plugins)
pub struct ToolRegistry {
    specs: HashMap<String, ToolSpec>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            specs: HashMap::new(),
        };
        registry.register_builtins();
        registry
    }

    fn register_builtins(&mut self) {
        self.register(ToolSpec {
            name: "describe_setup".into(),
            description: "Parse trader's chart description into a structured setup: symbol, timeframe, indicators, price action, support/resistance levels.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "description": { "type": "string", "description": "The trader's natural language description of what they see on the chart" }
                },
                "required": ["description"]
            }),
            permission: PermissionLevel::ReadOnly,
        });

        self.register(ToolSpec {
            name: "analyze_indicator".into(),
            description: "Analyze indicator signals in historical OHLCV data. Returns a list of signals with timestamps, types (bullish/bearish/neutral), and strength.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "indicator": { "type": "string", "description": "Indicator name (e.g., RSI, MACD, custom name)" },
                    "symbol": { "type": "string" },
                    "timeframe": { "type": "string" },
                    "params": { "type": "object", "description": "Indicator parameters (e.g., period, overbought level)" }
                },
                "required": ["indicator", "symbol", "timeframe"]
            }),
            permission: PermissionLevel::ReadOnly,
        });

        self.register(ToolSpec {
            name: "propose_entry_rules".into(),
            description: "Suggest entry conditions based on indicator signals and setup context. Returns structured EntryRule objects.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "setup": { "type": "object", "description": "SetupDescription from describe_setup" },
                    "signals": { "type": "array", "description": "Signal list from analyze_indicator" },
                    "style": { "type": "string", "description": "Trading style preference: aggressive, moderate, conservative" }
                },
                "required": ["setup"]
            }),
            permission: PermissionLevel::ReadOnly,
        });

        self.register(ToolSpec {
            name: "propose_sl_tp".into(),
            description: "Suggest stop loss and take profit placement strategies. Returns 3 variations: fixed, ATR-based, and structure-based.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "entry_rules": { "type": "array", "description": "Entry rules from propose_entry_rules" },
                    "symbol": { "type": "string" },
                    "timeframe": { "type": "string" },
                    "sl_type": { "type": "string", "description": "Preferred SL type: fixed, atr, structure, or all" }
                },
                "required": ["entry_rules", "symbol", "timeframe"]
            }),
            permission: PermissionLevel::ReadOnly,
        });

        self.register(ToolSpec {
            name: "backtest_strategy".into(),
            description: "Run a backtest on a proposed strategy against historical OHLCV data. Returns win rate, risk/reward, max drawdown, Sharpe ratio, and trade list.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "strategy": { "type": "object", "description": "Full strategy definition" },
                    "symbol": { "type": "string" },
                    "timeframe": { "type": "string" },
                    "period": { "type": "string", "description": "Backtest period e.g. '1Y', '6M', '3M'" }
                },
                "required": ["strategy", "symbol", "timeframe"]
            }),
            permission: PermissionLevel::ReadOnly,
        });

        self.register(ToolSpec {
            name: "compare_strategies".into(),
            description: "Compare multiple strategy variations side-by-side with backtest results.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "strategies": { "type": "array", "description": "Array of strategy definitions to compare" },
                    "symbol": { "type": "string" },
                    "timeframe": { "type": "string" },
                    "period": { "type": "string" }
                },
                "required": ["strategies", "symbol", "timeframe"]
            }),
            permission: PermissionLevel::ReadOnly,
        });

        self.register(ToolSpec {
            name: "generate_pine".into(),
            description: "Generate Pine Script v6 code for a finalized strategy. Output is loadable directly into TradingView.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "strategy": { "type": "object", "description": "Full strategy definition" }
                },
                "required": ["strategy"]
            }),
            permission: PermissionLevel::ReadOnly,
        });

        self.register(ToolSpec {
            name: "lint_pine".into(),
            description: "Heuristic lint of Pine Script v6 code: bracket matching, required strategy() call, deprecated function detection.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "code": { "type": "string", "description": "Pine Script code to lint" }
                },
                "required": ["code"]
            }),
            permission: PermissionLevel::ReadOnly,
        });

        self.register(ToolSpec {
            name: "fetch_ohlcv".into(),
            description: "Fetch historical OHLCV data for a symbol and timeframe via ccxt. Caches locally.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "symbol": { "type": "string", "description": "Trading pair e.g. BTC/USDT" },
                    "timeframe": { "type": "string", "description": "Candle timeframe e.g. 1h, 4h, 1d" },
                    "exchange": { "type": "string", "description": "Exchange name e.g. binance (default: binance)" },
                    "limit": { "type": "integer", "description": "Number of candles to fetch (default: 1000)" }
                },
                "required": ["symbol", "timeframe"]
            }),
            permission: PermissionLevel::ReadOnly,
        });

        self.register(ToolSpec {
            name: "risk_analysis".into(),
            description: "Compute risk metrics from backtest results: Sharpe, Sortino, max drawdown, Calmar ratio, win rate.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "backtest_result": { "type": "object", "description": "BacktestResult from backtest_strategy" }
                },
                "required": ["backtest_result"]
            }),
            permission: PermissionLevel::ReadOnly,
        });

        self.register(ToolSpec {
            name: "save_strategy".into(),
            description: "Save a strategy definition to disk as JSON.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "strategy": { "type": "object", "description": "Strategy definition to save" },
                    "name": { "type": "string", "description": "Strategy name for the file" }
                },
                "required": ["strategy"]
            }),
            permission: PermissionLevel::WorkspaceWrite,
        });

        self.register(ToolSpec {
            name: "load_strategy".into(),
            description: "Load a previously saved strategy definition from disk.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Strategy name or ID to load" }
                },
                "required": ["name"]
            }),
            permission: PermissionLevel::ReadOnly,
        });

        self.register(ToolSpec {
            name: "register_indicator".into(),
            description: "Register a custom indicator from source code (Pine Script v6, Python, or any language). The LLM parses the logic; Python scripts can execute directly for backtesting.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Indicator name" },
                    "code": { "type": "string", "description": "Indicator source code" },
                    "language": { "type": "string", "description": "Source language: pinescript, python, or other" }
                },
                "required": ["name", "code", "language"]
            }),
            permission: PermissionLevel::ReadOnly,
        });

        // --- File tools ---
        self.register(ToolSpec {
            name: "read_file".into(),
            description: "Read a file from the local filesystem. Use this to read Pine Script files, strategy files, or any text file the trader references.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path relative to the working directory" }
                },
                "required": ["path"]
            }),
            permission: PermissionLevel::ReadOnly,
        });

        self.register(ToolSpec {
            name: "write_file".into(),
            description: "Write content to a file. Use this to save generated Pine Script code, strategy definitions, or analysis results.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path relative to the working directory" },
                    "content": { "type": "string", "description": "Content to write" }
                },
                "required": ["path", "content"]
            }),
            permission: PermissionLevel::WorkspaceWrite,
        });

        self.register(ToolSpec {
            name: "list_files".into(),
            description: "List files in a directory, optionally filtered by extension (e.g. '.pine', '.json').".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "directory": { "type": "string", "description": "Directory path relative to working directory (default: '.')" },
                    "extension": { "type": "string", "description": "Optional file extension filter e.g. '.pine', '.json'" }
                },
                "required": []
            }),
            permission: PermissionLevel::ReadOnly,
        });

        self.register(ToolSpec {
            name: "render_chart".into(),
            description: "Render an interactive chart with Lightweight Charts showing OHLCV data, indicator signals, and SL/TP levels. Opens in browser.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "symbol": { "type": "string" },
                    "timeframe": { "type": "string" },
                    "signals": { "type": "array", "description": "Optional signals to overlay" },
                    "sl_tp": { "type": "object", "description": "Optional SL/TP levels to show" }
                },
                "required": ["symbol", "timeframe"]
            }),
            permission: PermissionLevel::ReadOnly,
        });
    }

    pub fn register(&mut self, spec: ToolSpec) {
        self.specs.insert(spec.name.clone(), spec);
    }

    pub fn get(&self, name: &str) -> Option<&ToolSpec> {
        self.specs.get(name)
    }

    pub fn all_specs(&self) -> Vec<&ToolSpec> {
        self.specs.values().collect()
    }

    pub fn api_definitions(&self) -> Vec<ToolDefinition> {
        self.specs.values().map(|s| s.to_api_definition()).collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
