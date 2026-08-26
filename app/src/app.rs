//! The main dashboard view: sidebar (projects + Claude sessions), terminal
//! tabs, status bar, settings/update dialogs.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use gpui::prelude::*;
use gpui::{App, AppContext, Context, Focusable, MouseButton, ScrollWheelEvent, Window, div, px};

use crate::i18n::{AppLanguage, t, tf};
use crate::models::app_settings::{AppSettings, LastOpenedSession};
use crate::models::claude_session::ClaudeSession;
use crate::services::claude_session_service::ClaudeSessionService;
use crate::services::project_service::ProjectService;
use crate::services::settings_service::SettingsService;
use crate::services::storage_service::StorageService;
use crate::services::update_service::{UpdateCheckResult, UpdateService};
use crate::terminal::{TerminalLaunch, TerminalTab, TerminalView};
use crate::theme::{self, Theme, ThemePreference};
use crate::ui::{TextField, label_button};

const STATUS_AUTO_CLEAR: Duration = Duration::from_millis(3200);
const SIDEBAR_MIN_WIDTH: f32 = 200.0;
const SIDEBAR_MAX_WIDTH: f32 = 520.0;

struct SidebarProject {
    path: String,
    sessions: Vec<ClaudeSession>,
    sessions_loaded: bool,
}

impl SidebarProject {
    fn display_name(&self) -> String {
        PathBuf::from(&self.path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&self.path)
            .to_string()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum UpdatePhase {
    Idle,
    Checking,
    Available,
    Downloading,
    UpToDate,
    Completed,
    Failed,
}

struct UpdateUiState {
    phase: UpdatePhase,
    result: Option<UpdateCheckResult>,
    error: Option<String>,
    busy: bool,
}

#[derive(Clone)]
enum ConfirmKind {
    DeleteSession {
        project_path: String,
        session_id: String,
    },
    DeleteProject {
        path: String,
    },
}

struct ConfirmState {
    kind: ConfirmKind,
    message: String,
}

enum Overlay {
    None,
    Settings,
    Update,
    Confirm(ConfirmState),
    Rename { session_id: String },
    ProjectMenu { path: String },
}

pub struct Dashboard {
    settings_service: SettingsService,
    project_service: ProjectService,
    settings: AppSettings,
    projects: Vec<SidebarProject>,
    expanded: std::collections::HashSet<String>,
    tabs: Vec<TerminalTab>,
    active_tab: usize,
    sidebar_width: f32,
    resizing_sidebar: bool,
    status: Option<(String, Instant)>,
    overlay: Overlay,
    update: UpdateUiState,
    focus_handle: gpui::FocusHandle,
    settings_pane: gpui::Entity<SettingsPane>,
    rename_field: Option<gpui::Entity<TextField>>,
    last_window_save: Option<Instant>,
    _subscriptions: Vec<gpui::Subscription>,
}

impl Dashboard {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let settings = SettingsService::new().get_settings().unwrap_or_default();
        let expanded: std::collections::HashSet<String> = settings
            .ui
            .project_tree
            .expanded_projects
            .iter()
            .filter(|(_, open)| **open)
            .map(|(path, _)| path.clone())
            .collect();
        let sidebar_width = settings.ui.layout.sidebar_width as f32;
        let restore = settings.sessions.restore_last_opened_session;
        let last_opened = settings.sessions.last_opened.clone();

        let settings_pane = cx.new(|cx| SettingsPane::new(settings.clone(), cx));

        let mut subscriptions = Vec::new();
        subscriptions.push(cx.observe_window_appearance(window, |_, _, cx| {
            cx.notify();
        }));
        subscriptions.push(cx.observe_window_bounds(window, |this, _window, cx| {
            let now = Instant::now();
            let should_save = this
                .last_window_save
                .map(|last| now.duration_since(last) > Duration::from_secs(2))
                .unwrap_or(true);
            if should_save {
                this.last_window_save = Some(now);
            }
            cx.notify();
        }));

        let focus_handle = cx.focus_handle();

        let mut dashboard = Self {
            settings_service: SettingsService::new(),
            project_service: ProjectService::new(),
            settings,
            projects: Vec::new(),
            expanded,
            tabs: Vec::new(),
            active_tab: 0,
            sidebar_width: sidebar_width.clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH),
            resizing_sidebar: false,
            status: None,
            overlay: Overlay::None,
            update: UpdateUiState {
                phase: UpdatePhase::Idle,
                result: None,
                error: None,
                busy: false,
            },
            focus_handle,
            settings_pane,
            rename_field: None,
            last_window_save: None,
            _subscriptions: subscriptions,
        };

        dashboard.refresh_projects(cx);
        if restore {
            if let Some(LastOpenedSession {
                project_path,
                session_id,
            }) = last_opened
            {
                dashboard.open_claude_session(&project_path, &session_id, window, cx);
            }
        }
        dashboard
    }

    fn lang(&self) -> AppLanguage {
        AppLanguage::from_str(&self.settings.appearance.language)
    }

    fn t(&self, key: &str) -> &'static str {
        t(self.lang(), key)
    }

    fn tf(&self, key: &str, params: &[(&str, &str)]) -> String {
        tf(self.lang(), key, params)
    }

    fn theme(&self, cx: &App) -> Theme {
        let appearance = cx.window_appearance();
        let preference = ThemePreference::from_str(&self.settings.appearance.theme_preference);
        let palette = theme::resolve_palette(
            &self.settings.appearance.theme_preset,
            preference,
            &self.settings.appearance.theme_palettes,
            appearance,
        );
        Theme::from_palette(&palette, preference.resolves_to_dark(appearance))
    }

    fn show_status(&mut self, message: String, cx: &mut Context<Self>) {
        self.status = Some((message, Instant::now()));
        cx.notify();
        let hide_at = Instant::now() + STATUS_AUTO_CLEAR;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(STATUS_AUTO_CLEAR).await;
            let _ = hide_at;
            this.update(cx, |this, cx| {
                if let Some((_, shown_at)) = &this.status {
                    if shown_at.elapsed() >= STATUS_AUTO_CLEAR - Duration::from_millis(50) {
                        this.status = None;
                        cx.notify();
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    fn persist_settings(&mut self, cx: &mut Context<Self>) {
        if let Err(error) = self.settings_service.set_settings(self.settings.clone()) {
            log::warn!("Failed to save settings: {}", error);
            self.show_status(self.t("status_settings_save_failed").to_string(), cx);
        }
    }

    // ----- data -----

    fn refresh_projects(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let loaded = cx
                .background_executor()
                .spawn(async move { ClaudeSessionService::list_claude_projects() })
                .await;
            this.update(cx, |this, cx| match loaded {
                Ok(entries) => {
                    let order = this.settings.ui.project_tree.project_order.clone();
                    let mut projects: Vec<SidebarProject> = entries
                        .into_iter()
                        .map(|(_dir_name, path)| SidebarProject {
                            path,
                            sessions: Vec::new(),
                            sessions_loaded: false,
                        })
                        .collect();
                    // User-added projects that do not have Claude sessions yet
                    // still need to appear in the sidebar.
                    for stored in this.project_service.list_projects().unwrap_or_default() {
                        if !projects.iter().any(|project| project.path == stored.path) {
                            projects.push(SidebarProject {
                                path: stored.path,
                                sessions: Vec::new(),
                                sessions_loaded: false,
                            });
                        }
                    }

                    projects.sort_by_key(|project| {
                        let index = order.iter().position(|path| path == &project.path);
                        (index.is_none(), index.unwrap_or(usize::MAX))
                    });

                    this.projects = projects;
                    for path in this.expanded.iter().cloned().collect::<Vec<_>>() {
                        this.load_sessions_for(&path, cx);
                    }
                    cx.notify();
                }
                Err(error) => {
                    log::warn!("Failed to list claude projects: {}", error);
                    this.show_status(this.t("status_load_failed").to_string(), cx);
                }
            })
            .ok();
        })
        .detach();
    }

    fn load_sessions_for(&mut self, project_path: &str, cx: &mut Context<Self>) {
        let path = project_path.to_string();
        let path_for_update = path.clone();
        cx.spawn(async move |this, cx| {
            let loaded = cx
                .background_executor()
                .spawn(async move { ClaudeSessionService::list_sessions_for_project(&path, None) })
                .await;
            this.update(cx, |this, cx| {
                if let Ok(sessions) = loaded {
                    if let Some(project) = this
                        .projects
                        .iter_mut()
                        .find(|project| project.path == path_for_update)
                    {
                        project.sessions = sessions;
                        project.sessions_loaded = true;
                    }
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn visible_sessions<'a>(&'a self, project: &'a SidebarProject) -> Vec<&'a ClaudeSession> {
        project
            .sessions
            .iter()
            .filter(|session| !session.is_sidechain)
            .filter(|session| {
                !self
                    .settings
                    .sessions
                    .hidden
                    .get(&session.session_id)
                    .copied()
                    .unwrap_or(false)
            })
            .collect()
    }

    fn session_label(&self, session: &ClaudeSession) -> String {
        self.settings
            .sessions
            .aliases
            .get(&session.session_id)
            .cloned()
            .filter(|alias| !alias.trim().is_empty())
            .unwrap_or_else(|| {
                let summary = session.summary.trim();
                if summary.is_empty() {
                    session.first_prompt.trim().to_string()
                } else {
                    summary.to_string()
                }
            })
    }

    fn claude_args(&self) -> Vec<String> {
        if !self.settings.claude.use_custom_startup_args {
            return Vec::new();
        }
        self.settings
            .claude
            .custom_startup_args
            .split_whitespace()
            .map(str::trim)
            .filter(|arg| !arg.is_empty())
            .map(str::to_string)
            .collect()
    }

    // ----- terminal actions -----

    fn tab_for_session(&self, project_path: &str, session_id: &str) -> Option<usize> {
        self.tabs.iter().position(|tab| {
            tab.session_id.as_deref() == Some(session_id)
                && tab.project_path.as_deref() == Some(project_path)
        })
    }

    fn open_claude_session(
        &mut self,
        project_path: &str,
        session_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(index) = self.tab_for_session(project_path, session_id) {
            self.active_tab = index;
            cx.notify();
            return;
        }

        // Hide unresumable sessions the same way the previous frontend did.
        let resumable = ClaudeSessionService::list_sessions_for_project(project_path, None)
            .map(|sessions| {
                sessions
                    .iter()
                    .any(|session| session.session_id == session_id && !session.is_sidechain)
            })
            .unwrap_or(false);
        if !resumable {
            self.settings
                .sessions
                .hidden
                .insert(session_id.to_string(), true);
            self.persist_settings(cx);
            self.show_status(self.t("status_session_hidden_invalid").to_string(), cx);
            return;
        }

        let label = self
            .projects
            .iter()
            .find(|project| project.path == project_path)
            .and_then(|project| {
                project
                    .sessions
                    .iter()
                    .find(|session| session.session_id == session_id)
            })
            .map(|session| self.session_label(session))
            .unwrap_or_else(|| session_id.to_string());

        let view = cx.new(|cx| {
            TerminalView::new(
                PathBuf::from(project_path),
                TerminalLaunch::ClaudeResume {
                    session_id: session_id.to_string(),
                    claude_args: self.claude_args(),
                },
                cx,
            )
        });
        self.tabs.push(TerminalTab {
            key: format!("{}:{}", project_path, session_id),
            title: label,
            project_path: Some(project_path.to_string()),
            session_id: Some(session_id.to_string()),
            view,
        });
        self.active_tab = self.tabs.len() - 1;
        self.focus_active_terminal(window, cx);
        self.settings.sessions.last_opened = Some(LastOpenedSession {
            project_path: project_path.to_string(),
            session_id: session_id.to_string(),
        });
        self.persist_settings(cx);
        cx.notify();
    }

    fn focus_active_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.tabs.get(self.active_tab) {
            let handle = tab.view.read(cx).focus_handle(cx);
            window.focus(&handle, cx);
        }
    }

    fn new_claude_session(
        &mut self,
        project_path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let label = PathBuf::from(project_path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("claude")
            .to_string();
        let view = cx.new(|cx| {
            TerminalView::new(
                PathBuf::from(project_path),
                TerminalLaunch::ClaudeResume {
                    session_id: String::new(),
                    claude_args: self.claude_args(),
                },
                cx,
            )
        });
        self.tabs.push(TerminalTab {
            key: format!("{}:new-claude-{}", project_path, self.tabs.len()),
            title: label,
            project_path: Some(project_path.to_string()),
            session_id: None,
            view,
        });
        self.active_tab = self.tabs.len() - 1;
        self.focus_active_terminal(window, cx);
        cx.notify();
    }

    fn new_plain_terminal(
        &mut self,
        project_path: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let home = dirs::home_dir()
            .map(|home| home.to_string_lossy().to_string())
            .unwrap_or_else(|| "/".to_string());
        let working_dir = PathBuf::from(project_path.unwrap_or(&home));
        let label = working_dir
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("terminal")
            .to_string();
        let view = cx.new(|cx| TerminalView::new(working_dir, TerminalLaunch::Plain, cx));
        self.tabs.push(TerminalTab {
            key: format!("plain-{}", self.tabs.len()),
            title: label,
            project_path: project_path.map(str::to_string),
            session_id: None,
            view,
        });
        self.active_tab = self.tabs.len() - 1;
        self.focus_active_terminal(window, cx);
        cx.notify();
    }

    fn close_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.tabs.len() {
            self.tabs.remove(index);
            if self.active_tab >= self.tabs.len() {
                self.active_tab = self.tabs.len().saturating_sub(1);
            }
            cx.notify();
        }
    }

    // ----- sidebar actions -----

    fn toggle_project(&mut self, path: &str, cx: &mut Context<Self>) {
        let expanded = self.expanded.contains(path);
        if expanded {
            self.expanded.remove(path);
        } else {
            self.expanded.insert(path.to_string());
            self.load_sessions_for(path, cx);
        }
        self.settings
            .ui
            .project_tree
            .expanded_projects
            .insert(path.to_string(), !expanded);
        self.persist_settings(cx);
        cx.notify();
    }

    fn begin_rename(
        &mut self,
        session_id: &str,
        current: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let field = cx.new(|cx| TextField::new_value(current, "", cx));
        window.focus(&field.read(cx).focus_handle(cx), cx);
        let dashboard = cx.entity().downgrade();
        field.update(cx, |field, _| {
            field.set_on_commit(move |_, window, cx| {
                if let Some(dashboard) = dashboard.upgrade() {
                    dashboard.update(cx, |dashboard, cx| dashboard.commit_rename(window, cx));
                }
            });
        });
        let dashboard = cx.entity().downgrade();
        field.update(cx, |field, _| {
            field.set_on_cancel(move |_window, cx| {
                if let Some(dashboard) = dashboard.upgrade() {
                    dashboard.update(cx, |dashboard, cx| {
                        dashboard.overlay = Overlay::None;
                        dashboard.rename_field = None;
                        cx.notify();
                    });
                }
            });
        });
        self.overlay = Overlay::Rename {
            session_id: session_id.to_string(),
        };
        self.rename_field = Some(field);
        cx.notify();
    }

    fn commit_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Overlay::Rename { session_id, .. } = &self.overlay else {
            return;
        };
        let session_id = session_id.clone();
        let value = self
            .rename_field
            .as_ref()
            .map(|field| field.read(cx).value().trim().to_string());
        if let Some(value) = value {
            if value.is_empty() {
                self.settings.sessions.aliases.remove(&session_id);
                self.show_status(self.t("status_session_alias_cleared").to_string(), cx);
            } else {
                if let Some(project_path) = self
                    .tabs
                    .iter()
                    .find(|tab| tab.session_id.as_deref() == Some(&session_id))
                    .and_then(|tab| tab.project_path.clone())
                    .or_else(|| {
                        self.projects
                            .iter()
                            .find(|project| {
                                project
                                    .sessions
                                    .iter()
                                    .any(|session| session.session_id == session_id)
                            })
                            .map(|project| project.path.clone())
                    })
                {
                    if let Err(error) = ClaudeSessionService::rename_claude_session(
                        &project_path,
                        &session_id,
                        &value,
                    ) {
                        log::warn!("Failed to rename claude session {}: {}", session_id, error);
                    }
                }
                self.settings
                    .sessions
                    .aliases
                    .insert(session_id.clone(), value.clone());
                self.show_status(
                    self.tf("status_session_alias_saved", &[("name", &value)]),
                    cx,
                );
            }
            self.persist_settings(cx);
            if let Some(tab) = self
                .tabs
                .iter_mut()
                .find(|tab| tab.session_id.as_deref() == Some(&session_id))
            {
                tab.title = value;
            }
        }
        self.overlay = Overlay::None;
        self.rename_field = None;
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    fn request_delete_session(
        &mut self,
        project_path: &str,
        session_id: &str,
        cx: &mut Context<Self>,
    ) {
        let label = self
            .projects
            .iter()
            .find(|project| project.path == project_path)
            .and_then(|project| {
                project
                    .sessions
                    .iter()
                    .find(|session| session.session_id == session_id)
            })
            .map(|session| self.session_label(session))
            .unwrap_or_default();
        let message = format!("{}\n\n{}", label, self.t("confirm_delete_session"));
        self.overlay = Overlay::Confirm(ConfirmState {
            kind: ConfirmKind::DeleteSession {
                project_path: project_path.to_string(),
                session_id: session_id.to_string(),
            },
            message,
        });
        cx.notify();
    }

    fn perform_delete_session(
        &mut self,
        project_path: &str,
        session_id: &str,
        cx: &mut Context<Self>,
    ) {
        match ClaudeSessionService::delete_claude_session(project_path, session_id) {
            Ok(()) => {
                if let Some(index) = self.tab_for_session(project_path, session_id) {
                    self.close_tab(index, cx);
                }
                self.show_status(self.t("status_session_deleted").to_string(), cx);
                self.load_sessions_for(project_path, cx);
            }
            Err(error) => {
                log::warn!("Failed to delete session {}: {}", session_id, error);
                self.show_status(self.t("status_session_delete_failed").to_string(), cx);
            }
        }
    }

    fn request_remove_project(&mut self, path: &str, cx: &mut Context<Self>) {
        let name_path = PathBuf::from(path);
        let name = name_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(path)
            .to_string();
        let message = self.tf("confirm_delete_project", &[("name", &name)]);
        self.overlay = Overlay::Confirm(ConfirmState {
            kind: ConfirmKind::DeleteProject {
                path: path.to_string(),
            },
            message,
        });
        cx.notify();
    }

    fn perform_remove_project(&mut self, path: &str, cx: &mut Context<Self>) {
        let name = PathBuf::from(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(path)
            .to_string();
        if let Ok(stored) = self.project_service.list_projects() {
            if let Some(project) = stored.iter().find(|project| project.path == path) {
                let id = project.id.clone();
                if let Err(error) = self.project_service.delete_project(&id) {
                    log::warn!("Failed to delete project {}: {}", id, error);
                }
            }
        }
        self.projects.retain(|project| project.path != path);
        self.settings
            .ui
            .project_tree
            .project_order
            .retain(|p| p != path);
        self.expanded.remove(path);
        self.persist_settings(cx);
        self.show_status(self.tf("status_project_removed", &[("name", &name)]), cx);
    }

    fn add_project(&mut self, cx: &mut Context<Self>) {
        let task = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: None,
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = task.await else {
                this.update(cx, |this, cx| {
                    this.show_status(this.t("status_project_create_cancelled").to_string(), cx);
                })
                .ok();
                return;
            };
            let Some(path) = paths.first() else {
                return;
            };
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Project")
                .to_string();
            let path_string = path.to_string_lossy().to_string();
            this.update(cx, |this, cx| {
                match this
                    .project_service
                    .create_project(name.clone(), path_string.clone())
                {
                    Ok(_) => {
                        if !this
                            .settings
                            .ui
                            .project_tree
                            .project_order
                            .contains(&path_string)
                        {
                            this.settings
                                .ui
                                .project_tree
                                .project_order
                                .push(path_string.clone());
                        }
                        this.persist_settings(cx);
                        this.show_status(this.tf("status_project_added", &[("name", &name)]), cx);
                        this.refresh_projects(cx);
                    }
                    Err(error) => {
                        this.show_status(
                            this.tf("status_project_create_failed", &[("message", &error)]),
                            cx,
                        );
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    fn open_project_in_external_terminal(&mut self, path: &str, cx: &mut Context<Self>) {
        let app = self.settings.integrations.default_external_terminal.clone();
        let terminal_app = crate::models::terminal::TerminalApp::from_display_name(&app)
            .unwrap_or(crate::models::terminal::TerminalApp::Terminal);
        match crate::utils::open_terminal_with_path(terminal_app, path) {
            Ok(()) => self.show_status(self.tf("status_opened_in_app", &[("app", &app)]), cx),
            Err(_) => self.show_status(self.t("status_open_terminal_failed").to_string(), cx),
        }
    }

    fn open_project_in_editor(&mut self, path: &str, cx: &mut Context<Self>) {
        match crate::services::editor::open_project_in_editor(
            path,
            &self.settings.integrations.default_external_editor,
        ) {
            Ok(()) => {}
            Err(_) => self.show_status(self.t("status_open_editor_failed").to_string(), cx),
        }
    }

    // ----- settings / config -----

    pub fn toggle_sidebar_action(&mut self, cx: &mut Context<Self>) {
        self.settings.ui.sidebar_collapsed = !self.settings.ui.sidebar_collapsed;
        self.persist_settings(cx);
        let settings_snapshot = self.settings.clone();
        crate::refresh_menus(cx, &settings_snapshot);
        cx.notify();
    }

    pub fn new_terminal_action(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let project = self
            .projects
            .iter()
            .find(|project| self.expanded.contains(&project.path))
            .map(|project| project.path.clone());
        self.new_plain_terminal(project.as_deref(), window, cx);
    }

    pub fn new_claude_session_action(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let project = self
            .projects
            .iter()
            .find(|project| self.expanded.contains(&project.path))
            .map(|project| project.path.clone());
        if let Some(project) = project {
            self.new_claude_session(&project, window, cx);
        }
    }

    pub fn open_settings(&mut self, cx: &mut Context<Self>) {
        self.settings_pane
            .update(cx, |pane, cx| pane.sync_from(&self.settings, cx));
        self.overlay = Overlay::Settings;
        cx.notify();
    }

    pub fn reload_config(&mut self, cx: &mut Context<Self>) {
        match self.settings_service.get_settings() {
            Ok(latest) => {
                self.settings = latest;
                self.show_status(self.t("status_reload_settings").to_string(), cx);
            }
            Err(_) => {
                self.show_status(self.t("status_settings_load_failed").to_string(), cx);
            }
        }
    }

    pub fn open_config_file(&mut self, cx: &mut Context<Self>) {
        let path = StorageService::preferences_file();
        let _ = std::process::Command::new("open").arg(&path).spawn();
        let _ = cx;
    }

    fn apply_settings_pane(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let (mut draft, language_changed) = self
            .settings_pane
            .update(cx, |pane, cx| pane.take_draft(cx));
        draft.version = self.settings.version;
        draft.ui = self.settings.ui.clone();
        draft.sessions = self.settings.sessions.clone();
        self.settings = draft;
        self.persist_settings(cx);
        let _ = language_changed;
        let settings_snapshot = self.settings.clone();
        crate::refresh_menus(cx, &settings_snapshot);
        self.overlay = Overlay::None;
        cx.notify();
    }

    // ----- update flow -----

    pub fn check_for_updates(&mut self, cx: &mut Context<Self>) {
        if self.update.busy {
            self.show_status(self.t("status_update_busy").to_string(), cx);
            return;
        }
        self.update.busy = true;
        self.update.phase = UpdatePhase::Checking;
        self.update.error = None;
        cx.notify();
        let task = cx.background_executor().spawn(async move {
            UpdateService::fetch_latest_release_info(
                env!("CARGO_PKG_VERSION"),
                std::env::consts::ARCH,
            )
        });
        let this = cx.entity();
        cx.spawn(async move |_, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                this.update.busy = false;
                match result {
                    Ok(info) => {
                        this.update.result = Some(info.clone());
                        if info.update_available {
                            this.update.phase = UpdatePhase::Available;
                        } else {
                            this.update.phase = UpdatePhase::UpToDate;
                        }
                    }
                    Err(error) => {
                        this.update.phase = UpdatePhase::Failed;
                        this.update.error = Some(error);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn open_update_dialog(&mut self, cx: &mut Context<Self>) {
        self.overlay = Overlay::Update;
        if self.update.phase == UpdatePhase::Idle {
            self.check_for_updates(cx);
        }
        cx.notify();
    }

    fn download_update(&mut self, cx: &mut Context<Self>) {
        let Some(result) = self.update.result.clone() else {
            return;
        };
        if !result.update_available || self.update.busy {
            return;
        }
        self.update.busy = true;
        self.update.phase = UpdatePhase::Downloading;
        self.show_status(
            self.tf(
                "status_update_downloading",
                &[("version", &result.latest_version)],
            ),
            cx,
        );
        cx.notify();
        let task = cx.background_executor().spawn(async move {
            UpdateService::download_update(
                &result.download_url,
                &result.asset_name,
                &result.expected_sha256,
            )
            .and_then(|path| UpdateService::open_downloaded_installer(&path).map(|_| path))
        });
        let this = cx.entity();
        cx.spawn(async move |_, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                this.update.busy = false;
                match result {
                    Ok(path) => {
                        this.update.phase = UpdatePhase::Completed;
                        let version = this
                            .update
                            .result
                            .as_ref()
                            .map(|r| r.latest_version.clone())
                            .unwrap_or_default();
                        this.show_status(
                            this.tf("status_update_installer_opened", &[("version", &version)]),
                            cx,
                        );
                        log::info!("Update installer opened at {}", path.display());
                    }
                    Err(error) => {
                        this.update.phase = UpdatePhase::Failed;
                        this.update.error = Some(error.clone());
                        this.show_status(
                            this.tf("status_update_failed", &[("message", &error)]),
                            cx,
                        );
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    // ----- sidebar resize -----

    fn on_sidebar_resize(
        &mut self,
        event: &gpui::MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.resizing_sidebar {
            return;
        }
        let width = f32::from(event.position.x).clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH);
        if (width - self.sidebar_width).abs() >= 1.0 {
            self.sidebar_width = width;
            cx.notify();
        }
    }

    fn end_sidebar_resize(
        &mut self,
        _: &gpui::MouseUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.resizing_sidebar {
            self.resizing_sidebar = false;
            self.settings.ui.layout.sidebar_width = self.sidebar_width as u32;
            self.persist_settings(cx);
        }
    }

    fn on_sidebar_scroll(&mut self, _: &ScrollWheelEvent, _: &mut Window, _: &mut Context<Self>) {}
}

impl gpui::Focusable for Dashboard {
    fn focus_handle(&self, _: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Dashboard {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme(cx);
        crate::CurrentTheme::set(&theme, cx);

        let sidebar_collapsed = self.settings.ui.sidebar_collapsed;
        let status = self.status.as_ref().map(|(message, _)| message.clone());

        let mut root = div()
            .id("dashboard")
            .track_focus(&self.focus_handle)
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.app_bg)
            .text_color(theme.text_main)
            .on_mouse_move(cx.listener(Self::on_sidebar_resize))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::end_sidebar_resize))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::end_sidebar_resize));

        let mut main_row = div().flex_1().min_h_0().flex();

        if !sidebar_collapsed {
            let sidebar = self.render_sidebar(&theme, cx);
            let resizer = div()
                .id("sidebar-resize")
                .w(px(4.0))
                .flex_none()
                .cursor_col_resize()
                .hover(|this| this.bg(theme.border_color))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.resizing_sidebar = true;
                        cx.notify();
                    }),
                )
                .child(
                    div()
                        .id("sidebar-scroll-capture")
                        .on_scroll_wheel(cx.listener(Self::on_sidebar_scroll)),
                );
            main_row = main_row
                .child(
                    div()
                        .flex()
                        .flex_none()
                        .w(px(self.sidebar_width))
                        .child(sidebar),
                )
                .child(resizer);
        }
        main_row = main_row.child(self.render_workspace(&theme, window, cx));

        root = root.child(main_row);

        let status_bar = div()
            .h(px(26.0))
            .flex_none()
            .flex()
            .items_center()
            .px(px(10.0))
            .border_t_1()
            .border_color(theme.border_color)
            .bg(theme.panel_bg)
            .text_size(px(11.0))
            .text_color(theme.text_sub)
            .child(status.unwrap_or_else(|| self.t("status_ready").to_string()));
        root = root.child(status_bar);

        match &self.overlay {
            Overlay::None => {}
            Overlay::Settings => {
                root = root.child(self.render_settings_overlay(&theme, window, cx));
            }
            Overlay::Update => {
                root = root.child(render_update_overlay(self, &theme, window, cx));
            }
            Overlay::Confirm(state) => {
                root = root.child(render_confirm_overlay(self, state, &theme, window, cx));
            }
            Overlay::Rename { .. } => {
                if let Some(field) = self.rename_field.clone() {
                    root = root.child(render_rename_overlay(self, field, &theme, window, cx));
                }
            }
            Overlay::ProjectMenu { path } => {
                root = root.child(render_project_menu_overlay(self, path, &theme, window, cx));
            }
        }
        root
    }
}

// ----- sidebar -----

impl Dashboard {
    fn render_sidebar(&mut self, theme: &Theme, cx: &mut Context<Self>) -> gpui::AnyElement {
        let mut list = div()
            .id("sidebar-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll();

        if self.projects.is_empty() {
            list = list.child(
                div()
                    .px(px(12.0))
                    .py(px(10.0))
                    .text_size(px(12.0))
                    .text_color(theme.text_sub)
                    .child(self.t("tree_no_projects").to_string()),
            );
        }

        let project_paths: Vec<String> = self.projects.iter().map(|p| p.path.clone()).collect();
        for path in project_paths {
            let Some(index) = self.projects.iter().position(|p| p.path == path) else {
                continue;
            };
            let is_expanded = self.expanded.contains(&path);
            let display_name = self.projects[index].display_name().to_string();
            let sessions = self.visible_sessions(&self.projects[index]);
            let session_count = sessions.len();

            let mut rows = Vec::new();

            // Project row.
            let path_for_actions = path.clone();
            let path_for_toggle = path.clone();
            let path_for_new = path.clone();
            let project_row = div()
                .id(("project", index))
                .w_full()
                .flex()
                .items_center()
                .gap(px(4.0))
                .px(px(6.0))
                .py(px(4.0))
                .rounded_md()
                .text_size(px(12.5))
                .hover(|this| this.bg(theme.hover_bg))
                .on_click({
                    let this = cx.entity();
                    move |_, window, cx| {
                        this.update(cx, |this, cx| this.toggle_project(&path_for_toggle, cx));
                        window.prevent_default();
                    }
                })
                .child(
                    div()
                        .id(("project-expand", index))
                        .text_color(theme.text_sub)
                        .text_size(px(10.0))
                        .w(px(12.0))
                        .flex_none()
                        .child(if is_expanded { "▾" } else { "▸" }),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(display_name.clone()),
                )
                .child(
                    div()
                        .id(("project-new-claude", index))
                        .text_size(px(11.0))
                        .text_color(theme.text_sub)
                        .hover(|this| this.text_color(theme.text_main))
                        .child("+")
                        .on_click({
                            let this = cx.entity();
                            move |_, window, cx| {
                                this.update(cx, |this, cx| {
                                    this.new_claude_session(&path_for_new, window, cx)
                                });
                                window.prevent_default();
                                cx.stop_propagation();
                            }
                        }),
                )
                .child(
                    div()
                        .id(("project-more", index))
                        .text_size(px(11.0))
                        .text_color(theme.text_sub)
                        .hover(|this| this.text_color(theme.text_main))
                        .child("⋯")
                        .on_click({
                            let this = cx.entity();
                            move |_, window, cx| {
                                this.update(cx, |this, cx| {
                                    this.overlay = Overlay::ProjectMenu {
                                        path: path_for_actions.clone(),
                                    };
                                    cx.notify();
                                });
                                window.prevent_default();
                                cx.stop_propagation();
                            }
                        }),
                );
            rows.push(project_row.into_any_element());

            if is_expanded {
                if !self.projects[index].sessions_loaded {
                    rows.push(
                        div()
                            .id(("project-loading", index))
                            .pl(px(30.0))
                            .py(px(2.0))
                            .text_size(px(11.0))
                            .text_color(theme.text_sub)
                            .child("…")
                            .into_any_element(),
                    );
                } else if sessions.is_empty() {
                    rows.push(
                        div()
                            .id(("project-empty", index))
                            .pl(px(30.0))
                            .py(px(2.0))
                            .text_size(px(11.0))
                            .text_color(theme.text_sub)
                            .child(self.t("tree_no_sessions").to_string())
                            .into_any_element(),
                    );
                } else {
                    let limit = 8;
                    let mut row_id = index * 1000;
                    for (session_index, session) in sessions.iter().enumerate() {
                        let _ = session_index;
                        row_id += 1;
                        if session_index >= limit {
                            let remaining = session_count - limit;
                            let count_string = remaining.to_string();
                            rows.push(
                                div()
                                    .id(("session-more", index))
                                    .pl(px(30.0))
                                    .py(px(2.0))
                                    .text_size(px(11.0))
                                    .text_color(theme.text_sub)
                                    .child(
                                        self.t("show_more_count").replace("{count}", &count_string),
                                    )
                                    .into_any_element(),
                            );
                            break;
                        }
                        let session_id = session.session_id.clone();
                        let label = self.session_label(session);
                        let modified = session.modified.clone();
                        let is_open = self.tab_for_session(&path, &session_id).is_some();
                        let project_path_for_open = path.clone();
                        let session_id_for_open = session_id.clone();
                        let project_path_for_delete = path.clone();
                        let session_id_for_delete = session_id.clone();
                        let session_id_for_rename = session_id.clone();
                        let project_path_for_stop = path.clone();
                        let session_id_for_stop = session_id.clone();
                        let current_label = label.clone();

                        let row = div()
                            .id(("session-row", row_id))
                            .w_full()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .pl(px(26.0))
                            .pr(px(6.0))
                            .py(px(2.5))
                            .rounded_md()
                            .text_size(px(11.5))
                            .when(is_open, |this| this.bg(theme.selected_bg))
                            .hover(|this| this.bg(theme.hover_bg))
                            .on_click({
                                let this = cx.entity();
                                move |_, window, cx| {
                                    this.update(cx, |this, cx| {
                                        this.open_claude_session(
                                            &project_path_for_open,
                                            &session_id_for_open,
                                            window,
                                            cx,
                                        )
                                    });
                                    window.prevent_default();
                                }
                            })
                            .child(
                                div()
                                    .w(px(6.0))
                                    .h(px(6.0))
                                    .rounded_full()
                                    .flex_none()
                                    .when(is_open, |this| this.bg(theme.accent))
                                    .when(!is_open, |this| this.bg(theme.border_soft)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .text_color(theme.text_main)
                                    .child(label.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(theme.text_sub)
                                    .flex_none()
                                    .child(short_time(&modified)),
                            )
                            .child(
                                div()
                                    .id(("session-rename", row_id))
                                    .flex_none()
                                    .text_size(px(11.0))
                                    .text_color(theme.text_sub)
                                    .hover(|this| this.text_color(theme.text_main))
                                    .child("✎")
                                    .on_click({
                                        let this = cx.entity();
                                        move |_, window, cx| {
                                            let label = current_label.clone();
                                            let id = session_id_for_rename.clone();
                                            this.update(cx, |this, cx| {
                                                this.begin_rename(&id, &label, window, cx)
                                            });
                                            window.prevent_default();
                                            cx.stop_propagation();
                                        }
                                    }),
                            )
                            .child(
                                div()
                                    .id(("session-stop", row_id))
                                    .flex_none()
                                    .text_size(px(11.0))
                                    .text_color(theme.text_sub)
                                    .hover(|this| this.text_color(theme.alert_border))
                                    .child("■")
                                    .when(is_open, |this| this)
                                    .when(!is_open, |this| this.opacity(0.0))
                                    .on_click({
                                        let this = cx.entity();
                                        move |_, window, cx| {
                                            this.update(cx, |this, cx| {
                                                if let Some(index) = this.tab_for_session(
                                                    &project_path_for_stop,
                                                    &session_id_for_stop,
                                                ) {
                                                    this.close_tab(index, cx);
                                                }
                                            });
                                            window.prevent_default();
                                            cx.stop_propagation();
                                        }
                                    }),
                            )
                            .child(
                                div()
                                    .id(("session-delete", row_id))
                                    .flex_none()
                                    .text_size(px(11.0))
                                    .text_color(theme.text_sub)
                                    .hover(|this| this.text_color(theme.alert_border))
                                    .child("✕")
                                    .on_click({
                                        let this = cx.entity();
                                        move |_, window, cx| {
                                            this.update(cx, |this, cx| {
                                                this.request_delete_session(
                                                    &project_path_for_delete,
                                                    &session_id_for_delete,
                                                    cx,
                                                )
                                            });
                                            window.prevent_default();
                                            cx.stop_propagation();
                                        }
                                    }),
                            );
                        rows.push(row.into_any_element());
                    }
                }
            }

            list = list.child(
                div()
                    .id(("project-group", index))
                    .flex()
                    .flex_col()
                    .pb(px(1.0))
                    .children(rows),
            );
        }

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.panel_bg)
            .border_r_1()
            .border_color(theme.border_color)
            .child(
                div()
                    .h(px(34.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(10.0))
                    .border_b_1()
                    .border_color(theme.border_color)
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("Claude Session Switch"),
                    )
                    .child(
                        div()
                            .id("sidebar-add-project")
                            .text_size(px(13.0))
                            .text_color(theme.text_sub)
                            .hover(|this| this.text_color(theme.text_main))
                            .child("＋")
                            .on_click({
                                let this = cx.entity();
                                move |_, window, cx| {
                                    this.update(cx, |this, cx| this.add_project(cx));
                                    window.prevent_default();
                                }
                            }),
                    ),
            )
            .child(list)
            .child(
                div()
                    .h(px(30.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(10.0))
                    .border_t_1()
                    .border_color(theme.border_color)
                    .child(
                        div()
                            .id("sidebar-new-terminal")
                            .text_size(px(11.5))
                            .text_color(theme.text_sub)
                            .hover(|this| this.text_color(theme.text_main))
                            .child(self.t("menu_new_terminal_session").to_string())
                            .on_click({
                                let this = cx.entity();
                                move |_, window, cx| {
                                    this.update(cx, |this, cx| {
                                        let project = this
                                            .projects
                                            .iter()
                                            .find(|project| {
                                                this.expanded.contains(&project.path)
                                                    && project.sessions_loaded
                                            })
                                            .map(|project| project.path.clone());
                                        this.new_plain_terminal(project.as_deref(), window, cx);
                                    });
                                    window.prevent_default();
                                }
                            }),
                    ),
            )
            .into_any_element()
    }

    // ----- workspace -----

    fn render_workspace(
        &mut self,
        theme: &Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let mut workspace = div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(theme.app_bg);

        if self.tabs.is_empty() {
            workspace = workspace.child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(13.0))
                    .text_color(theme.text_sub)
                    .child(self.t("workspace_placeholder").to_string()),
            );
            return workspace.into_any_element();
        }

        // Tab bar.
        let mut tab_bar = div()
            .h(px(30.0))
            .flex_none()
            .flex()
            .items_center()
            .px(px(6.0))
            .gap(px(2.0))
            .border_b_1()
            .border_color(theme.border_color)
            .bg(theme.panel_bg)
            .overflow_hidden();

        let tab_keys: Vec<String> = self.tabs.iter().map(|tab| tab.key.clone()).collect();
        for (index, _key) in tab_keys.iter().enumerate() {
            let is_active = index == self.active_tab;
            let title = self.tabs[index]
                .view
                .read(cx)
                .dyn_title()
                .filter(|title| !title.trim().is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| self.tabs[index].title.clone());
            let tab = div()
                .id(("tab", index))
                .flex()
                .items_center()
                .gap(px(6.0))
                .px(px(10.0))
                .py(px(3.0))
                .rounded_md()
                .text_size(px(11.5))
                .max_w(px(220.0))
                .when(is_active, |this| {
                    this.bg(theme.selected_bg).text_color(theme.text_main)
                })
                .when(!is_active, |this| {
                    this.text_color(theme.text_sub)
                        .hover(|this| this.bg(theme.hover_bg))
                })
                .on_click({
                    let this = cx.entity();
                    move |_, window, cx| {
                        this.update(cx, |this, cx| {
                            this.active_tab = index;
                            cx.notify();
                        });
                        window.prevent_default();
                    }
                })
                .child(
                    div()
                        .overflow_hidden()
                        .text_ellipsis()
                        .flex_1()
                        .min_w_0()
                        .whitespace_nowrap()
                        .child(title),
                )
                .child(
                    div()
                        .id(("tab-close", index))
                        .flex_none()
                        .text_size(px(11.0))
                        .text_color(theme.text_sub)
                        .hover(|this| this.text_color(theme.alert_border))
                        .child("✕")
                        .on_click({
                            let this = cx.entity();
                            move |_, window, cx| {
                                this.update(cx, |this, cx| this.close_tab(index, cx));
                                window.prevent_default();
                                cx.stop_propagation();
                            }
                        }),
                );
            tab_bar = tab_bar.child(tab);
        }
        workspace = workspace.child(tab_bar);

        // Terminal area.
        let active = self.active_tab.min(self.tabs.len().saturating_sub(1));
        let mut area = div().flex_1().min_h_0().min_w_0().flex().flex_col();
        for (index, tab) in self.tabs.iter().enumerate() {
            let visible = index == active;
            area = area.child(
                div()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .when(!visible, |this| this.hidden())
                    .child(tab.view.clone()),
            );
        }
        let _ = window;
        workspace = workspace.child(area);
        workspace.into_any_element()
    }

    // ----- overlays -----

    fn render_settings_overlay(
        &mut self,
        theme: &Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let pane = self.settings_pane.clone();
        let mut overlay = render_modal_scrim(theme, 480.0, 520.0);
        overlay = overlay
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(16.0))
                    .h(px(40.0))
                    .border_b_1()
                    .border_color(theme.border_color)
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(self.t("settings_title").to_string()),
                    )
                    .child(
                        div()
                            .id("settings-close")
                            .text_size(px(12.0))
                            .text_color(theme.text_sub)
                            .hover(|this| this.text_color(theme.text_main))
                            .child("✕")
                            .on_click({
                                let this = cx.entity();
                                move |_, window, cx| {
                                    this.update(cx, |this, cx| {
                                        this.overlay = Overlay::None;
                                        cx.notify();
                                    });
                                    window.prevent_default();
                                }
                            }),
                    ),
            )
            .child(
                div()
                    .id("settings-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(px(16.0))
                    .py(px(12.0))
                    .flex()
                    .flex_col()
                    .gap(px(14.0))
                    .child(pane),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(16.0))
                    .h(px(44.0))
                    .border_t_1()
                    .border_color(theme.border_color)
                    .child(label_button(
                        "settings-cancel",
                        self.t("update_dialog_close"),
                        theme,
                        false,
                        {
                            let this = cx.entity();
                            move |_, window, cx| {
                                this.update(cx, |this, cx| {
                                    this.overlay = Overlay::None;
                                    cx.notify();
                                });
                                window.prevent_default();
                            }
                        },
                    ))
                    .child(label_button(
                        "settings-apply",
                        self.t("button_done"),
                        theme,
                        true,
                        {
                            let this = cx.entity();
                            move |_, window, cx| {
                                this.update(cx, |this, cx| {
                                    this.apply_settings_pane(window, cx);
                                });
                                window.prevent_default();
                            }
                        },
                    )),
            );
        let _ = window;
        overlay.into_any_element()
    }
}

fn render_modal_scrim(theme: &Theme, width: f32, height: f32) -> gpui::Div {
    let mut scrim = gpui::black();
    scrim.a = 0.35;
    div()
        .absolute()
        .inset_0()
        .bg(scrim)
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .w(px(width))
                .h(px(height))
                .rounded_lg()
                .bg(theme.app_bg)
                .border_1()
                .border_color(theme.border_color)
                .shadow_lg()
                .flex()
                .flex_col()
                .overflow_hidden(),
        )
}

fn short_time(timestamp: &str) -> String {
    // Claude timestamps are RFC3339-ish; show MM-DD HH:MM when parseable.
    chrono::DateTime::parse_from_rfc3339(timestamp)
        .map(|dt| {
            dt.with_timezone(&chrono::Local)
                .format("%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_default()
}

fn render_confirm_overlay(
    dashboard: &Dashboard,
    state: &ConfirmState,
    theme: &Theme,
    _window: &mut Window,
    cx: &mut Context<Dashboard>,
) -> impl IntoElement {
    let lang = dashboard.lang();
    let title = match &state.kind {
        ConfirmKind::DeleteSession { .. } => t(lang, "menu_delete"),
        ConfirmKind::DeleteProject { .. } => t(lang, "menu_delete"),
    };
    render_modal_scrim(theme, 380.0, 180.0)
        .child(
            div()
                .px(px(16.0))
                .py(px(14.0))
                .text_size(px(13.0))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .border_b_1()
                .border_color(theme.border_color)
                .child(title.to_string()),
        )
        .child(
            div()
                .flex_1()
                .px(px(16.0))
                .py(px(12.0))
                .text_size(px(12.0))
                .text_color(theme.text_main)
                .line_height(px(18.0))
                .child(state.message.replace('\n', " · ")),
        )
        .child(
            div()
                .flex()
                .justify_end()
                .gap(px(8.0))
                .px(px(16.0))
                .pb(px(12.0))
                .child(label_button(
                    "confirm-cancel",
                    t(lang, "update_dialog_close"),
                    theme,
                    false,
                    {
                        let this = cx.entity();
                        move |_, window, cx| {
                            this.update(cx, |this, cx| {
                                this.overlay = Overlay::None;
                                cx.notify();
                            });
                            window.prevent_default();
                        }
                    },
                ))
                .child(label_button(
                    "confirm-delete",
                    t(lang, "menu_delete"),
                    theme,
                    true,
                    {
                        let this = cx.entity();
                        let kind = state.kind.clone();
                        move |_, window, cx| {
                            this.update(cx, |this, cx| {
                                this.overlay = Overlay::None;
                                match kind.clone() {
                                    ConfirmKind::DeleteSession {
                                        project_path,
                                        session_id,
                                        ..
                                    } => {
                                        this.perform_delete_session(&project_path, &session_id, cx)
                                    }
                                    ConfirmKind::DeleteProject { path } => {
                                        this.perform_remove_project(&path, cx)
                                    }
                                }
                                cx.notify();
                            });
                            window.prevent_default();
                        }
                    },
                )),
        )
}

fn render_rename_overlay(
    dashboard: &Dashboard,
    field: gpui::Entity<TextField>,
    theme: &Theme,
    _window: &mut Window,
    cx: &mut Context<Dashboard>,
) -> impl IntoElement {
    let lang = dashboard.lang();
    render_modal_scrim(theme, 380.0, 150.0)
        .child(
            div()
                .px(px(16.0))
                .py(px(12.0))
                .text_size(px(13.0))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child(t(lang, "menu_edit_session_name").to_string()),
        )
        .child(
            div()
                .flex_1()
                .px(px(16.0))
                .flex()
                .items_center()
                .child(field),
        )
        .child(
            div()
                .flex()
                .justify_end()
                .gap(px(8.0))
                .px(px(16.0))
                .pb(px(12.0))
                .child(label_button(
                    "rename-cancel",
                    t(lang, "update_dialog_close"),
                    theme,
                    false,
                    {
                        let this = cx.entity();
                        move |_, window, cx| {
                            this.update(cx, |this, cx| {
                                this.overlay = Overlay::None;
                                this.rename_field = None;
                                cx.notify();
                            });
                            window.prevent_default();
                        }
                    },
                ))
                .child(label_button(
                    "rename-save",
                    t(lang, "button_done"),
                    theme,
                    true,
                    {
                        let this = cx.entity();
                        move |_, window, cx| {
                            this.update(cx, |this, cx| this.commit_rename(window, cx));
                            window.prevent_default();
                        }
                    },
                )),
        )
}

type PickHandler = Box<dyn Fn(&mut Window, &mut App) + 'static>;

fn render_project_menu_overlay(
    dashboard: &Dashboard,
    path: &str,
    theme: &Theme,
    _window: &mut Window,
    cx: &mut Context<Dashboard>,
) -> gpui::AnyElement {
    let lang = dashboard.lang();
    let name = PathBuf::from(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
        .to_string();

    fn menu_row(
        theme: &Theme,
        id: &'static str,
        label: &'static str,
        on_pick: PickHandler,
    ) -> impl IntoElement {
        label_button(id, label, theme, false, move |_, window, cx| {
            on_pick(window, cx);
        })
    }

    let path_for_terminal = path.to_string();
    let path_for_editor = path.to_string();
    let path_for_remove = path.to_string();

    let mut rows = div().flex().flex_col().gap(px(4.0));
    rows = rows.child(menu_row(
        theme,
        "project-menu-new-claude",
        dashboard.t("title_quick_new_session"),
        Box::new({
            let this = cx.entity();
            let path = path.to_string();
            move |window, cx| {
                this.update(cx, |this, cx| {
                    this.overlay = Overlay::None;
                    this.new_claude_session(&path, window, cx);
                });
            }
        }),
    ));
    rows = rows.child(menu_row(
        theme,
        "project-menu-open-terminal",
        dashboard.t("menu_open_project_terminal"),
        Box::new({
            let this = cx.entity();
            move |_window, cx| {
                this.update(cx, |this, cx| {
                    this.overlay = Overlay::None;
                    this.open_project_in_external_terminal(&path_for_terminal, cx);
                });
            }
        }),
    ));
    rows = rows.child(menu_row(
        theme,
        "project-menu-open-editor",
        dashboard.t("menu_open_project_editor"),
        Box::new({
            let this = cx.entity();
            move |_window, cx| {
                this.update(cx, |this, cx| {
                    this.overlay = Overlay::None;
                    this.open_project_in_editor(&path_for_editor, cx);
                });
            }
        }),
    ));
    rows = rows.child(menu_row(
        theme,
        "project-menu-remove",
        dashboard.t("menu_delete"),
        Box::new({
            let this = cx.entity();
            move |_window, cx| {
                this.update(cx, |this, cx| {
                    this.overlay = Overlay::None;
                    this.request_remove_project(&path_for_remove, cx);
                });
            }
        }),
    ));

    render_modal_scrim(theme, 300.0, 240.0)
        .child(
            div()
                .px(px(16.0))
                .py(px(12.0))
                .text_size(px(12.0))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .border_b_1()
                .border_color(theme.border_color)
                .text_ellipsis()
                .child(name),
        )
        .child(
            div()
                .flex_1()
                .px(px(16.0))
                .py(px(10.0))
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(rows),
        )
        .child(
            div()
                .flex()
                .justify_end()
                .px(px(16.0))
                .pb(px(10.0))
                .child(label_button(
                    "project-menu-cancel",
                    t(lang, "update_dialog_close"),
                    theme,
                    false,
                    {
                        let this = cx.entity();
                        move |_, window, cx| {
                            this.update(cx, |this, cx| {
                                this.overlay = Overlay::None;
                                cx.notify();
                            });
                            window.prevent_default();
                        }
                    },
                )),
        )
        .into_any_element()
}

fn render_update_overlay(
    dashboard: &Dashboard,
    theme: &Theme,
    _window: &mut Window,
    cx: &mut Context<Dashboard>,
) -> impl IntoElement {
    let lang = dashboard.lang();
    let update = &dashboard.update;
    let phase_label = match update.phase {
        UpdatePhase::Idle => t(lang, "update_dialog_phase_idle"),
        UpdatePhase::Checking => t(lang, "update_dialog_phase_checking"),
        UpdatePhase::Available => t(lang, "update_dialog_phase_available"),
        UpdatePhase::Downloading => t(lang, "update_dialog_phase_downloading"),
        UpdatePhase::UpToDate => t(lang, "update_dialog_phase_up_to_date"),
        UpdatePhase::Completed => t(lang, "update_dialog_phase_completed"),
        UpdatePhase::Failed => t(lang, "update_dialog_phase_error"),
    };
    let title = match update.phase {
        UpdatePhase::Checking => t(lang, "update_dialog_title_checking"),
        UpdatePhase::Available | UpdatePhase::Downloading => {
            t(lang, "update_dialog_title_available")
        }
        UpdatePhase::UpToDate => t(lang, "update_dialog_title_up_to_date"),
        UpdatePhase::Completed => t(lang, "update_dialog_title_completed"),
        UpdatePhase::Failed => t(lang, "update_dialog_title_error"),
        UpdatePhase::Idle => t(lang, "update_dialog_title"),
    };
    let result = update.result.as_ref();

    let detail_row = |label: &str, value: String| {
        div()
            .flex()
            .gap(px(8.0))
            .text_size(px(11.5))
            .child(
                div()
                    .w(px(80.0))
                    .flex_none()
                    .text_color(theme.text_sub)
                    .child(label.to_string()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_color(theme.text_main)
                    .child(value),
            )
    };

    let mut body = div().flex().flex_col().gap(px(6.0));
    body = body.child(detail_row(
        t(lang, "update_dialog_current_version"),
        env!("CARGO_PKG_VERSION").to_string(),
    ));
    if let Some(result) = result {
        body = body
            .child(detail_row(
                t(lang, "update_dialog_latest_version"),
                result.latest_version.clone(),
            ))
            .child(detail_row(
                t(lang, "update_dialog_published_at"),
                result.published_at.clone(),
            ))
            .child(detail_row(
                t(lang, "update_dialog_status"),
                phase_label.to_string(),
            ));
        if result.update_available {
            let mut notes = div()
                .mt(px(6.0))
                .p(px(10.0))
                .rounded_md()
                .bg(theme.panel_bg)
                .border_1()
                .border_color(theme.border_color)
                .text_size(px(11.0))
                .line_height(px(16.0))
                .text_color(theme.text_main)
                .max_h(px(180.0))
                .id("update-notes")
                .overflow_y_scroll();
            let note_text = if result.release_notes.trim().is_empty() {
                t(lang, "update_dialog_no_release_notes").to_string()
            } else {
                result.release_notes.clone()
            };
            notes = notes.child(note_text);
            body = body.child(notes);
        }
    } else {
        body = body.child(detail_row(
            t(lang, "update_dialog_status"),
            phase_label.to_string(),
        ));
    }
    if let Some(error) = &update.error {
        body = body.child(
            div()
                .mt(px(6.0))
                .p(px(10.0))
                .rounded_md()
                .bg(theme.alert_bg)
                .border_1()
                .border_color(theme.alert_border)
                .text_size(px(11.0))
                .text_color(theme.alert_text)
                .child(error.clone()),
        );
    }

    let mut actions = div().flex().justify_end().gap(px(8.0));
    if update.phase == UpdatePhase::Available && result.is_some_and(|r| r.update_available) {
        actions = actions.child(label_button(
            "update-download",
            t(lang, "update_dialog_download"),
            theme,
            true,
            {
                let this = cx.entity();
                move |_, window, cx| {
                    this.update(cx, |this, cx| this.download_update(cx));
                    window.prevent_default();
                }
            },
        ));
    }
    if let Some(url) = result.map(|r| r.release_url.clone()) {
        actions = actions.child(label_button(
            "update-open-github",
            t(lang, "update_dialog_open_github"),
            theme,
            false,
            move |_, window, cx| {
                cx.open_url(&url);
                window.prevent_default();
            },
        ));
    }
    actions = actions.child(label_button(
        "update-close",
        t(lang, "update_dialog_close"),
        theme,
        false,
        {
            let this = cx.entity();
            move |_, window, cx| {
                this.update(cx, |this, cx| {
                    this.overlay = Overlay::None;
                    cx.notify();
                });
                window.prevent_default();
            }
        },
    ));

    render_modal_scrim(theme, 460.0, 420.0)
        .child(
            div()
                .px(px(16.0))
                .flex()
                .items_center()
                .justify_between()
                .h(px(40.0))
                .border_b_1()
                .border_color(theme.border_color)
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child(title.to_string()),
                )
                .child(
                    div()
                        .id("update-close-x")
                        .text_size(px(12.0))
                        .text_color(theme.text_sub)
                        .hover(|this| this.text_color(theme.text_main))
                        .child("✕")
                        .on_click({
                            let this = cx.entity();
                            move |_, window, cx| {
                                this.update(cx, |this, cx| {
                                    this.overlay = Overlay::None;
                                    cx.notify();
                                });
                                window.prevent_default();
                            }
                        }),
                ),
        )
        .child(
            div()
                .id("update-body")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .px(px(16.0))
                .py(px(12.0))
                .child(body),
        )
        .child(
            div()
                .flex()
                .items_center()
                .justify_end()
                .gap(px(8.0))
                .px(px(16.0))
                .h(px(44.0))
                .border_t_1()
                .border_color(theme.border_color)
                .child(actions),
        )
}

// ----- settings pane -----

pub struct SettingsPane {
    draft: AppSettings,
    language: AppLanguage,
    theme_preset_everforest: bool,
    theme_preference: ThemePreference,
    use_custom_args: bool,
    custom_args: String,
    restore_last_session: bool,
    external_terminal: String,
    external_editor: String,
    available_terminals: Vec<String>,
    available_editors: Vec<String>,
    args_field: gpui::Entity<TextField>,
}

impl SettingsPane {
    fn new(settings: AppSettings, cx: &mut Context<Self>) -> Self {
        let args_field = cx.new(|cx| {
            TextField::new_value(
                &settings.claude.custom_startup_args,
                "--dangerously-skip-permissions",
                cx,
            )
        });
        let language = AppLanguage::from_str(&settings.appearance.language);
        let theme_preset_everforest = settings
            .appearance
            .theme_preset
            .trim()
            .eq_ignore_ascii_case("everforest");
        Self {
            language,
            theme_preset_everforest,
            theme_preference: ThemePreference::from_str(&settings.appearance.theme_preference),
            use_custom_args: settings.claude.use_custom_startup_args,
            custom_args: settings.claude.custom_startup_args.clone(),
            restore_last_session: settings.sessions.restore_last_opened_session,
            external_terminal: settings.integrations.default_external_terminal.clone(),
            external_editor: settings.integrations.default_external_editor.clone(),
            available_terminals: crate::utils::detect_available_terminals()
                .into_iter()
                .map(|terminal| terminal.display_name().to_string())
                .collect(),
            available_editors: crate::services::editor::detect_available_editors(),
            args_field,
            draft: settings,
        }
    }

    fn sync_from(&mut self, settings: &AppSettings, cx: &mut Context<Self>) {
        self.language = AppLanguage::from_str(&settings.appearance.language);
        self.theme_preset_everforest = settings
            .appearance
            .theme_preset
            .trim()
            .eq_ignore_ascii_case("everforest");
        self.theme_preference = ThemePreference::from_str(&settings.appearance.theme_preference);
        self.use_custom_args = settings.claude.use_custom_startup_args;
        self.custom_args = settings.claude.custom_startup_args.clone();
        self.restore_last_session = settings.sessions.restore_last_opened_session;
        self.external_terminal = settings.integrations.default_external_terminal.clone();
        self.external_editor = settings.integrations.default_external_editor.clone();
        self.args_field
            .update(cx, |field, _| field.set_value(self.custom_args.clone()));
        cx.notify();
    }

    fn take_draft(&mut self, cx: &App) -> (AppSettings, bool) {
        let mut draft = self.draft.clone();
        self.custom_args = self.args_field.read(cx).value().to_string();
        let language_before = draft.appearance.language.clone();
        draft.appearance.language = self.language.as_str().to_string();
        draft.appearance.theme_preset = if self.theme_preset_everforest {
            "everforest".to_string()
        } else {
            "default".to_string()
        };
        draft.appearance.theme_preference = self.theme_preference.as_str().to_string();
        draft.claude.use_custom_startup_args = self.use_custom_args;
        draft.claude.custom_startup_args = self.custom_args.trim().to_string();
        draft.sessions.restore_last_opened_session = self.restore_last_session;
        draft.integrations.default_external_terminal = self.external_terminal.clone();
        draft.integrations.default_external_editor = self.external_editor.clone();
        let changed = language_before != draft.appearance.language;
        (draft, changed)
    }
}

impl Render for SettingsPane {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = crate::CurrentTheme::get(cx);
        let lang = self.language;

        let section = |title: &str| {
            div()
                .text_size(px(12.0))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(theme.text_main)
                .child(title.to_string())
        };
        let hint = |text: &str| {
            div()
                .text_size(px(10.5))
                .line_height(px(15.0))
                .text_color(theme.text_sub)
                .child(text.to_string())
        };
        fn choice(
            theme: &Theme,
            id: impl Into<gpui::ElementId>,
            label: &str,
            active: bool,
            on_pick: PickHandler,
        ) -> gpui::Stateful<gpui::Div> {
            div()
                .id(id)
                .px(px(10.0))
                .py(px(4.0))
                .rounded_md()
                .text_size(px(11.5))
                .when(active, |this| {
                    this.bg(theme.accent).text_color(gpui::white())
                })
                .when(!active, |this| {
                    this.bg(theme.button_bg)
                        .text_color(theme.button_text)
                        .hover(|this| this.bg(theme.button_hover))
                })
                .child(label.to_string())
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    on_pick(window, cx);
                })
        }

        let mut layout = div().flex().flex_col().gap(px(10.0));

        // Language.
        layout = layout.child(section(t(lang, "section_language")));
        {
            let zh = choice(
                &theme,
                "settings-lang-zh",
                t(lang, "language_zh_cn"),
                self.language == AppLanguage::ZhCn,
                Box::new({
                    let this = cx.entity();
                    move |_, cx| {
                        this.update(cx, |pane, cx| {
                            pane.language = AppLanguage::ZhCn;
                            cx.notify();
                        });
                    }
                }),
            );
            let en = choice(
                &theme,
                "settings-lang-en",
                t(lang, "language_en_us"),
                self.language == AppLanguage::EnUs,
                Box::new({
                    let this = cx.entity();
                    move |_, cx| {
                        this.update(cx, |pane, cx| {
                            pane.language = AppLanguage::EnUs;
                            cx.notify();
                        });
                    }
                }),
            );
            layout = layout
                .child(div().flex().gap(px(8.0)).child(zh).child(en))
                .child(hint(""));
        }

        // Theme preset.
        layout = layout.child(section(t(lang, "section_theme_preset")));
        {
            let default = choice(
                &theme,
                "settings-preset-default",
                t(lang, "theme_preset_default"),
                !self.theme_preset_everforest,
                Box::new({
                    let this = cx.entity();
                    move |_, cx| {
                        this.update(cx, |pane, cx| {
                            pane.theme_preset_everforest = false;
                            cx.notify();
                        });
                    }
                }),
            );
            let everforest = choice(
                &theme,
                "settings-preset-everforest",
                t(lang, "theme_preset_everforest"),
                self.theme_preset_everforest,
                Box::new({
                    let this = cx.entity();
                    move |_, cx| {
                        this.update(cx, |pane, cx| {
                            pane.theme_preset_everforest = true;
                            cx.notify();
                        });
                    }
                }),
            );
            layout = layout.child(div().flex().gap(px(8.0)).child(default).child(everforest));
            layout = layout.child(hint(t(lang, "hint_theme_preset")));
        }

        // Theme mode.
        layout = layout.child(section(t(lang, "section_theme_mode")));
        {
            let light = choice(
                &theme,
                "settings-mode-light",
                t(lang, "theme_light"),
                self.theme_preference == ThemePreference::Light,
                Box::new({
                    let this = cx.entity();
                    move |_, cx| {
                        this.update(cx, |pane, cx| {
                            pane.theme_preference = ThemePreference::Light;
                            cx.notify();
                        });
                    }
                }),
            );
            let dark = choice(
                &theme,
                "settings-mode-dark",
                t(lang, "theme_dark"),
                self.theme_preference == ThemePreference::Dark,
                Box::new({
                    let this = cx.entity();
                    move |_, cx| {
                        this.update(cx, |pane, cx| {
                            pane.theme_preference = ThemePreference::Dark;
                            cx.notify();
                        });
                    }
                }),
            );
            let system = choice(
                &theme,
                "settings-mode-system",
                t(lang, "theme_system"),
                self.theme_preference == ThemePreference::System,
                Box::new({
                    let this = cx.entity();
                    move |_, cx| {
                        this.update(cx, |pane, cx| {
                            pane.theme_preference = ThemePreference::System;
                            cx.notify();
                        });
                    }
                }),
            );
            layout = layout
                .child(
                    div()
                        .flex()
                        .gap(px(8.0))
                        .child(light)
                        .child(dark)
                        .child(system),
                )
                .child(hint(if self.theme_preset_everforest {
                    t(lang, "hint_theme_palette_builtin")
                } else {
                    t(lang, "hint_theme_palette_default")
                }));
        }

        // Claude startup args.
        layout = layout.child(section(t(lang, "section_claude_startup")));
        {
            let toggle_label = t(lang, "label_enable_custom_args").to_string();
            let toggle = choice(
                &theme,
                "settings-custom-args-toggle",
                &toggle_label,
                self.use_custom_args,
                Box::new({
                    let this = cx.entity();
                    move |_, cx| {
                        this.update(cx, |pane, cx| {
                            pane.use_custom_args = !pane.use_custom_args;
                            cx.notify();
                        });
                    }
                }),
            );
            let field = self.args_field.clone();
            layout = layout
                .child(toggle)
                .when(self.use_custom_args, |this| this.child(field))
                .child(hint(t(lang, "hint_claude_startup")));
        }

        // Session restore.
        layout = layout.child(section(t(lang, "section_session_restore")));
        {
            let toggle_label = t(lang, "label_restore_last_session").to_string();
            let toggle = choice(
                &theme,
                "settings-restore-toggle",
                &toggle_label,
                self.restore_last_session,
                Box::new({
                    let this = cx.entity();
                    move |_, cx| {
                        this.update(cx, |pane, cx| {
                            pane.restore_last_session = !pane.restore_last_session;
                            cx.notify();
                        });
                    }
                }),
            );
            layout = layout
                .child(toggle)
                .child(hint(t(lang, "hint_restore_last_session")));
        }

        // Integrations.
        layout = layout
            .child(section(t(lang, "section_external_terminal")))
            .child(hint(t(lang, "hint_integrations")));
        {
            let mut terminals = div().flex().flex_wrap().gap(px(6.0));
            for (terminal_index, name) in self.available_terminals.clone().into_iter().enumerate() {
                let active = name == self.external_terminal;
                let picked = name.clone();
                terminals = terminals.child(choice(
                    &theme,
                    ("settings-terminal-choice", terminal_index),
                    &name,
                    active,
                    Box::new({
                        let this = cx.entity();
                        move |_, cx| {
                            let picked = picked.clone();
                            this.update(cx, |pane, cx| {
                                pane.external_terminal = picked.clone();
                                cx.notify();
                            });
                        }
                    }),
                ));
            }
            layout = layout.child(terminals);
        }
        layout = layout.child(section(t(lang, "section_external_editor")));
        {
            let mut editors = div().flex().flex_wrap().gap(px(6.0));
            for (editor_index, name) in self.available_editors.clone().into_iter().enumerate() {
                let active = name == self.external_editor;
                let picked = name.clone();
                editors = editors.child(choice(
                    &theme,
                    ("settings-editor-choice", editor_index),
                    &name,
                    active,
                    Box::new({
                        let this = cx.entity();
                        move |_, cx| {
                            let picked = picked.clone();
                            this.update(cx, |pane, cx| {
                                pane.external_editor = picked.clone();
                                cx.notify();
                            });
                        }
                    }),
                ));
            }
            layout = layout.child(editors);
        }

        layout
    }
}
