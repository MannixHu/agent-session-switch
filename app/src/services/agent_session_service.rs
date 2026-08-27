//! Multi-agent session discovery: Claude Code, Codex CLI, and oh-my-pi (omp).
//!
//! The sidebar lists only sessions created inside this app (see
//! [`crate::services::app_session_store`]). Discovery here exists solely to
//! backfill CLI session ids for those app-created sessions, and to delete
//! their backing files. Storage layouts:
//! - Claude: `~/.claude/projects/<encoded-cwd>/<session-id>.jsonl` (+ optional
//!   `sessions-index.json`), handled by [`ClaudeSessionService`].
//! - Codex: `$CODEX_HOME/sessions/YYYY/MM/DD/rollout-<ts>-<uuid>.jsonl`, first
//!   line is a `session_meta` record with `payload.id` and `payload.cwd`.
//! - omp: `~/.omp/agent/sessions/<bucket>/<id>.jsonl` (plus profile roots);
//!   line 1 is a `title` slot, line 2 a pi-shaped `session` header with
//!   `id`/`cwd`/`timestamp`.

use std::collections::HashSet;
use std::fs;
use std::io::BufRead;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use crate::models::agent::{AgentKind, AgentSession};
use crate::services::claude_session_service::ClaudeSessionService;

pub struct AgentSessionService;

/// How many leading lines to inspect for header metadata.
const HEADER_LINE_BUDGET: usize = 40;

/// One pending app-session record waiting for its CLI session id.
#[derive(Debug, Clone)]
pub struct PendingBackfill {
    /// AppSession registry id.
    pub record_id: String,
    pub agent: AgentKind,
    pub created_at: String,
}

/// A backfill decision produced by [`AgentSessionService::match_backfills`].
#[derive(Debug, Clone, PartialEq)]
pub struct Backfill {
    pub record_id: String,
    pub agent_session_id: String,
    /// Best-effort label from the CLI's own metadata.
    pub label: String,
    /// Data-file location for Codex / oh-my-pi sessions, used by delete.
    pub file_path: Option<PathBuf>,
}

impl AgentSessionService {
    /// Discover sessions the CLIs recorded for one project path, across all
    /// supported agents, most recently modified first. Used only to backfill
    /// ids for sessions started from this app — the sidebar never lists CLI
    /// history directly.
    pub fn list_sessions_for_project(project_path: &str) -> Vec<AgentSession> {
        let mut sessions = Vec::new();
        Self::collect_claude(project_path, &mut sessions);
        Self::collect_codex(project_path, &mut sessions);
        Self::collect_omp(project_path, &mut sessions);
        Self::log_discovery(project_path, &sessions);
        sessions.sort_by_key(|session| std::cmp::Reverse(modified_millis(session)));
        sessions
    }

    /// Pair pending app-session records with sessions the CLIs just recorded
    /// for the same project. A discovered session matches a pending record
    /// when the agent kind is equal, its file landed at/after the record was
    /// created, and its id is not already taken (by the registry or by an
    /// earlier pairing in this run). Records and candidates are matched
    /// oldest-first so concurrent sessions stay in creation order.
    pub fn match_backfills(
        pending: &[PendingBackfill],
        discovered: &[AgentSession],
        known_ids: &HashSet<String>,
    ) -> Vec<Backfill> {
        let mut taken: HashSet<String> = known_ids.clone();
        let mut ordered_pending: Vec<&PendingBackfill> = pending.iter().collect();
        ordered_pending.sort_by_key(|record| rfc3339_millis(&record.created_at));

        let mut backfills = Vec::new();
        for record in ordered_pending {
            let created = rfc3339_millis(&record.created_at);
            let candidate = discovered
                .iter()
                .filter(|session| session.agent == record.agent)
                .filter(|session| !taken.contains(&session.session_id))
                .filter(|session| {
                    let modified = modified_millis(session);
                    modified != i64::MIN && modified >= created
                })
                .min_by_key(|session| modified_millis(session));
            if let Some(session) = candidate {
                taken.insert(session.session_id.clone());
                backfills.push(Backfill {
                    record_id: record.record_id.clone(),
                    agent_session_id: session.session_id.clone(),
                    label: session.summary.trim().to_string(),
                    file_path: session.file_path.clone(),
                });
            }
        }
        backfills
    }

    fn log_discovery(project_path: &str, sessions: &[AgentSession]) {
        let claude = sessions
            .iter()
            .filter(|s| s.agent == AgentKind::Claude)
            .count();
        let codex = sessions
            .iter()
            .filter(|s| s.agent == AgentKind::Codex)
            .count();
        let omp = sessions.len() - claude - codex;
        log::debug!(
            "session discovery for {}: {} claude, {} codex, {} oh-my-pi",
            project_path,
            claude,
            codex,
            omp
        );
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

    fn collect_claude(project_path: &str, out: &mut Vec<AgentSession>) {
        for session in
            ClaudeSessionService::list_sessions_for_project(project_path, None).unwrap_or_default()
        {
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
                project_path: session.project_path,
                summary,
                created: session.created,
                modified: session.modified,
                file_path: None,
            });
        }
    }

    fn collect_codex(project_path: &str, out: &mut Vec<AgentSession>) {
        for root in codex_session_roots() {
            for file in walk_jsonl(&root) {
                if let Some(session) = parse_codex_file(&file) {
                    if same_path(&session.project_path, project_path) {
                        out.push(session);
                    }
                }
            }
        }
    }

    fn collect_omp(project_path: &str, out: &mut Vec<AgentSession>) {
        for root in omp_session_roots() {
            for file in walk_jsonl(&root) {
                if let Some(session) = parse_omp_file(&file) {
                    if same_path(&session.project_path, project_path) {
                        out.push(session);
                    }
                }
            }
        }
    }
}

/// Loose path equality that ignores trailing slashes so a recorded cwd of
/// "/tmp/demo/" still matches a stored project path of "/tmp/demo".
fn same_path(left: &str, right: &str) -> bool {
    normalize_path(left) == normalize_path(right)
}

fn normalize_path(path: &str) -> String {
    path.trim_end_matches('/').to_string()
}

fn modified_millis(session: &AgentSession) -> i64 {
    rfc3339_millis(&session.modified)
}

fn rfc3339_millis(value: &str) -> i64 {
    DateTime::parse_from_rfc3339(value)
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
    use crate::services::storage_service::storage_test_env_lock;

    fn discovered(agent: AgentKind, session_id: &str, modified: &str) -> AgentSession {
        AgentSession {
            agent,
            session_id: session_id.to_string(),
            project_path: "/tmp/demo".to_string(),
            summary: format!("label-{session_id}"),
            created: String::new(),
            modified: modified.to_string(),
            file_path: None,
        }
    }

    fn pending_record(record_id: &str, agent: AgentKind, created_at: &str) -> PendingBackfill {
        PendingBackfill {
            record_id: record_id.to_string(),
            agent,
            created_at: created_at.to_string(),
        }
    }

    #[test]
    fn match_backfills_pairs_pending_records_with_new_files_in_order() {
        let pending_records = vec![
            pending_record("rec-older", AgentKind::Codex, "2026-08-01T10:00:00+00:00"),
            pending_record("rec-newer", AgentKind::Codex, "2026-08-01T11:00:00+00:00"),
            pending_record("rec-claude", AgentKind::Claude, "2026-08-01T09:00:00+00:00"),
        ];
        let discovered_sessions = vec![
            // Wrong agent for the codex records.
            discovered(
                AgentKind::Claude,
                "claude-known",
                "2026-08-01T10:30:00+00:00",
            ),
            discovered(AgentKind::Codex, "codex-early", "2026-08-01T10:20:00+00:00"),
            discovered(AgentKind::Codex, "codex-mid", "2026-08-01T11:10:00+00:00"),
            discovered(AgentKind::Codex, "codex-late", "2026-08-01T11:40:00+00:00"),
        ];
        let mut known = HashSet::new();
        known.insert("claude-known".to_string());

        let backfills =
            AgentSessionService::match_backfills(&pending_records, &discovered_sessions, &known);

        // Oldest pending codex record takes the earliest file at/after its
        // creation; the newer record takes the next one; the claude record
        // finds nothing because its only candidate id is already known.
        assert_eq!(
            backfills,
            vec![
                Backfill {
                    record_id: "rec-older".to_string(),
                    agent_session_id: "codex-early".to_string(),
                    label: "label-codex-early".to_string(),
                    file_path: None,
                },
                Backfill {
                    record_id: "rec-newer".to_string(),
                    agent_session_id: "codex-mid".to_string(),
                    label: "label-codex-mid".to_string(),
                    file_path: None,
                },
            ]
        );
    }

    #[test]
    fn match_backfills_skips_unparseable_timestamps() {
        let pending_records = vec![pending_record(
            "rec",
            AgentKind::Codex,
            "2026-08-01T10:00:00+00:00",
        )];
        let discovered_sessions = vec![discovered(AgentKind::Codex, "bad-time", "not-a-date")];

        let backfills = AgentSessionService::match_backfills(
            &pending_records,
            &discovered_sessions,
            &HashSet::new(),
        );

        assert!(backfills.is_empty());
    }

    #[test]
    fn list_sessions_for_project_scopes_codex_and_omp_by_cwd() {
        let _guard = storage_test_env_lock().lock().unwrap();
        let dir = std::env::temp_dir().join(format!(
            "agent-session-per-project-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        // Codex rollout for /tmp/wanted-project ...
        let codex_sessions = dir
            .join("codex-home")
            .join("sessions")
            .join("2026")
            .join("08");
        fs::create_dir_all(&codex_sessions).unwrap();
        fs::write(
            codex_sessions.join("rollout-2026-08-01T00-00-00-c0dec48-622f-7472-be4a-a870e7fa0001.jsonl"),
            concat!(
                r#"{"timestamp":"2026-08-01T00:00:00.000Z","type":"session_meta","payload":{"id":"019bcd48-622f-7472-be4a-a870e7fa0001","cwd":"/tmp/wanted-project","timestamp":"2026-08-01T00:00:00.000Z"}}"#,
                "\n",
            ),
        )
        .unwrap();
        // ... and one for a different cwd that must not leak in.
        fs::write(
            codex_sessions.join("rollout-2026-08-01T00-00-00-c0dec48-622f-7472-be4a-a870e7fa0002.jsonl"),
            concat!(
                r#"{"timestamp":"2026-08-01T00:00:00.000Z","type":"session_meta","payload":{"id":"019bcd48-622f-7472-be4a-a870e7fa0002","cwd":"/tmp/other-project","timestamp":"2026-08-01T00:00:00.000Z"}}"#,
                "\n",
            ),
        )
        .unwrap();

        // omp session whose recorded cwd has a trailing slash; same_path must
        // still match it to the stored project path.
        let omp_sessions = dir.join("omp-home").join("agent").join("sessions");
        fs::create_dir_all(&omp_sessions).unwrap();
        fs::write(
            omp_sessions.join("019bcd48-622f-7472-be4a-a870e7fa0003.jsonl"),
            concat!(
                r#"{"type":"title","title":"Trailing slash"}"#,
                "\n",
                r#"{"type":"session","id":"019bcd48-622f-7472-be4a-a870e7fa0003","cwd":"/tmp/wanted-project/","timestamp":"2026-08-01T00:00:00.000Z"}"#,
                "\n",
            ),
        )
        .unwrap();

        unsafe {
            std::env::set_var("CODEX_HOME", dir.join("codex-home"));
            std::env::set_var("PI_CONFIG_DIR", dir.join("omp-home"));
        }

        let sessions = AgentSessionService::list_sessions_for_project("/tmp/wanted-project");

        unsafe {
            std::env::remove_var("CODEX_HOME");
            std::env::remove_var("PI_CONFIG_DIR");
        }

        let ids: Vec<&str> = sessions.iter().map(|s| s.session_id.as_str()).collect();
        assert!(ids.contains(&"019bcd48-622f-7472-be4a-a870e7fa0001"));
        assert!(ids.contains(&"019bcd48-622f-7472-be4a-a870e7fa0003"));
        assert!(!ids.contains(&"019bcd48-622f-7472-be4a-a870e7fa0002"));

        let _ = fs::remove_dir_all(&dir);
    }

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
