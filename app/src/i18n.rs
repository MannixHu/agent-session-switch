//! Bilingual UI strings. Keys and translations mirror the previous web
//! frontend's dictionary exactly, so behavior and copy stay identical.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppLanguage {
    ZhCn,
    EnUs,
}

impl AppLanguage {
    pub fn from_str(value: &str) -> Self {
        if value == "en-US" {
            Self::EnUs
        } else {
            Self::ZhCn
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::ZhCn => "zh-CN",
            Self::EnUs => "en-US",
        }
    }
}

const ZH_CN: &[(&str, &str)] = &[
    ("confirm_delete_session", "删除此会话？此操作不可撤销。"),
    ("confirm_delete_project", "从侧边栏删除项目“{name}”？"),
    ("status_ready", "已就绪。"),
    ("status_load_failed", "加载项目/会话失败。"),
    ("status_project_create_cancelled", "已取消添加项目。"),
    ("status_project_added", "已添加项目：{name}"),
    ("status_project_create_failed", "添加项目失败：{message}"),
    ("status_session_deleted", "会话已删除。"),
    ("status_session_delete_failed", "删除会话失败。"),
    ("status_stop_terminal_failed", "停止会话终端失败。"),
    ("status_opened_in_app", "已在 {app} 中打开。"),
    ("status_open_terminal_failed", "外部终端打开失败。"),
    ("status_open_editor_failed", "编辑器打开失败。"),
    ("status_project_removed", "已移除项目：{name}"),
    ("status_project_delete_failed", "删除项目失败。"),
    ("status_session_alias_saved", "左侧会话显示名已保存：{name}"),
    ("status_session_alias_cleared", "已恢复默认会话显示名。"),
    (
        "status_session_hidden_invalid",
        "该会话无法恢复，已在左侧隐藏。",
    ),
    (
        "status_settings_load_failed",
        "读取配置失败，已使用当前内存设置。",
    ),
    ("status_settings_save_failed", "保存配置失败。"),
    ("status_reload_settings", "已重新加载配置文件。"),
    ("status_update_busy", "更新检查正在进行，请稍候。"),
    ("status_update_checking", "正在检查更新…"),
    ("status_update_up_to_date", "当前已是最新版本：{version}"),
    ("status_update_downloading", "正在下载更新：{version}"),
    ("status_update_installer_opened", "已打开安装包：{version}"),
    ("status_update_failed", "更新失败：{message}"),
    ("status_open_release_failed", "打开发布页失败：{message}"),
    (
        "status_terminal_write_failed",
        "终端写入失败，请重试或重开会话。",
    ),
    ("status_terminal_start_failed", "终端启动失败：{message}"),
    ("status_unexpected_error", "发生未处理错误，请查看日志。"),
    ("title_show_sidebar", "展开侧边栏"),
    ("title_hide_sidebar", "收起侧边栏"),
    ("title_switch_to_light", "切换浅色"),
    ("title_switch_to_dark", "切换深色"),
    ("title_add_project", "添加项目"),
    ("title_settings_shortcut", "设置"),
    ("title_remove_project_sidebar", "从侧边栏移除此项目"),
    ("title_quick_new_session", "快速新建 Claude 会话"),
    ("title_session_actions", "会话操作"),
    ("title_drag_resize_sidebar", "拖动调整侧边栏宽度"),
    ("settings_title", "设置"),
    ("aria_close_settings", "关闭设置"),
    ("nav_claude", "Claude"),
    ("nav_appearance", "外观"),
    ("nav_integrations", "集成"),
    ("section_language", "语言"),
    ("language_zh_cn", "中文"),
    ("language_en_us", "English"),
    ("section_theme_preset", "主题预设"),
    ("theme_preset_default", "Default"),
    ("theme_preset_everforest", "Everforest"),
    (
        "hint_theme_preset",
        "Default 保持当前这套主题；Everforest 保留 light / dark / system 三种模式。",
    ),
    ("section_theme_mode", "主题模式"),
    ("theme_light", "浅色"),
    ("theme_dark", "深色"),
    ("theme_system", "跟随系统"),
    (
        "hint_theme_palette_default",
        "Default 主题色板仍存储在 preferences.json 中，可继续直接编辑。",
    ),
    (
        "hint_theme_palette_builtin",
        "Everforest 使用内置预设色板，不读取 preferences.json 中的自定义主题色。",
    ),
    ("section_external_terminal", "外部终端"),
    ("section_external_editor", "外部编辑器"),
    (
        "hint_integrations",
        "会话菜单中的“终端打开项目 / 编辑器打开项目”将使用此默认配置，所有值都写入 preferences.json。",
    ),
    ("section_claude_startup", "Claude 启动参数"),
    ("label_enable_custom_args", "启用自定义 Claude 启动参数"),
    (
        "hint_claude_startup",
        "关闭时使用默认 `claude --resume`。开启后默认参数为 `--dangerously-skip-permissions`，并应用于会话恢复与快速新建会话。",
    ),
    ("section_session_restore", "会话恢复"),
    ("label_restore_last_session", "启动时恢复上次打开的会话"),
    (
        "hint_restore_last_session",
        "应用启动后自动恢复上次活跃会话。关闭后将回退到“最近会话优先”的默认行为。",
    ),
    ("button_done", "完成"),
    ("tree_no_sessions", "暂无会话"),
    ("tree_no_projects", "暂无项目"),
    ("menu_edit_session_name", "编辑显示名"),
    ("menu_open_project_terminal", "终端打开项目"),
    ("menu_open_project_editor", "编辑器打开项目"),
    ("menu_check_for_updates", "检查更新…"),
    ("menu_stop", "停止"),
    ("menu_delete", "删除"),
    (
        "confirm_update_available",
        "发现新版本 {latest}（当前 {current}）。现在下载并打开安装包吗？",
    ),
    (
        "alert_update_install_guidance",
        "已下载并打开 {version} 的安装包。请将新版本拖到 Applications 覆盖安装。",
    ),
    ("update_dialog_title", "应用更新"),
    ("update_dialog_title_checking", "正在检查更新"),
    ("update_dialog_title_available", "发现新版本"),
    ("update_dialog_title_downloading", "正在下载更新"),
    ("update_dialog_title_up_to_date", "当前已是最新版本"),
    ("update_dialog_title_completed", "安装包已准备好"),
    ("update_dialog_title_error", "更新失败"),
    (
        "update_dialog_subtitle",
        "查看版本内容，并在这里直接开始下载。",
    ),
    ("update_dialog_current_version", "当前版本"),
    ("update_dialog_latest_version", "最新版本"),
    ("update_dialog_published_at", "发布时间"),
    ("update_dialog_status", "状态"),
    ("update_dialog_phase_idle", "待检查"),
    ("update_dialog_phase_checking", "检查中"),
    ("update_dialog_phase_available", "可更新"),
    ("update_dialog_phase_downloading", "下载中"),
    ("update_dialog_phase_up_to_date", "已是最新"),
    ("update_dialog_phase_completed", "安装包已打开"),
    ("update_dialog_phase_error", "检查失败"),
    ("update_dialog_error_label", "错误详情"),
    ("update_dialog_install_ready", "安装包已经打开"),
    (
        "update_dialog_install_guidance",
        "已下载并打开 {version} 的安装包。把新版本拖到 Applications 覆盖安装即可。",
    ),
    ("update_dialog_changelog", "Changelog"),
    ("update_dialog_commit", "Commit"),
    ("update_dialog_description", "Description"),
    ("update_dialog_release_notes", "Release Notes"),
    (
        "update_dialog_no_release_notes",
        "这个版本没有额外的发布说明。",
    ),
    ("update_dialog_download", "下载并打开安装包"),
    ("update_dialog_open_github", "在 GitHub 查看"),
    ("update_dialog_close", "稍后"),
    ("update_dialog_working", "处理中…"),
    ("aria_close_update_dialog", "关闭更新弹窗"),
    ("show_less", "收起"),
    ("show_more_count", "再显示 {count} 条..."),
    (
        "workspace_placeholder",
        "请选择一个会话以打开 Claude Code。",
    ),
    ("fallback_new_project", "新项目"),
    ("menu_new_terminal_session", "新建终端"),
    ("menu_settings", "设置…"),
    ("menu_open_config_file", "打开配置文件"),
    ("menu_reload_config", "重新加载配置"),
];

const EN_US: &[(&str, &str)] = &[
    (
        "confirm_delete_session",
        "Delete this session? This cannot be undone.",
    ),
    (
        "confirm_delete_project",
        "Remove project \"{name}\" from sidebar?",
    ),
    ("status_ready", "Ready."),
    ("status_load_failed", "Failed to load projects/sessions."),
    (
        "status_project_create_cancelled",
        "Project creation canceled.",
    ),
    ("status_project_added", "Project added: {name}"),
    (
        "status_project_create_failed",
        "Create project failed: {message}",
    ),
    ("status_session_deleted", "Session deleted."),
    ("status_session_delete_failed", "Failed to delete session."),
    (
        "status_stop_terminal_failed",
        "Failed to stop terminal session.",
    ),
    ("status_opened_in_app", "Opened in {app}."),
    (
        "status_open_terminal_failed",
        "Failed to open project in external terminal.",
    ),
    (
        "status_open_editor_failed",
        "Failed to open project in editor.",
    ),
    ("status_project_removed", "Project removed: {name}"),
    ("status_project_delete_failed", "Delete project failed."),
    (
        "status_session_alias_saved",
        "Saved sidebar session label: {name}",
    ),
    (
        "status_session_alias_cleared",
        "Restored the default session label.",
    ),
    (
        "status_session_hidden_invalid",
        "This session could not be resumed and has been hidden from the sidebar.",
    ),
    (
        "status_settings_load_failed",
        "Failed to load settings file. Using current in-memory state.",
    ),
    (
        "status_settings_save_failed",
        "Failed to save settings file.",
    ),
    ("status_reload_settings", "Settings reloaded from file."),
    (
        "status_update_busy",
        "An update check is already in progress.",
    ),
    ("status_update_checking", "Checking for updates…"),
    (
        "status_update_up_to_date",
        "You're already on the latest version: {version}",
    ),
    ("status_update_downloading", "Downloading update: {version}"),
    (
        "status_update_installer_opened",
        "Opened installer for {version}",
    ),
    ("status_update_failed", "Update failed: {message}"),
    (
        "status_open_release_failed",
        "Failed to open release page: {message}",
    ),
    (
        "status_terminal_write_failed",
        "Terminal write failed. Please retry or reopen the session.",
    ),
    (
        "status_terminal_start_failed",
        "Terminal failed to start: {message}",
    ),
    (
        "status_unexpected_error",
        "An unexpected error occurred. Check logs for details.",
    ),
    ("title_show_sidebar", "Show sidebar"),
    ("title_hide_sidebar", "Hide sidebar"),
    ("title_switch_to_light", "Switch to light"),
    ("title_switch_to_dark", "Switch to dark"),
    ("title_add_project", "Add project"),
    ("title_settings_shortcut", "Settings"),
    (
        "title_remove_project_sidebar",
        "Remove project from sidebar",
    ),
    ("title_quick_new_session", "Quick new Claude session"),
    ("title_session_actions", "Session actions"),
    ("title_drag_resize_sidebar", "Drag to resize sidebar"),
    ("settings_title", "Settings"),
    ("aria_close_settings", "Close settings"),
    ("nav_claude", "Claude"),
    ("nav_appearance", "Appearance"),
    ("nav_integrations", "Integrations"),
    ("section_language", "Language"),
    ("language_zh_cn", "中文"),
    ("language_en_us", "English"),
    ("section_theme_preset", "Theme preset"),
    ("theme_preset_default", "Default"),
    ("theme_preset_everforest", "Everforest"),
    (
        "hint_theme_preset",
        "Default keeps the current built-in look. Everforest still supports light / dark / system modes.",
    ),
    ("section_theme_mode", "Theme mode"),
    ("theme_light", "Light"),
    ("theme_dark", "Dark"),
    ("theme_system", "System"),
    (
        "hint_theme_palette_default",
        "The Default preset still reads its customizable palette values from preferences.json.",
    ),
    (
        "hint_theme_palette_builtin",
        "Everforest uses a built-in palette and does not read custom theme colors from preferences.json.",
    ),
    ("section_external_terminal", "External terminal"),
    ("section_external_editor", "External editor"),
    (
        "hint_integrations",
        "Session quick actions use these defaults for \"Open project in terminal\" and \"Open project in editor\". All values are stored in preferences.json.",
    ),
    ("section_claude_startup", "Claude startup arguments"),
    (
        "label_enable_custom_args",
        "Enable custom Claude startup arguments",
    ),
    (
        "hint_claude_startup",
        "When disabled, it uses default `claude --resume`. When enabled, the default argument is `--dangerously-skip-permissions`, and it applies to session resume and quick new session startup.",
    ),
    ("section_session_restore", "Session restore"),
    (
        "label_restore_last_session",
        "Restore last opened session on launch",
    ),
    (
        "hint_restore_last_session",
        "Automatically re-open the last active session after app start. Turn this off to use most recent-session fallback behavior.",
    ),
    ("button_done", "Done"),
    ("tree_no_sessions", "No sessions"),
    ("tree_no_projects", "No projects yet"),
    ("menu_edit_session_name", "Edit display name"),
    ("menu_open_project_terminal", "Open project in terminal"),
    ("menu_open_project_editor", "Open project in editor"),
    ("menu_check_for_updates", "Check for Updates…"),
    ("menu_stop", "Stop"),
    ("menu_delete", "Delete"),
    (
        "confirm_update_available",
        "Version {latest} is available (current: {current}). Download and open the installer now?",
    ),
    (
        "alert_update_install_guidance",
        "Downloaded and opened the installer for {version}. Drag the new app into Applications to finish the update.",
    ),
    ("update_dialog_title", "App Update"),
    ("update_dialog_title_checking", "Checking for Updates"),
    ("update_dialog_title_available", "Update Available"),
    ("update_dialog_title_downloading", "Downloading Update"),
    ("update_dialog_title_up_to_date", "You're Up to Date"),
    ("update_dialog_title_completed", "Installer Ready"),
    ("update_dialog_title_error", "Update Failed"),
    (
        "update_dialog_subtitle",
        "Review the release contents and start the download from here.",
    ),
    ("update_dialog_current_version", "Current Version"),
    ("update_dialog_latest_version", "Latest Version"),
    ("update_dialog_published_at", "Published"),
    ("update_dialog_status", "Status"),
    ("update_dialog_phase_idle", "Idle"),
    ("update_dialog_phase_checking", "Checking"),
    ("update_dialog_phase_available", "Available"),
    ("update_dialog_phase_downloading", "Downloading"),
    ("update_dialog_phase_up_to_date", "Up to date"),
    ("update_dialog_phase_completed", "Installer opened"),
    ("update_dialog_phase_error", "Failed"),
    ("update_dialog_error_label", "Error details"),
    ("update_dialog_install_ready", "The installer is ready"),
    (
        "update_dialog_install_guidance",
        "The installer for {version} has been downloaded and opened. Drag the new app into Applications to complete the update.",
    ),
    ("update_dialog_changelog", "Changelog"),
    ("update_dialog_commit", "Commit"),
    ("update_dialog_description", "Description"),
    ("update_dialog_release_notes", "Release Notes"),
    (
        "update_dialog_no_release_notes",
        "No extra release notes were provided for this version.",
    ),
    ("update_dialog_download", "Download & Open Installer"),
    ("update_dialog_open_github", "View on GitHub"),
    ("update_dialog_close", "Later"),
    ("update_dialog_working", "Working…"),
    ("aria_close_update_dialog", "Close update dialog"),
    ("show_less", "Show less"),
    ("show_more_count", "Show {count} more..."),
    (
        "workspace_placeholder",
        "Select a session to open Claude Code.",
    ),
    ("fallback_new_project", "New Project"),
    ("menu_new_terminal_session", "New Terminal"),
    ("menu_settings", "Settings…"),
    ("menu_open_config_file", "Open Config File"),
    ("menu_reload_config", "Reload Config"),
];

/// Translate `key` for `language`. Missing keys fall back to the key itself so
/// oversights are visible in the UI instead of silently blank.
pub fn t(language: AppLanguage, key: &str) -> &'static str {
    let dictionary: &'static [(&'static str, &'static str)] = match language {
        AppLanguage::ZhCn => ZH_CN,
        AppLanguage::EnUs => EN_US,
    };
    dictionary
        .iter()
        .find(|(entry_key, _)| *entry_key == key)
        .map(|(_, value)| *value)
        .unwrap_or("")
}

/// Translate with `{name}` interpolation.
pub fn tf(language: AppLanguage, key: &str, params: &[(&str, &str)]) -> String {
    let template = t(language, key);
    let mut result = template.to_string();
    for (name, value) in params {
        result = result.replace(&format!("{{{}}}", name), value);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_known_keys() {
        assert_eq!(t(AppLanguage::ZhCn, "button_done"), "完成");
        assert_eq!(t(AppLanguage::EnUs, "button_done"), "Done");
    }

    #[test]
    fn interpolates_params() {
        let message = tf(
            AppLanguage::ZhCn,
            "status_project_added",
            &[("name", "demo")],
        );
        assert_eq!(message, "已添加项目：demo");
    }

    #[test]
    fn falls_back_to_empty_for_missing_entries() {
        assert_eq!(t(AppLanguage::ZhCn, "no_such_key"), "");
    }

    #[test]
    fn dictionaries_have_matching_keys() {
        let zh: std::collections::HashSet<&str> = ZH_CN.iter().map(|(key, _)| *key).collect();
        let en: std::collections::HashSet<&str> = EN_US.iter().map(|(key, _)| *key).collect();
        for key in zh.iter() {
            assert!(en.contains(key), "en-US missing key: {}", key);
        }
        for key in en {
            assert!(zh.contains(key), "zh-CN missing key: {}", key);
        }
    }
}
