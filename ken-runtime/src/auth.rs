use crate::config::Config;
use anyhow::bail;

/// Resolve the API key from the config hierarchy.
///
/// Priority: `KEN_API_KEY` env > `./ken.local.json` > `./ken.json` > `~/.ken/config.json`
/// (all already merged into `config.api_key` by `Config::load`)
pub fn resolve_api_key(config: &Config) -> anyhow::Result<String> {
    if !config.api_key.is_empty() {
        return Ok(config.api_key.clone());
    }

    bail!(
        "No API key found.\n\n\
         Quick setup:\n  \
         ken setup-token                          # paste your API key interactively\n  \
         export KEN_API_KEY=sk-ant-api03-...      # or set via environment variable\n  \
         # or create ~/.ken/config.json with {{\"api_key\": \"sk-ant-api03-...\"}}\n\n\
         Get your key at: https://console.anthropic.com/settings/keys"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_with_api_key() {
        let config = Config {
            api_key: "sk-test-key".to_string(),
            ..Config::default()
        };
        let key = resolve_api_key(&config).unwrap();
        assert_eq!(key, "sk-test-key");
    }

    #[test]
    fn resolve_empty_api_key_fails() {
        let config = Config::default();
        let result = resolve_api_key(&config);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("No API key found"));
    }
}
