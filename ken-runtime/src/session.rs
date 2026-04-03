use ken_api::types::{ContentBlock, Message, Role};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Persistent conversation session stored as JSONL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub created_at: String,
    pub messages: Vec<Message>,
    /// Currently loaded strategy ID (if any)
    pub active_strategy_id: Option<String>,
    /// Token usage tracking
    pub total_input_tokens: u32,
    pub total_output_tokens: u32,
}

impl Session {
    pub fn new() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            messages: Vec::new(),
            active_strategy_id: None,
            total_input_tokens: 0,
            total_output_tokens: 0,
        }
    }

    pub fn add_user_message(&mut self, text: &str) {
        self.messages.push(Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: text.to_string(),
            }],
        });
    }

    pub fn add_assistant_message(&mut self, text: &str, tool_calls: Vec<ContentBlock>) {
        let mut content = Vec::new();
        if !text.is_empty() {
            content.push(ContentBlock::Text {
                text: text.to_string(),
            });
        }
        content.extend(tool_calls);
        self.messages.push(Message {
            role: Role::Assistant,
            content,
        });
    }

    pub fn add_tool_result(&mut self, tool_use_id: &str, result: &str, is_error: bool) {
        self.messages.push(Message {
            role: Role::User,
            content: vec![ContentBlock::ToolResult {
                tool_use_id: tool_use_id.to_string(),
                content: result.to_string(),
                is_error: if is_error { Some(true) } else { None },
            }],
        });
    }

    pub fn track_tokens(&mut self, input: u32, output: u32) {
        self.total_input_tokens += input;
        self.total_output_tokens += output;
    }

    /// Save session to JSONL file
    pub fn save(&self, dir: &PathBuf) -> anyhow::Result<()> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(format!("{}.jsonl", self.id));
        let json = serde_json::to_string(self)?;
        std::fs::write(&path, json)?;
        Ok(())
    }

    /// Load session from JSONL file
    pub fn load(dir: &Path, session_id: &str) -> anyhow::Result<Self> {
        let path = dir.join(format!("{session_id}.jsonl"));
        let data = std::fs::read_to_string(&path)?;
        let session: Session = serde_json::from_str(&data)?;
        Ok(session)
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}
