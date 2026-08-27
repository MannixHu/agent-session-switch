use serde::{Deserialize, Serialize};

use crate::models::agent::AgentKind;

/// One session created inside this app. The sidebar lists only these records
/// (kept in the app's own `sessions.json`); the CLIs' pre-existing history on
/// disk is never surfaced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppSession {
    /// Registry key (app-generated UUID), independent from CLI session ids.
    pub id: String,
    pub project_path: String,
    pub agent: AgentKind,
    /// Display name. Defaults to a generic label; backfill may replace it
    /// with the CLI's own summary/first prompt until the user renames.
    pub title: String,
    /// CLI-side session id. Claude gets it up front (`--session-id`); Codex
    /// and oh-my-pi records start empty and are backfilled once their session
    /// file lands on disk. Empty means "not yet resumable".
    pub agent_session_id: String,
    /// Where the CLI keeps this session's data file (Codex / oh-my-pi), so
    /// deletion does not need to rescan. Claude resolves by project + id.
    #[serde(default)]
    pub agent_session_file: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl AppSession {
    pub fn new(
        project_path: String,
        agent: AgentKind,
        title: String,
        agent_session_id: String,
    ) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            project_path,
            agent,
            title,
            agent_session_id,
            agent_session_file: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}
