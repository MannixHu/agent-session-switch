//! Multi-agent session discovery: Claude Code, Codex CLI, and oh-my-pi (omp).
//!
//! Storage layouts (discovered from each CLI's own data):
//! - Claude: `~/.claude/projects/<encoded-cwd>/<session-id>.jsonl` (+ optional
//!   `sessions-index.json`), handled by [`ClaudeSessionService`].
//! - Codex: `$CODEX_HOME/sessions/YYYY/MM/DD/rollout-<ts>-<uuid>.jsonl`, first
//!   line is a `session_meta` record with `payload.id` and `payload.cwd`.
//! - omp: `~/.omp/agent/sessions/<bucket>/<id>.jsonl` (plus profile roots);
//!   line 1 is a `title` slot, line 2 a pi-shaped `session` header with
//!   `id`/`cwd`/`timestamp`.

use std::fs;
use std::io::BufRead;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use crate::models::agent::{AgentKind, AgentSession};
use crate::services::claude_session_service::ClaudeSessionService;

pub struct AgentSessionService;

/// How many leading lines to inspect for header metadata.
const HEADER_LINE_BUDGET: usize = 40;

impl AgentSessionService {
    /// Discover every resumable session across all supported agents.
    pub fn list_all_sessions() -> Vec<AgentSession> {
        let mut sessions = Vec::new();
        Self::collect_claude(&mut sessions);
        let claude_count = sessions.len();
        Self::collect_codex(&mut sessions);
        let codex_count = sessions.len() - claude_count;
        Self::collect_omp(&mut sessions);
        let omp_count = sessions.len() - claude_count - codex_count;
        log::info!(
            "session discovery: {} claude, {} codex, {} oh-my-pi",
            claude_count,
            codex_count,
            omp_count
        );
        sessions.sort_by_key(|session| std::cmp::Reverse(modified_millis(session)));
        sessions
    }

    /// Delete a session's backing file. Claude sessions go through their own
    /// service (sessions-index maintenance); Codex/omp files are removed
    /// directly after the id is validated.
    pub fn delete_session(session: &AgentSession) -> Result<(), String> {
        match session.agent {
            AgentKind::Claude => ClaudeSessionService::delete_claude_session(
                &session.project_path,
                &session.session_id,
            ),
            AgentKind::Codex | AgentKind::OhMyPi => {
                validate_uuid(&session.session_id)?;
                let path = session.file_path.as_ref().ok_or_else(|| {
                    format!("Session file location unknown: {}", session.session_id)
                })?;
                fs::remove_file(path).map_err(|error| {
                    format!(
                        "Failed to delete session file {}: {}",
                        path.display(),
                        error
                    )
                })
            }
        }
    }

    fn collect_claude(out: &mut Vec<AgentSession>) {
        for (_, project_path) in ClaudeSessionService::list_claude_projects().unwrap_or_default() {
            let sessions = ClaudeSessionService::list_sessions_for_project(&project_path, None)
                .unwrap_or_default();
            for session in sessions {
                log::debug!(
                    "claude session {} in {} (sidechain={})",
                    session.session_id,
                    session.project_path,
                    session.is_sidechain
                );
                if session.is_sidechain {
                    continue;
                }
                let summary = if session.summary.trim().is_empty() {
                    session.first_prompt.trim().to_string()
                } else {
                    session.summary.trim().to_string()
                };
                out.push(AgentSession {
                    agent: AgentKind::Claude,
                    session_id: session.session_id,
                    project_path: session.project_path.clone(),
                    summary,
                    created: session.created,
                    modified: session.modified,
                    file_path: None,
                });
            }
        }
    }

    fn collect_codex(out: &mut Vec<AgentSession>) {
        for root in codex_session_roots() {
            for file in walk_jsonl(&root) {
                if let Some(session) = parse_codex_file(&file) {
                    out.push(session);
                }
            }
        }
    }

    fn collect_omp(out: &mut Vec<AgentSession>) {
        for root in omp_session_roots() {
            for file in walk_jsonl(&root) {
                if let Some(session) = parse_omp_file(&file) {
                    out.push(session);
                }
            }
        }
    }
}

fn modified_millis(session: &AgentSession) -> i64 {
    DateTime::parse_from_rfc3339(&session.modified)
        .map(|dt| dt.with_timezone(&Utc).timestamp_millis())
        .unwrap_or(i64::MIN)
}

fn file_mtime_rfc3339(path: &Path) -> String {
    fs::metadata(path)
        .and_then(|meta| meta.modified())
        .map(DateTime::<Utc>::from)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_default()
}

fn walk_jsonl(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Ok(entries) = fs::read_dir(root) else {
        return files;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files.extend(walk_jsonl(&path));
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("jsonl") {
            files.push(path);
        }
    }
    files.sort_by(|a, b| b.cmp(a));
    files
}

fn open_header_lines(path: &Path) -> Vec<String> {
    let Ok(file) = fs::File::open(path) else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    for line in std::io::BufReader::new(file)
        .lines()
        .take(HEADER_LINE_BUDGET)
    {
        match line {
            Ok(line) => lines.push(line),
            Err(_) => break,
        }
    }
    lines
}

fn parse_json_line(line: &str) -> Option<serde_json::Value> {
    serde_json::from_str::<serde_json::Value>(line.trim()).ok()
}

// ----- Codex -----

fn codex_home() -> PathBuf {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".codex"))
}

fn codex_session_roots() -> Vec<PathBuf> {
    vec![codex_home().join("sessions")]
}

fn parse_codex_file(path: &Path) -> Option<AgentSession> {
    let mut id = String::new();
    let mut cwd = String::new();
    let mut created = String::new();
    let mut first_user_message = String::new();
    for line in open_header_lines(path) {
        let Some(obj) = parse_json_line(&line) else {
            continue;
        };
        match obj.get("type").and_then(|v| v.as_str()) {
            Some("session_meta") => {
                let payload = obj.get("payload").cloned().unwrap_or_default();
                id = payload
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                cwd = payload
                    .get("cwd")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                created = payload
                    .get("timestamp")
                    .and_then(|v| v.as_str())
                    .or_else(|| obj.get("timestamp").and_then(|v| v.as_str()))
                    .unwrap_or_default()
                    .to_string();
            }
            Some("event_msg") if first_user_message.is_empty() => {
                let payload = obj.get("payload").cloned().unwrap_or_default();
                if payload.get("type").and_then(|v| v.as_str()) == Some("user_message") {
                    first_user_message = payload
                        .get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();
                }
            }
            _ => {}
        }
        if !id.is_empty() && !cwd.is_empty() && !first_user_message.is_empty() {
            break;
        }
    }
    if id.is_empty() || cwd.is_empty() {
        return None;
    }
    let summary = sanitize_label(&first_user_message);
    Some(AgentSession {
        agent: AgentKind::Codex,
        session_id: id,
        project_path: cwd,
        summary,
        created,
        modified: file_mtime_rfc3339(path),
        file_path: Some(path.to_path_buf()),
    })
}

// ----- oh-my-pi (omp) -----

fn omp_config_dir() -> PathBuf {
    std::env::var_os("PI_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".omp"))
}

/// Session roots: the default profile plus every named profile, mirroring
/// omp's `getSessionsDir` layout (`~/.omp/agent/sessions`,
/// `~/.omp/profiles/<name>/agent/sessions`).
fn omp_session_roots() -> Vec<PathBuf> {
    let config = omp_config_dir();
    let mut roots = vec![config.join("agent").join("sessions")];
    if let Ok(entries) = fs::read_dir(config.join("profiles")) {
        for entry in entries.flatten() {
            roots.push(entry.path().join("agent").join("sessions"));
        }
    }
    roots
}

fn parse_omp_file(path: &Path) -> Option<AgentSession> {
    let lines = open_header_lines(path);
    let mut title = String::new();
    let mut id = String::new();
    let mut cwd = String::new();
    let mut created = String::new();
    for (index, line) in lines.iter().enumerate() {
        let Some(obj) = parse_json_line(line) else {
            continue;
        };
        match obj.get("type").and_then(|v| v.as_str()) {
            // Line 1 is a fixed-width title slot.
            Some("title") if index == 0 => {
                title = obj
                    .get("title")
                    .and_then(|v| v.as_str())
                    .or_else(|| obj.get("text").and_then(|v| v.as_str()))
                    .unwrap_or_default()
                    .to_string();
            }
            Some("session") => {
                id = obj
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                cwd = obj
                    .get("cwd")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                created = obj
                    .get("timestamp")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                break;
            }
            _ => {}
        }
    }
    if id.is_empty() || cwd.is_empty() {
        return None;
    }
    let summary = title;
    Some(AgentSession {
        agent: AgentKind::OhMyPi,
        session_id: id,
        project_path: cwd,
        summary,
        created,
        modified: file_mtime_rfc3339(path),
        file_path: Some(path.to_path_buf()),
    })
}

fn validate_uuid(session_id: &str) -> Result<(), String> {
    uuid::Uuid::parse_str(session_id)
        .map(|_| ())
        .map_err(|_| format!("Invalid session id: {}", session_id))
}

/// Trim long first-message labels the same way the Claude side does.
fn sanitize_label(raw: &str) -> String {
    let normalized = raw.replace('\n', " ");
    let trimmed = normalized.trim();
    if trimmed.chars().count() <= 80 {
        trimmed.to_string()
    } else {
        let prefix: String = trimmed.chars().take(77).collect();
        format!("{}...", prefix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_codex_rollout_header() {
        let dir = std::env::temp_dir().join(format!(
            "codex-parse-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let nested = dir.join("2026").join("01");
        fs::create_dir_all(&nested).unwrap();
        let file =
            nested.join("rollout-2026-01-01T00-00-00-019bcd48-622f-7472-be4a-a870e7fa8500.jsonl");
        fs::write(
            &file,
            concat!(
                r#"{"timestamp":"2026-01-01T00:00:00.000Z","type":"session_meta","payload":{"id":"019bcd48-622f-7472-be4a-a870e7fa8500","cwd":"/tmp/demo","timestamp":"2026-01-01T00:00:00.000Z"}}"#,
                "\n",
                r#"{"timestamp":"2026-01-01T00:00:01.000Z","type":"event_msg","payload":{"type":"user_message","message":"fix the login bug"}}"#,
                "\n",
            ),
        )
        .unwrap();

        let session = parse_codex_file(&file).expect("should parse");
        assert_eq!(session.agent, AgentKind::Codex);
        assert_eq!(session.session_id, "019bcd48-622f-7472-be4a-a870e7fa8500");
        assert_eq!(session.project_path, "/tmp/demo");
        assert_eq!(session.summary, "fix the login bug");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parses_omp_title_and_session_header() {
        let dir = std::env::temp_dir().join(format!(
            "omp-parse-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("019bcd48-622f-7472-be4a-a870e7fa8511.jsonl");
        fs::write(
            &file,
            concat!(
                r#"{"type":"title","title":"Refactor storage layer"}"#,
                "\n",
                r#"{"type":"session","id":"019bcd48-622f-7472-be4a-a870e7fa8511","cwd":"/tmp/demo","timestamp":"2026-01-01T00:00:00.000Z"}"#,
                "\n",
            ),
        )
        .unwrap();

        let session = parse_omp_file(&file).expect("should parse");
        assert_eq!(session.agent, AgentKind::OhMyPi);
        assert_eq!(session.session_id, "019bcd48-622f-7472-be4a-a870e7fa8511");
        assert_eq!(session.project_path, "/tmp/demo");
        assert_eq!(session.summary, "Refactor storage layer");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn sanitize_label_truncates_long_messages() {
        let long = "x".repeat(120);
        assert_eq!(sanitize_label(&long).chars().count(), 80);
        assert!(sanitize_label(&long).ends_with("..."));
    }
}
