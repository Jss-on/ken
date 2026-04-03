use serde::{Deserialize, Serialize};

/// Cache control marker for prompt caching.
/// Cache reads are ~10x cheaper than normal input tokens.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheControl {
    #[serde(rename = "type")]
    pub control_type: String,
}

impl CacheControl {
    pub fn ephemeral() -> Self {
        Self {
            control_type: "ephemeral".to_string(),
        }
    }
}

/// A system prompt block (supports cache_control)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

/// Tool definition sent to the API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

/// A message in the conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Vec<ContentBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

/// Content blocks within a message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
}

/// Request to the Messages API
#[derive(Debug, Clone, Serialize)]
pub struct ApiRequest {
    pub model: String,
    pub max_tokens: u32,
    pub system: Vec<SystemBlock>,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDefinition>>,
    pub stream: bool,
}

/// Streamed events from the API
#[derive(Debug, Clone)]
pub enum StreamEvent {
    TextDelta(String),
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    MessageStop,
    Error(String),
}

/// The full assistant response after streaming completes
#[derive(Debug, Clone)]
pub struct AssistantResponse {
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    pub stop_reason: StopReason,
    pub input_tokens: u32,
    pub output_tokens: u32,
    /// Tokens read from cache (10x cheaper than input)
    pub cache_read_input_tokens: u32,
    /// Tokens written to cache (1.25x input cost, amortized over reads)
    pub cache_creation_input_tokens: u32,
}

#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
}

/// API key credential — sent as `x-api-key` header.
#[derive(Debug, Clone)]
pub struct ApiKey(pub String);

/// Errors from the API layer
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("API timeout after {0} retries")]
    Timeout(u32),
    #[error("Rate limited (429) — backing off")]
    RateLimit,
    #[error("Authentication failed — check KEN_API_KEY or ~/.ken/config.json")]
    Auth,
    #[error("Failed to parse API response: {0}")]
    Parse(String),
    #[error("Network error: {0}")]
    Network(String),
}
