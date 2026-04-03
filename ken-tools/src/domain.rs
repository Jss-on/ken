use serde::{Deserialize, Serialize};

// ─── Core trading types ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candle {
    pub timestamp: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OhlcvData {
    pub symbol: String,
    pub timeframe: String,
    pub candles: Vec<Candle>,
}

// ─── Indicator signals ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signal {
    pub timestamp: i64,
    pub indicator_name: String,
    pub signal_type: SignalType,
    /// Normalized 0.0..=1.0 (0=weakest, 1=strongest)
    pub strength: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SignalType {
    Bullish,
    Bearish,
    Neutral,
}

// ─── Strategy rules ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryRule {
    /// Human-readable description of the condition
    pub condition: String,
    /// Which indicators this rule references
    pub indicator_refs: Vec<String>,
    pub logic: EntryLogic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum EntryLogic {
    And { rules: Vec<EntryLogic> },
    Or { rules: Vec<EntryLogic> },
    IndicatorCrossover { fast: String, slow: String },
    Threshold { indicator: String, op: CompareOp, value: f64 },
    PriceAction { pattern: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompareOp {
    Gt,
    Lt,
    Eq,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum StopLossLogic {
    Fixed { pips: f64 },
    Atr { multiplier: f64 },
    Structure { lookback_bars: u32 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TakeProfitLogic {
    Fixed { pips: f64 },
    RiskReward { ratio: f64 },
    Structure { lookback_bars: u32 },
    Trailing { pips: f64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action")]
pub enum TradeAction {
    Buy { sl: f64, tp: f64 },
    Sell { sl: f64, tp: f64 },
    Hold,
}

// ─── Strategy definition ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyDefinition {
    pub id: String,
    pub name: String,
    pub version: u32,
    pub symbol: String,
    pub timeframe: String,
    pub entry_rules: Vec<EntryRule>,
    pub sl_logic: StopLossLogic,
    pub tp_logic: TakeProfitLogic,
    pub indicators: Vec<String>,
    pub created_at: String,
    pub session_id: Option<String>,
}

// ─── Setup description (output of describe_setup) ────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupDescription {
    pub symbol: String,
    pub timeframe: String,
    pub indicators: Vec<String>,
    pub price_action: String,
    pub support_resistance_levels: Vec<f64>,
}

// ─── Backtest results ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestResult {
    pub trades: Vec<Trade>,
    pub win_rate: f64,
    pub avg_rr: f64,
    pub max_drawdown: f64,
    pub sharpe: f64,
    pub total_pnl: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub entry_time: i64,
    pub exit_time: i64,
    pub direction: String,
    pub entry_price: f64,
    pub exit_price: f64,
    pub pnl: f64,
    pub pnl_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonReport {
    pub strategies: Vec<(String, BacktestResult)>,
    pub ranking: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskMetrics {
    pub sharpe_ratio: f64,
    pub sortino_ratio: f64,
    pub max_drawdown: f64,
    pub calmar_ratio: f64,
    pub win_rate: f64,
    pub avg_rr: f64,
}
