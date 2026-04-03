use crate::types::*;
use reqwest::blocking::Client;
use std::io::BufRead;
use std::time::Duration;

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const DEFAULT_MODEL: &str = "claude-opus-4-6";
const MAX_RETRIES: u32 = 2;

pub struct AnthropicClient {
    client: Client,
    api_key: String,
    model: String,
}

impl AnthropicClient {
    pub fn new(api_key: impl Into<String>) -> Result<Self, ApiError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|e| ApiError::Network(format!("failed to build HTTP client: {e}")))?;

        Ok(Self {
            client,
            api_key: api_key.into(),
            model: DEFAULT_MODEL.to_string(),
        })
    }

    pub fn with_model(mut self, model: &str) -> Self {
        self.model = model.to_string();
        self
    }

    /// Send a streaming request to the Messages API and collect the full response.
    pub fn send(&self, request: &ApiRequest) -> Result<AssistantResponse, ApiError> {
        let mut last_err = None;

        for attempt in 0..=MAX_RETRIES {
            match self.try_send(request) {
                Ok(response) => return Ok(response),
                Err(ApiError::Auth) => return Err(ApiError::Auth),
                Err(e) => {
                    if attempt < MAX_RETRIES {
                        let backoff = Duration::from_secs(1 << attempt);
                        std::thread::sleep(backoff);
                    }
                    last_err = Some(e);
                }
            }
        }

        Err(last_err.unwrap_or(ApiError::Timeout(MAX_RETRIES)))
    }

    fn try_send(&self, request: &ApiRequest) -> Result<AssistantResponse, ApiError> {
        let resp = self
            .client
            .post(API_URL)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .header("x-api-key", &self.api_key)
            .json(request)
            .send()
            .map_err(|e| ApiError::Network(e.to_string()))?;

        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            let body = resp.text().unwrap_or_default();
            if let Ok(err_json) = serde_json::from_str::<serde_json::Value>(&body)
                && let Some(msg) = err_json["error"]["message"].as_str()
            {
                return Err(ApiError::Network(format!("401 Unauthorized: {msg}")));
            }
            return Err(ApiError::Auth);
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(ApiError::RateLimit);
        }
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(ApiError::Network(format!("HTTP {status}: {body}")));
        }

        if request.stream {
            self.parse_stream(resp)
        } else {
            self.parse_json(resp)
        }
    }

    fn parse_stream(
        &self,
        resp: reqwest::blocking::Response,
    ) -> Result<AssistantResponse, ApiError> {
        let reader = std::io::BufReader::new(resp);
        let mut text = String::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut current_tool_json = String::new();
        let mut current_tool_id = String::new();
        let mut current_tool_name = String::new();
        let mut input_tokens = 0u32;
        let mut output_tokens = 0u32;
        let mut stop_reason = StopReason::EndTurn;

        for line in reader.lines() {
            let line = line.map_err(|e| ApiError::Network(e.to_string()))?;

            if !line.starts_with("data: ") {
                continue;
            }
            let data = &line[6..];
            if data == "[DONE]" {
                break;
            }

            let event: serde_json::Value =
                serde_json::from_str(data).map_err(|e| ApiError::Parse(e.to_string()))?;

            match event["type"].as_str() {
                Some("message_start") => {
                    if let Some(usage) = event["message"]["usage"].as_object() {
                        input_tokens = usage
                            .get("input_tokens")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0) as u32;
                    }
                }
                Some("content_block_start") => {
                    if event["content_block"]["type"] == "tool_use" {
                        current_tool_id = event["content_block"]["id"]
                            .as_str()
                            .unwrap_or("")
                            .to_string();
                        current_tool_name = event["content_block"]["name"]
                            .as_str()
                            .unwrap_or("")
                            .to_string();
                        current_tool_json.clear();
                    }
                }
                Some("content_block_delta") => {
                    if let Some(txt) = event["delta"]["text"].as_str() {
                        text.push_str(txt);
                        eprint!("{txt}");
                    }
                    if let Some(json_chunk) = event["delta"]["partial_json"].as_str() {
                        current_tool_json.push_str(json_chunk);
                    }
                }
                Some("content_block_stop") => {
                    if !current_tool_id.is_empty() {
                        let input: serde_json::Value = serde_json::from_str(&current_tool_json)
                            .map_err(|e| ApiError::Parse(format!("tool input JSON: {e}")))?;
                        tool_calls.push(ToolCall {
                            id: std::mem::take(&mut current_tool_id),
                            name: std::mem::take(&mut current_tool_name),
                            input,
                        });
                        current_tool_json.clear();
                    }
                }
                Some("message_delta") => {
                    if let Some(reason) = event["delta"]["stop_reason"].as_str() {
                        stop_reason = match reason {
                            "tool_use" => StopReason::ToolUse,
                            "max_tokens" => StopReason::MaxTokens,
                            _ => StopReason::EndTurn,
                        };
                    }
                    if let Some(usage) = event["usage"].as_object() {
                        output_tokens = usage
                            .get("output_tokens")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0) as u32;
                    }
                }
                _ => {}
            }
        }
        eprintln!();

        Ok(AssistantResponse {
            text,
            tool_calls,
            stop_reason,
            input_tokens,
            output_tokens,
        })
    }

    fn parse_json(&self, resp: reqwest::blocking::Response) -> Result<AssistantResponse, ApiError> {
        let body: serde_json::Value = resp.json().map_err(|e| ApiError::Parse(e.to_string()))?;

        let mut text = String::new();
        let mut tool_calls = Vec::new();

        if let Some(content) = body["content"].as_array() {
            for block in content {
                match block["type"].as_str() {
                    Some("text") => {
                        text.push_str(block["text"].as_str().unwrap_or(""));
                    }
                    Some("tool_use") => {
                        tool_calls.push(ToolCall {
                            id: block["id"].as_str().unwrap_or("").to_string(),
                            name: block["name"].as_str().unwrap_or("").to_string(),
                            input: block["input"].clone(),
                        });
                    }
                    _ => {}
                }
            }
        }

        let stop_reason = match body["stop_reason"].as_str() {
            Some("tool_use") => StopReason::ToolUse,
            Some("max_tokens") => StopReason::MaxTokens,
            _ => StopReason::EndTurn,
        };

        let input_tokens = body["usage"]["input_tokens"].as_u64().unwrap_or(0) as u32;
        let output_tokens = body["usage"]["output_tokens"].as_u64().unwrap_or(0) as u32;

        Ok(AssistantResponse {
            text,
            tool_calls,
            stop_reason,
            input_tokens,
            output_tokens,
        })
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    /// Make a minimal API call to verify that an API key is accepted.
    pub fn validate_api_key(api_key: &str) -> Result<(), ApiError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| ApiError::Network(format!("failed to build HTTP client: {e}")))?;

        let body = serde_json::json!({
            "model": "claude-haiku-4-5-20251001",
            "max_tokens": 1,
            "messages": [{"role": "user", "content": "hi"}],
        });

        let resp = client
            .post(API_URL)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .header("x-api-key", api_key)
            .json(&body)
            .send()
            .map_err(|e| ApiError::Network(e.to_string()))?;

        let status = resp.status();
        if status.is_success() {
            return Ok(());
        }

        let resp_body = resp.text().unwrap_or_default();
        if let Ok(err_json) = serde_json::from_str::<serde_json::Value>(&resp_body)
            && let Some(msg) = err_json["error"]["message"].as_str()
        {
            return Err(ApiError::Network(format!("HTTP {status}: {msg}")));
        }
        Err(ApiError::Network(format!("HTTP {status}: {resp_body}")))
    }
}
