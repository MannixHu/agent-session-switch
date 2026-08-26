use serde::{Deserialize, Serialize};

/// The coding-agent CLIs whose sessions this app can discover and resume.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentKind {
    Claude,
    Codex,
    OhMyPi,
}

impl AgentKind {
    pub fn key(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::OhMyPi => "omp",
        }
    }

    #[allow(dead_code)]
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::Codex => "Codex",
            Self::OhMyPi => "oh my pi",
        }
    }

    /// Short sidebar badge.
    pub fn badge(self) -> &'static str {
        match self {
            Self::Claude => "C",
            Self::Codex => "X",
            Self::OhMyPi => "P",
        }
    }
}

/// One resumable session, regardless of which agent wrote it. Sessions are
/// grouped in the UI by their `project_path` (the recorded working
/// directory), so a repo can mix Claude, Codex, and oh-my-pi sessions.
#[derive(Debug, Clone)]
pub struct AgentSession {
    pub agent: AgentKind,
    pub session_id: String,
    pub project_path: String,
    /// Best-effort human label (Claude summary/first prompt, Codex first
    /// user message, omp title slot). May be empty.
    pub summary: String,
    #[allow(dead_code)]
    pub created: String,
    pub modified: String,
}
