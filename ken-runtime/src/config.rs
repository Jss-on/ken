use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Configuration hierarchy: ~/.ken/config.json < ./ken.json < ./ken.local.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default)]
    pub default_exchange: Option<String>,
    #[serde(default)]
    pub default_symbol: Option<String>,
    #[serde(default)]
    pub default_timeframe: Option<String>,
}

fn default_model() -> String {
    "claude-opus-4-6".to_string()
}

fn default_max_tokens() -> u32 {
    8192
}

impl Default for Config {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: default_model(),
            max_tokens: default_max_tokens(),
            default_exchange: Some("binance".to_string()),
            default_symbol: None,
            default_timeframe: None,
        }
    }
}

impl Config {
    /// Load config from the hierarchy, merging layers.
    /// Priority: env vars > local > project > user
    pub fn load() -> Self {
        let mut config = Config::default();

        // Layer 1: User config (~/.ken/config.json)
        if let Some(user_path) = Self::user_config_path() {
            if let Ok(data) = std::fs::read_to_string(&user_path) {
                if let Ok(user_config) = serde_json::from_str::<Config>(&data) {
                    config.merge(user_config);
                }
            }
        }

        // Layer 2: Project config (./ken.json)
        if let Ok(data) = std::fs::read_to_string("ken.json") {
            if let Ok(proj_config) = serde_json::from_str::<Config>(&data) {
                config.merge(proj_config);
            }
        }

        // Layer 3: Local config (./ken.local.json)
        if let Ok(data) = std::fs::read_to_string("ken.local.json") {
            if let Ok(local_config) = serde_json::from_str::<Config>(&data) {
                config.merge(local_config);
            }
        }

        // Layer 4: Environment variables (highest priority)
        if let Ok(key) = std::env::var("KEN_API_KEY") {
            config.api_key = key;
        }

        config
    }

    fn merge(&mut self, other: Config) {
        if !other.api_key.is_empty() {
            self.api_key = other.api_key;
        }
        if other.model != default_model() {
            self.model = other.model;
        }
        if other.max_tokens != default_max_tokens() {
            self.max_tokens = other.max_tokens;
        }
        if other.default_exchange.is_some() {
            self.default_exchange = other.default_exchange;
        }
        if other.default_symbol.is_some() {
            self.default_symbol = other.default_symbol;
        }
        if other.default_timeframe.is_some() {
            self.default_timeframe = other.default_timeframe;
        }
    }

    fn user_config_path() -> Option<PathBuf> {
        dirs_or_home().map(|h| h.join(".ken").join("config.json"))
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.api_key.is_empty() {
            return Err(
                "API key not configured. Set KEN_API_KEY env var or add to ~/.ken/config.json"
                    .to_string(),
            );
        }
        Ok(())
    }
}

fn dirs_or_home() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}
