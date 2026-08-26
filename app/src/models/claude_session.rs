use serde::{Deserialize, Serialize};

/// Represents a native Claude Code CLI session
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ClaudeSession {
    pub session_id: String,
    pub project_path: String,
    pub summary: String,
    pub first_prompt: String,
    pub message_count: u32,
    pub created: String,
    pub modified: String,
    pub git_branch: String,
    pub is_sidechain: bool,
}

/// The sessions-index.json format used by Claude Code
#[derive(Deserialize, Debug)]
pub struct ClaudeSessionsIndex {
    #[allow(dead_code)]
    pub version: Option<u32>,
    pub entries: Vec<ClaudeSessionsIndexEntry>,
    #[serde(rename = "originalPath")]
    pub original_path: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct ClaudeSessionsIndexEntry {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "fullPath")]
    #[allow(dead_code)]
    pub full_path: Option<String>,
    #[serde(rename = "fileMtime")]
    #[allow(dead_code)]
    pub file_mtime: Option<u64>,
    #[serde(rename = "firstPrompt")]
    pub first_prompt: Option<String>,
    pub summary: Option<String>,
    #[serde(rename = "messageCount")]
    pub message_count: Option<u32>,
    pub created: Option<String>,
    pub modified: Option<String>,
    #[serde(rename = "gitBranch")]
    pub git_branch: Option<String>,
    #[serde(rename = "projectPath")]
    #[allow(dead_code)]
    pub project_path: Option<String>,
    #[serde(rename = "isSidechain")]
    pub is_sidechain: Option<bool>,
}

/// A single line from a .jsonl session file
#[derive(Deserialize, Debug)]
pub struct JsonlEntry {
    #[serde(rename = "type")]
    pub entry_type: Option<String>,
    #[serde(rename = "sessionId")]
    #[allow(dead_code)]
    pub session_id: Option<String>,
    pub timestamp: Option<String>,
    #[serde(rename = "isSidechain")]
    pub is_sidechain: Option<bool>,
    #[serde(rename = "gitBranch")]
    pub git_branch: Option<String>,
    pub message: Option<JsonlMessage>,
}

#[derive(Deserialize, Debug)]
pub struct JsonlMessage {
    #[allow(dead_code)]
    pub role: Option<String>,
    pub content: Option<serde_json::Value>,
}
