use crate::models::Project;
use crate::services::storage_service::StorageService;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

const DEFAULT_PROJECT_COLOR: &str = "#3B82F6";

pub struct ProjectService {
    projects: Mutex<Vec<Project>>,
}

impl ProjectService {
    pub fn new() -> Self {
        let mut projects = StorageService::read::<Vec<Project>>(&StorageService::projects_file())
            .unwrap_or_default();

        if Self::dedupe_projects(&mut projects) {
            if let Err(error) = Self::persist_projects(&projects) {
                log::warn!("Failed to persist deduped projects at startup: {}", error);
            }
        }

        ProjectService {
            projects: Mutex::new(projects),
        }
    }

    pub fn create_project(&self, name: String, path: String) -> Result<Project, String> {
        let mut projects = self.projects.lock().map_err(|e| e.to_string())?;

        let normalized_name = Self::normalize_project_name(&name);
        if normalized_name.is_empty() {
            return Err("Project name cannot be empty".to_string());
        }

        let normalized_path = Self::normalize_project_path(&path);
        if normalized_path.is_empty() {
            return Err("Project path cannot be empty".to_string());
        }

        if let Some(existing) = projects
            .iter_mut()
            .find(|project| Self::has_same_project_path(project, &normalized_path))
        {
            existing.name = normalized_name;
            existing.path = normalized_path;
            existing.updated_at = chrono::Utc::now().to_rfc3339();

            let result = existing.clone();
            Self::persist_projects(&projects)?;
            return Ok(result);
        }

        let project = Project::new(normalized_name, normalized_path);
        projects.push(project.clone());

        Self::persist_projects(&projects)?;
        Ok(project)
    }

    pub fn list_projects(&self) -> Result<Vec<Project>, String> {
        let mut projects = self.projects.lock().map_err(|e| e.to_string())?;

        if Self::dedupe_projects(&mut projects) {
            Self::persist_projects(&projects)?;
        }

        Ok(projects.clone())
    }

    pub fn delete_project(&self, id: &str) -> Result<(), String> {
        let mut projects = self.projects.lock().map_err(|e| e.to_string())?;
        let normalized_id = id.trim();

        if normalized_id.is_empty() {
            return Err("Project id cannot be empty".to_string());
        }

        let before = projects.len();
        projects.retain(|p| p.id != normalized_id);

        if projects.len() == before {
            return Err(format!("Project not found: {}", normalized_id));
        }

        Self::persist_projects(&projects)
    }

    fn persist_projects(projects: &[Project]) -> Result<(), String> {
        StorageService::write(&StorageService::projects_file(), &projects)
            .map_err(|error| format!("Failed to save projects: {}", error))
    }

    fn normalize_project_name(name: &str) -> String {
        name.trim().to_string()
    }

    fn normalize_project_path(path: &str) -> String {
        let mut normalized = path.trim().replace('\\', "/");

        while normalized.len() > 1 && normalized.ends_with('/') {
            if normalized.len() == 3 && normalized.ends_with(":/") {
                break;
            }
            normalized.pop();
        }

        normalized
    }

    fn has_same_project_path(project: &Project, normalized_path: &str) -> bool {
        Self::normalize_project_path(&project.path) == normalized_path
    }

    fn dedupe_projects(projects: &mut Vec<Project>) -> bool {
        let original_len = projects.len();

        let mut merged: Vec<Project> = Vec::with_capacity(projects.len());
        let mut index_by_path: HashMap<String, usize> = HashMap::new();
        let mut changed = false;

        for mut project in projects.drain(..) {
            let normalized_name = Self::normalize_project_name(&project.name);
            let normalized_path = Self::normalize_project_path(&project.path);

            if normalized_name.is_empty() || normalized_path.is_empty() {
                changed = true;
                continue;
            }

            if normalized_name != project.name || normalized_path != project.path {
                project.name = normalized_name.clone();
                project.path = normalized_path.clone();
                changed = true;
            }

            if let Some(&target_idx) = index_by_path.get(&normalized_path) {
                changed = true;
                let target = &mut merged[target_idx];

                if !project.name.trim().is_empty()
                    && (target.name.trim().is_empty() || project.updated_at >= target.updated_at)
                {
                    target.name = project.name.clone();
                }

                if target.description.trim().is_empty() && !project.description.trim().is_empty() {
                    target.description = project.description;
                }

                if target.color == DEFAULT_PROJECT_COLOR && project.color != DEFAULT_PROJECT_COLOR {
                    target.color = project.color;
                }

                target.is_favorited = target.is_favorited || project.is_favorited;

                let mut existing_session_ids: HashSet<String> =
                    target.session_ids.iter().cloned().collect();
                for session_id in project.session_ids {
                    if !existing_session_ids.contains(&session_id) {
                        existing_session_ids.insert(session_id.clone());
                        target.session_ids.push(session_id);
                    }
                }

                if project.created_at < target.created_at {
                    target.created_at = project.created_at;
                }

                if project.updated_at > target.updated_at {
                    target.updated_at = project.updated_at;
                }
            } else {
                index_by_path.insert(normalized_path, merged.len());
                merged.push(project);
            }
        }

        if merged.len() != original_len {
            changed = true;
        }

        *projects = merged;
        changed
    }
}

impl Default for ProjectService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::ProjectService;
    use crate::models::Project;

    #[allow(clippy::too_many_arguments)]
    fn project(
        id: &str,
        name: &str,
        path: &str,
        description: &str,
        color: &str,
        is_favorited: bool,
        session_ids: &[&str],
        created_at: &str,
        updated_at: &str,
    ) -> Project {
        Project {
            id: id.to_string(),
            name: name.to_string(),
            description: description.to_string(),
            path: path.to_string(),
            color: color.to_string(),
            is_favorited,
            session_ids: session_ids
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            created_at: created_at.to_string(),
            updated_at: updated_at.to_string(),
        }
    }

    #[test]
    fn dedupe_projects_merges_same_path_even_when_names_differ() {
        let mut projects = vec![
            project(
                "one",
                "Alpha",
                "/tmp/demo/",
                "",
                "#3B82F6",
                false,
                &["session-a"],
                "2024-01-02T00:00:00Z",
                "2024-01-02T00:00:00Z",
            ),
            project(
                "two",
                "Beta",
                "/tmp/demo",
                "Imported project",
                "#ef4444",
                true,
                &["session-b", "session-a"],
                "2024-01-01T00:00:00Z",
                "2024-01-03T00:00:00Z",
            ),
        ];

        let changed = ProjectService::dedupe_projects(&mut projects);

        assert!(changed);
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].path, "/tmp/demo");
        assert_eq!(projects[0].name, "Beta");
        assert_eq!(projects[0].description, "Imported project");
        assert_eq!(projects[0].color, "#ef4444");
        assert!(projects[0].is_favorited);
        assert_eq!(projects[0].session_ids, vec!["session-a", "session-b"]);
        assert_eq!(projects[0].created_at, "2024-01-01T00:00:00Z");
        assert_eq!(projects[0].updated_at, "2024-01-03T00:00:00Z");
    }
}
