use crate::models::claude_session::{ClaudeSession, ClaudeSessionsIndex, JsonlEntry};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

pub struct ClaudeSessionService;

impl ClaudeSessionService {
    fn extract_text_from_json_value(value: &serde_json::Value) -> String {
        match value {
            serde_json::Value::String(text) => text.clone(),
            serde_json::Value::Array(items) => items
                .iter()
                .map(Self::extract_text_from_json_value)
                .filter(|text| !text.trim().is_empty())
                .collect::<Vec<_>>()
                .join("\n"),
            serde_json::Value::Object(map) => {
                if let Some(text) = map.get("text").and_then(|value| value.as_str()) {
                    if !text.trim().is_empty() {
                        return text.to_string();
                    }
                }

                for key in ["content", "title", "label", "summary", "prompt"] {
                    if let Some(value) = map.get(key) {
                        let extracted = Self::extract_text_from_json_value(value);
                        if !extracted.trim().is_empty() {
                            return extracted;
                        }
                    }
                }

                map.values()
                    .map(Self::extract_text_from_json_value)
                    .filter(|text| !text.trim().is_empty())
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            _ => String::new(),
        }
    }

    fn try_extract_structured_session_text(raw: &str) -> String {
        let trimmed = raw.trim();
        if !(trimmed.starts_with('[') || trimmed.starts_with('{')) {
            return raw.to_string();
        }

        serde_json::from_str::<serde_json::Value>(trimmed)
            .ok()
            .or_else(|| {
                let escaped_newlines = trimmed.replace("\r\n", "\\n").replace('\n', "\\n");
                serde_json::from_str::<serde_json::Value>(&escaped_newlines).ok()
            })
            .map(|value| Self::extract_text_from_json_value(&value))
            .filter(|text| !text.trim().is_empty())
            .unwrap_or_else(|| raw.to_string())
    }

    fn strip_html_comments(raw: &str) -> String {
        let mut output = raw.to_string();

        while let Some(start) = output.find("<!--") {
            let Some(end_relative) = output[start + 4..].find("-->") else {
                break;
            };
            let end = start + 4 + end_relative + 3;
            output.replace_range(start..end, " ");
        }

        output
    }

    fn replace_conversation_tag_block(
        raw: &str,
        tag: &str,
        keep_inner_when_terminal: bool,
    ) -> String {
        let open_tag = format!("<{}>", tag);
        let close_tag = format!("</{}>", tag);
        let mut output = raw.to_string();

        while let Some(start) = output.find(&open_tag) {
            let content_start = start + open_tag.len();
            let Some(relative_end) = output[content_start..].find(&close_tag) else {
                break;
            };
            let end = content_start + relative_end;
            let before = output[..start].to_string();
            let inner = output[content_start..end].to_string();
            let after = output[end + close_tag.len()..].to_string();
            let replacement = if after.trim().is_empty() {
                if keep_inner_when_terminal {
                    inner
                } else {
                    String::new()
                }
            } else {
                "\n".to_string()
            };

            output = format!("{}{}{}", before, replacement, after);
        }

        output
    }

    fn sanitize_session_label(raw: &str) -> String {
        let extracted = Self::try_extract_structured_session_text(raw);
        let normalized = Self::strip_html_comments(&extracted)
            .replace("\r\n", "\n")
            .replace('\r', "\n");
        let without_history =
            Self::replace_conversation_tag_block(&normalized, "conversation_history", false);
        let without_summary =
            Self::replace_conversation_tag_block(&without_history, "conversation_summary", true);

        without_summary
            .lines()
            .map(|line| {
                line.trim()
                    .trim_start_matches("Human:")
                    .trim_start_matches("human:")
                    .trim_start_matches("Assistant:")
                    .trim_start_matches("assistant:")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .map(|line| line.trim().to_string())
            .find(|line| {
                !line.is_empty()
                    && !(line.starts_with('<') && line.ends_with('>'))
                    && !line.starts_with(
                        "(This is a summary of earlier conversation turns for context.",
                    )
                    && !line.contains("Tool calls shown here were already executed")
            })
            .unwrap_or_default()
    }

    /// Get the Claude Code projects directory (~/.claude/projects/)
    fn claude_projects_dir() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".claude").join("projects"))
    }

    /// Convert a project path to the Claude Code directory name encoding.
    /// e.g., "/Users/mannix/Project/MeFlow3" -> "-Users-mannix-Project-MeFlow3"
    fn encode_project_path(project_path: &str) -> String {
        project_path.replace('/', "-")
    }

    /// List native Claude Code sessions for a given project path.
    /// Tries sessions-index.json first, falls back to scanning JSONL files.
    pub fn list_sessions_for_project(
        project_path: &str,
        limit: Option<usize>,
    ) -> Result<Vec<ClaudeSession>, String> {
        let normalized_project_path = Self::normalize_non_empty(project_path, "Project path")?;

        let Some(project_dir) = Self::resolve_project_dir(&normalized_project_path)? else {
            return Ok(vec![]);
        };

        // Try sessions-index.json first
        let index_path = project_dir.join("sessions-index.json");
        let mut sessions = if index_path.exists() {
            let indexed_sessions = Self::load_from_index(&index_path, &normalized_project_path)?;
            if indexed_sessions.is_empty() {
                Self::scan_jsonl_files(&project_dir, &normalized_project_path)?
            } else {
                indexed_sessions
            }
        } else {
            Self::scan_jsonl_files(&project_dir, &normalized_project_path)?
        };

        // Sort by parsed modification time descending (most recent first).
        // Raw string comparison is wrong across formats: Claude writes
        // "...Z" while chrono's to_rfc3339 produces "+00:00", and 'Z' sorts
        // above both digits and '+'.
        sessions.sort_by_key(|session| {
            chrono::DateTime::parse_from_rfc3339(&session.modified)
                .map(|dt| dt.timestamp_millis())
                .unwrap_or(i64::MIN)
        });
        sessions.reverse();

        // Apply limit
        if let Some(limit) = limit {
            sessions.truncate(limit);
        }

        Ok(sessions)
    }

    fn resolve_project_dir(project_path: &str) -> Result<Option<PathBuf>, String> {
        let normalized_project_path = Self::normalize_non_empty(project_path, "Project path")?;
        let projects_dir = Self::claude_projects_dir()
            .ok_or_else(|| "Cannot determine home directory".to_string())?;

        let encoded = Self::encode_project_path(&normalized_project_path);
        let exact_match = projects_dir.join(&encoded);
        if exact_match.exists() {
            return Ok(Some(exact_match));
        }

        let entries = fs::read_dir(&projects_dir)
            .map_err(|e| format!("Failed to read Claude projects directory: {}", e))?;

        for entry in entries.flatten() {
            let candidate_dir = entry.path();
            if !candidate_dir.is_dir() {
                continue;
            }

            let index_path = candidate_dir.join("sessions-index.json");
            if !index_path.exists() {
                continue;
            }

            let original_path = fs::read_to_string(&index_path)
                .ok()
                .and_then(|content| serde_json::from_str::<ClaudeSessionsIndex>(&content).ok())
                .and_then(|index| index.original_path)
                .map(|value| value.trim().to_string());

            if original_path.as_deref() == Some(normalized_project_path.as_str()) {
                return Ok(Some(candidate_dir));
            }
        }

        Ok(None)
    }

    /// Load sessions from a sessions-index.json file
    fn load_from_index(
        index_path: &Path,
        project_path: &str,
    ) -> Result<Vec<ClaudeSession>, String> {
        let content = fs::read_to_string(index_path)
            .map_err(|e| format!("Failed to read sessions index: {}", e))?;

        let index: ClaudeSessionsIndex = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse sessions index: {}", e))?;

        let project_dir = index_path
            .parent()
            .ok_or_else(|| "Failed to resolve project directory for sessions index".to_string())?;

        let sessions: Vec<ClaudeSession> = index
            .entries
            .into_iter()
            .filter(|entry| !entry.is_sidechain.unwrap_or(false))
            .filter_map(|entry| {
                let resolved_path = entry
                    .full_path
                    .as_ref()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| project_dir.join(format!("{}.jsonl", entry.session_id)));

                if !resolved_path.exists() {
                    log::debug!(
                        "Skipping stale Claude session index entry {} because file does not exist: {}",
                        entry.session_id,
                        resolved_path.display()
                    );
                    return None;
                }

                Some(ClaudeSession {
                    session_id: entry.session_id,
                    // Always bind to the currently requested project path.
                    // `projectPath` in sessions-index.json can be stale and break PTY cwd/resume.
                    project_path: project_path.to_string(),
                    summary: Self::sanitize_session_label(&entry.summary.unwrap_or_default()),
                    first_prompt: Self::sanitize_session_label(
                        &entry.first_prompt.unwrap_or_default(),
                    ),
                    message_count: entry.message_count.unwrap_or(0),
                    created: entry.created.unwrap_or_default(),
                    modified: entry.modified.unwrap_or_default(),
                    git_branch: entry.git_branch.unwrap_or_default(),
                    is_sidechain: false,
                })
            })
            .collect();

        Ok(sessions)
    }

    /// Scan .jsonl files in the project directory and extract session info
    fn scan_jsonl_files(
        project_dir: &Path,
        project_path: &str,
    ) -> Result<Vec<ClaudeSession>, String> {
        let entries = fs::read_dir(project_dir)
            .map_err(|e| format!("Failed to read project directory: {}", e))?;

        let mut sessions = Vec::new();

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }

            // Extract session ID from filename (UUID.jsonl)
            let file_stem = match path.file_stem().and_then(|s| s.to_str()) {
                Some(s) => s.to_string(),
                None => continue,
            };

            // Skip agent/subagent files
            if file_stem.starts_with("agent-") {
                continue;
            }

            if let Ok(session) = Self::parse_jsonl_file(&path, &file_stem, project_path) {
                sessions.push(session);
            }
        }

        Ok(sessions)
    }

    /// Parse a JSONL file to extract session metadata
    fn parse_jsonl_file(
        path: &Path,
        session_id: &str,
        project_path: &str,
    ) -> Result<ClaudeSession, String> {
        let file = fs::File::open(path).map_err(|e| format!("Failed to open JSONL file: {}", e))?;

        let metadata =
            fs::metadata(path).map_err(|e| format!("Failed to get file metadata: {}", e))?;

        let modified_time = metadata
            .modified()
            .ok()
            .and_then(|t| {
                t.duration_since(std::time::UNIX_EPOCH).ok().map(|d| {
                    chrono::DateTime::from_timestamp(d.as_secs() as i64, d.subsec_nanos())
                        .map(|dt| dt.to_rfc3339())
                        .unwrap_or_default()
                })
            })
            .unwrap_or_default();

        let reader = BufReader::new(file);

        let mut first_prompt = String::new();
        let mut first_timestamp = String::new();
        let mut last_timestamp = modified_time.clone();
        let mut git_branch = String::new();
        let mut message_count: u32 = 0;
        let mut is_sidechain = false;
        // Entries record the real working directory; decoded directory
        // names mangle paths that contain hyphens, so prefer the recorded
        // cwd whenever the file provides one.
        let mut recorded_cwd: Option<String> = None;

        // Only read first N lines to avoid parsing huge files
        for line in reader.lines().take(50) {
            let line = match line {
                Ok(l) => l,
                Err(_) => continue,
            };

            if line.trim().is_empty() {
                continue;
            }

            let entry: JsonlEntry = match serde_json::from_str(&line) {
                Ok(e) => e,
                Err(_) => continue,
            };

            if recorded_cwd.is_none() {
                if let Some(cwd) = entry.cwd.as_deref() {
                    if !cwd.trim().is_empty() {
                        recorded_cwd = Some(cwd.trim().to_string());
                    }
                }
            }

            if let Some(ref t) = entry.entry_type {
                if t == "user" {
                    message_count += 1;

                    if message_count == 1 {
                        // Extract first user prompt
                        if let Some(ref msg) = entry.message {
                            if let Some(ref content) = msg.content {
                                first_prompt = Self::sanitize_session_label(
                                    &Self::extract_text_from_json_value(content),
                                )
                                .chars()
                                .take(200)
                                .collect();
                            }
                        }

                        if let Some(ref ts) = entry.timestamp {
                            first_timestamp = ts.clone();
                        }
                    }

                    if let Some(ref branch) = entry.git_branch {
                        if !branch.is_empty() {
                            git_branch = branch.clone();
                        }
                    }

                    if let Some(sc) = entry.is_sidechain {
                        is_sidechain = sc;
                    }
                } else if t == "assistant" {
                    message_count += 1;
                    if let Some(ref ts) = entry.timestamp {
                        last_timestamp = ts.clone();
                    }
                }
            }
        }

        if message_count == 0 {
            return Err("JSONL file does not contain any conversational messages".to_string());
        }

        Ok(ClaudeSession {
            session_id: session_id.to_string(),
            // Prefer the recorded cwd: decoding the directory name cannot
            // distinguish hyphens from path separators.
            project_path: recorded_cwd.unwrap_or_else(|| project_path.to_string()),
            summary: String::new(), // No summary available without index
            first_prompt,
            message_count,
            created: first_timestamp,
            modified: last_timestamp,
            git_branch,
            is_sidechain,
        })
    }

    /// Delete a Claude Code session JSONL file for the given project path and session ID.
    /// Claude session ids are UUIDs. Validating the shape up front blocks
    /// path-traversal payloads like `../../foo` from ever reaching the
    /// filesystem join below.
    fn validate_session_id(session_id: &str) -> Result<(), String> {
        if uuid::Uuid::parse_str(session_id).is_ok() {
            return Ok(());
        }
        Err(format!("Invalid Claude session id: {}", session_id))
    }

    pub fn delete_claude_session(project_path: &str, session_id: &str) -> Result<(), String> {
        let normalized_project_path = Self::normalize_non_empty(project_path, "Project path")?;
        let normalized_session_id = Self::normalize_non_empty(session_id, "Session id")?;
        Self::validate_session_id(&normalized_session_id)?;

        let project_dir = Self::resolve_project_dir(&normalized_project_path)?
            .ok_or_else(|| "Claude project directory not found for this project".to_string())?;
        let jsonl_path = project_dir.join(format!("{}.jsonl", normalized_session_id));

        if !jsonl_path.exists() {
            return Err(format!("Session file not found: {}", normalized_session_id));
        }

        fs::remove_file(&jsonl_path)
            .map_err(|e| format!("Failed to delete session file: {}", e))?;

        Ok(())
    }

    fn normalize_non_empty(value: &str, field: &str) -> Result<String, String> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(format!("{} cannot be empty", field));
        }

        Ok(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::ClaudeSessionService;
    use crate::services::storage_service::storage_test_env_lock;
    use std::fs;
    use std::path::PathBuf;

    fn unique_temp_home(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "ccsm-claude-session-test-{}-{}",
            name,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|value| value.as_nanos())
                .unwrap_or_default()
        ))
    }

    #[test]
    fn list_sessions_for_project_falls_back_to_index_original_path_match() {
        let _guard = storage_test_env_lock().lock().unwrap();
        let temp_home = unique_temp_home("hyphen-dir");
        let project_dir =
            temp_home.join(".claude/projects/-Users-mannix-Project-PowerOffice-core813");
        fs::create_dir_all(&project_dir).unwrap();

        let session_id = "00000000-0000-4000-8000-000000000001";
        fs::write(project_dir.join(format!("{session_id}.jsonl")), "{}\n").unwrap();
        fs::write(
            project_dir.join("sessions-index.json"),
            r#"{
  "version": 1,
  "originalPath": "/Users/mannix/Project/PowerOffice_core813",
  "entries": [
    {
      "sessionId": "00000000-0000-4000-8000-000000000001",
      "summary": "Recovered session",
      "messageCount": 3,
      "created": "2026-03-08T00:00:00Z",
      "modified": "2026-03-09T00:00:00Z",
      "projectPath": "/Users/mannix/Project/PowerOffice_core813",
      "isSidechain": false
    }
  ]
}"#,
        )
        .unwrap();

        unsafe {
            std::env::set_var("HOME", &temp_home);
        }

        let sessions = ClaudeSessionService::list_sessions_for_project(
            "/Users/mannix/Project/PowerOffice_core813",
            None,
        )
        .unwrap();

        unsafe {
            std::env::remove_var("HOME");
        }

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, session_id);
        assert_eq!(sessions[0].summary, "Recovered session");

        let _ = fs::remove_dir_all(temp_home);
    }

    #[test]
    fn validate_session_id_rejects_path_traversal() {
        assert!(
            super::ClaudeSessionService::validate_session_id(
                "15722961-c140-48f3-afb6-af67ce75026e"
            )
            .is_ok()
        );
        assert!(super::ClaudeSessionService::validate_session_id("../../etc/passwd").is_err());
        assert!(super::ClaudeSessionService::validate_session_id("").is_err());
    }

    #[test]
    fn list_sessions_for_project_skips_snapshot_only_jsonl_files() {
        let _guard = storage_test_env_lock().lock().unwrap();
        let temp_home = unique_temp_home("snapshot-only-jsonl");
        let project_dir = temp_home.join(".claude/projects/-Users-mannix-Project-MeFlow2");
        fs::create_dir_all(&project_dir).unwrap();

        let session_id = "74292510-1512-4af8-b836-82392563dd4d";
        fs::write(
            project_dir.join(format!("{session_id}.jsonl")),
            r#"{"type":"file-history-snapshot","messageId":"8389075c-5862-4848-8378-97d701a686f9","snapshot":{"messageId":"8389075c-5862-4848-8378-97d701a686f9","trackedFileBackups":{},"timestamp":"2026-02-26T07:25:07.268Z"},"isSnapshotUpdate":false}
{"type":"file-history-snapshot","messageId":"a1e16513-87a5-423b-8ed3-234e309045d2","snapshot":{"messageId":"a1e16513-87a5-423b-8ed3-234e309045d2","trackedFileBackups":{"/Users/mannix/.claude/plans/moonlit-mapping-creek.md":{"backupFileName":null,"version":1,"backupTime":"2026-02-26T07:30:00.795Z"}},"timestamp":"2026-02-26T07:25:11.511Z"},"isSnapshotUpdate":false}
"#,
        )
        .unwrap();

        unsafe {
            std::env::set_var("HOME", &temp_home);
        }

        let sessions =
            ClaudeSessionService::list_sessions_for_project("/Users/mannix/Project/MeFlow2", None)
                .unwrap();

        unsafe {
            std::env::remove_var("HOME");
        }

        assert!(sessions.is_empty());

        let _ = fs::remove_dir_all(temp_home);
    }

    #[test]
    fn parse_jsonl_file_extracts_plain_text_from_structured_prompt_blocks() {
        let _guard = storage_test_env_lock().lock().unwrap();
        let temp_home = unique_temp_home("structured-jsonl-title");
        let project_dir = temp_home.join(".claude/projects/-Users-mannix-Documents-Obsidian-Notes");
        fs::create_dir_all(&project_dir).unwrap();

        let session_id = "structured-session";
        let jsonl_path = project_dir.join(format!("{session_id}.jsonl"));
        fs::write(
            &jsonl_path,
            r#"{"type":"user","timestamp":"2026-04-07T01:00:00Z","message":{"role":"user","content":[{"type":"text","text":"<conversation_history>\nEarlier context\n</conversation_history>\n\nReview API auth flow and summarize the next steps"}]}}
{"type":"assistant","timestamp":"2026-04-07T01:01:00Z","message":{"role":"assistant","content":[{"type":"text","text":"Review the middleware and capture the follow-up work."}]}}
"#,
        )
        .unwrap();

        let session = ClaudeSessionService::parse_jsonl_file(
            &jsonl_path,
            session_id,
            "/Users/mannix/Documents/Obsidian/Notes",
        )
        .unwrap();

        assert_eq!(
            session.first_prompt,
            "Review API auth flow and summarize the next steps"
        );

        let _ = fs::remove_dir_all(temp_home);
    }

    #[test]
    fn load_from_index_extracts_plain_text_from_structured_prompt_fields() {
        let _guard = storage_test_env_lock().lock().unwrap();
        let temp_home = unique_temp_home("structured-index-title");
        let project_dir = temp_home.join(".claude/projects/-Users-mannix-Documents-Obsidian-Notes");
        fs::create_dir_all(&project_dir).unwrap();

        let session_id = "structured-index-session";
        fs::write(project_dir.join(format!("{session_id}.jsonl")), "{}\n").unwrap();
        let index_path = project_dir.join("sessions-index.json");
        fs::write(
            &index_path,
            r#"{
  "version": 1,
  "originalPath": "/Users/mannix/Documents/Obsidian/Notes",
      "entries": [
    {
      "sessionId": "structured-index-session",
      "summary": "",
      "firstPrompt": "[{\"type\":\"text\",\"text\":\"<conversation_summary>\nCheck project status\n</conversation_summary>\"}]",
      "messageCount": 2,
      "created": "2026-04-07T01:00:00Z",
      "modified": "2026-04-07T01:05:00Z",
      "projectPath": "/Users/mannix/Documents/Obsidian/Notes",
      "isSidechain": false
    }
  ]
}"#,
        )
        .unwrap();

        let sessions = ClaudeSessionService::load_from_index(
            &index_path,
            "/Users/mannix/Documents/Obsidian/Notes",
        )
        .unwrap();

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].first_prompt, "Check project status");

        let _ = fs::remove_dir_all(temp_home);
    }
}
