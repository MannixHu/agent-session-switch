//! Registry of sessions created inside this app (`sessions.json`). The
//! sidebar's session lists come exclusively from here; the CLIs' own session
//! history is never listed, only touched for backfill/delete/resume.

use std::sync::Mutex;

use crate::models::app_session::AppSession;
use crate::services::storage_service::StorageService;

pub struct AppSessionStore {
    sessions: Mutex<Vec<AppSession>>,
}

impl AppSessionStore {
    pub fn new() -> Self {
        let sessions =
            StorageService::read::<Vec<AppSession>>(&StorageService::app_sessions_file())
                .unwrap_or_default();
        Self {
            sessions: Mutex::new(sessions),
        }
    }

    pub fn list(&self) -> Result<Vec<AppSession>, String> {
        self.sessions
            .lock()
            .map(|sessions| sessions.clone())
            .map_err(|e| e.to_string())
    }

    /// Sessions of one project, most recently updated first.
    pub fn list_for_project(&self, project_path: &str) -> Result<Vec<AppSession>, String> {
        let mut sessions: Vec<AppSession> = self
            .list()?
            .into_iter()
            .filter(|session| session.project_path == project_path)
            .collect();
        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(sessions)
    }

    pub fn get(&self, id: &str) -> Result<Option<AppSession>, String> {
        Ok(self.list()?.into_iter().find(|session| session.id == id))
    }

    /// Records whose CLI session id has not been backfilled yet.
    pub fn pending(&self) -> Result<Vec<AppSession>, String> {
        Ok(self
            .list()?
            .into_iter()
            .filter(|session| session.agent_session_id.trim().is_empty())
            .collect())
    }

    pub fn insert(&self, session: AppSession) -> Result<(), String> {
        let mut sessions = self.sessions.lock().map_err(|e| e.to_string())?;
        sessions.push(session);
        Self::persist(&sessions)
    }

    pub fn rename(&self, id: &str, title: &str) -> Result<(), String> {
        self.update(id, |session| {
            session.title = title.to_string();
        })
    }

    /// Backfill the CLI session id (and data-file location, where known) of a
    /// pending record.
    pub fn set_agent_session_id(
        &self,
        id: &str,
        agent_session_id: &str,
        agent_session_file: Option<&str>,
    ) -> Result<(), String> {
        self.update(id, |session| {
            session.agent_session_id = agent_session_id.to_string();
            if let Some(file) = agent_session_file {
                session.agent_session_file = Some(file.to_string());
            }
        })
    }

    /// Mark a session as touched (bumps `updated_at`, used for ordering).
    pub fn touch(&self, id: &str) -> Result<(), String> {
        let now = chrono::Utc::now().to_rfc3339();
        self.update(id, |session| {
            session.updated_at = now.clone();
        })
    }

    /// Replace a still-default title with the CLI's own label. User-chosen
    /// names (`title != default_title`) are never overwritten.
    pub fn set_title_if_default(
        &self,
        id: &str,
        default_title: &str,
        title: &str,
    ) -> Result<(), String> {
        let trimmed = title.trim();
        if trimmed.is_empty() {
            return Ok(());
        }
        self.update(id, |session| {
            if session.title == default_title {
                session.title = trimmed.to_string();
            }
        })
    }

    pub fn remove(&self, id: &str) -> Result<Option<AppSession>, String> {
        let mut sessions = self.sessions.lock().map_err(|e| e.to_string())?;
        let index = sessions.iter().position(|session| session.id == id);
        let Some(index) = index else {
            return Ok(None);
        };
        let removed = sessions.remove(index);
        Self::persist(&sessions)?;
        Ok(Some(removed))
    }

    fn update(&self, id: &str, apply: impl FnOnce(&mut AppSession)) -> Result<(), String> {
        let mut sessions = self.sessions.lock().map_err(|e| e.to_string())?;
        let Some(session) = sessions.iter_mut().find(|session| session.id == id) else {
            return Err(format!("Session not found: {}", id));
        };
        apply(session);
        session.updated_at = chrono::Utc::now().to_rfc3339();
        Self::persist(&sessions)
    }

    fn persist(sessions: &[AppSession]) -> Result<(), String> {
        StorageService::write(&StorageService::app_sessions_file(), &sessions)
            .map_err(|error| format!("Failed to save app sessions: {}", error))
    }
}

impl Default for AppSessionStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::AppSessionStore;
    use crate::models::agent::AgentKind;
    use crate::models::app_session::AppSession;
    use crate::services::storage_service::{
        DATA_DIR_OVERRIDE_ENV, StorageService, storage_test_env_lock, unique_test_data_dir,
    };

    fn record(project_path: &str, agent: AgentKind, agent_session_id: &str) -> AppSession {
        AppSession::new(
            project_path.to_string(),
            agent,
            "新会话".to_string(),
            agent_session_id.to_string(),
        )
    }

    #[test]
    fn store_roundtrips_and_orders_by_recent_update() {
        let _guard = storage_test_env_lock().lock().unwrap();
        let dir = unique_test_data_dir("app-session-store");
        unsafe {
            std::env::set_var(DATA_DIR_OVERRIDE_ENV, &dir);
        }

        let store = AppSessionStore::new();
        let first = record("/tmp/demo", AgentKind::Claude, "claude-id-1");
        let second = record("/tmp/demo", AgentKind::Codex, "");
        let other = record("/tmp/elsewhere", AgentKind::OhMyPi, "");
        store.insert(first.clone()).unwrap();
        store.insert(second.clone()).unwrap();
        store.insert(other).unwrap();

        // Fresh instance reads the persisted file.
        let reloaded = AppSessionStore::new();
        let for_project = reloaded.list_for_project("/tmp/demo").unwrap();
        assert_eq!(for_project.len(), 2);
        assert!(for_project.iter().any(|s| s.id == first.id));
        assert!(for_project.iter().any(|s| s.id == second.id));

        // touch reorders: bump the older record.
        std::thread::sleep(std::time::Duration::from_millis(10));
        reloaded.touch(&first.id).unwrap();
        let ordered = reloaded.list_for_project("/tmp/demo").unwrap();
        assert_eq!(ordered[0].id, first.id);

        // Backfill + rename + remove.
        reloaded
            .set_agent_session_id(&second.id, "codex-id-2", Some("/tmp/x.jsonl"))
            .unwrap();
        reloaded.rename(&second.id, "Fix login bug").unwrap();
        assert_eq!(
            reloaded.get(&second.id).unwrap().unwrap().agent_session_id,
            "codex-id-2"
        );
        assert_eq!(
            reloaded.get(&second.id).unwrap().unwrap().title,
            "Fix login bug"
        );
        assert_eq!(
            reloaded
                .get(&second.id)
                .unwrap()
                .unwrap()
                .agent_session_file
                .as_deref(),
            Some("/tmp/x.jsonl")
        );

        // set_title_if_default keeps user names.
        reloaded
            .set_title_if_default(&second.id, "新会话", "auto")
            .unwrap();
        assert_eq!(
            reloaded.get(&second.id).unwrap().unwrap().title,
            "Fix login bug"
        );

        let removed = reloaded.remove(&second.id).unwrap();
        assert_eq!(removed.unwrap().agent_session_id, "codex-id-2");
        assert_eq!(reloaded.list_for_project("/tmp/demo").unwrap().len(), 1);

        unsafe {
            std::env::remove_var(DATA_DIR_OVERRIDE_ENV);
        }
        let _ = std::fs::remove_dir_all(&dir);
        let _ = StorageService::app_data_dir();
    }
}
