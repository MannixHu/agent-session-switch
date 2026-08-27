use std::collections::HashMap;

use serde::{Deserialize, Serialize};

fn default_version() -> u32 {
    11
}

fn default_theme_preference() -> String {
    "light".to_string()
}

fn default_theme_preset() -> String {
    "default".to_string()
}

fn default_language() -> String {
    "zh-CN".to_string()
}

fn default_custom_startup_args() -> String {
    "--dangerously-skip-permissions".to_string()
}

fn default_sidebar_width() -> u32 {
    300
}

fn default_main_width() -> u32 {
    500
}

fn default_terminal_height() -> u32 {
    300
}

fn default_window_width() -> u32 {
    1200
}

fn default_window_height() -> u32 {
    800
}

fn default_restore_last_opened_session() -> bool {
    true
}

fn default_terminal_font_family() -> String {
    r#""JetBrains Mono", "SF Pro Text", "SF Mono", "SFMono-Regular", "Consolas", "Menlo", "Monaco", "Courier New", "DejaVu Sans Mono", "Liberation Mono", "Noto Sans Mono", "Noto Sans Mono CJK SC", "Noto Sans Mono CJK JP", "PingFang SC", "Microsoft YaHei", "Hiragino Sans GB", "WenQuanYi Micro Hei", "Noto Color Emoji", "Segoe UI", "Ubuntu Mono", monospace"#
        .to_string()
}

fn default_terminal_scrollbar_width() -> u32 {
    6
}

fn default_external_terminal() -> String {
    "Terminal".to_string()
}

fn default_external_editor() -> String {
    "VSCode".to_string()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    #[serde(default = "default_version")]
    pub version: u32,
    pub appearance: AppearanceSettings,
    pub claude: ClaudeSettings,
    pub integrations: IntegrationSettings,
    pub ui: UiSettings,
    pub sessions: SessionSettings,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            version: default_version(),
            appearance: AppearanceSettings::default(),
            claude: ClaudeSettings::default(),
            integrations: IntegrationSettings::default(),
            ui: UiSettings::default(),
            sessions: SessionSettings::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppearanceSettings {
    #[serde(default = "default_theme_preference")]
    pub theme_preference: String,
    #[serde(default = "default_theme_preset")]
    pub theme_preset: String,
    #[serde(default = "default_language")]
    pub language: String,
    pub theme_palettes: ThemePalettes,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            theme_preference: default_theme_preference(),
            theme_preset: default_theme_preset(),
            language: default_language(),
            theme_palettes: ThemePalettes::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemePalettes {
    pub dark: ThemePalette,
    pub light: ThemePalette,
}

impl Default for ThemePalettes {
    fn default() -> Self {
        Self {
            dark: ThemePalette::dark_defaults(),
            light: ThemePalette::light_defaults(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemePalette {
    pub app_bg: String,
    pub panel_bg: String,
    pub border_color: String,
    pub border_soft: String,
    pub text_main: String,
    pub text_sub: String,
    pub text_soft: String,
    pub hover_bg: String,
    pub selected_bg: String,
    pub selected_text: String,
    pub button_bg: String,
    pub button_hover: String,
    pub button_text: String,
    pub alert_bg: String,
    pub alert_border: String,
    pub alert_text: String,
    pub accent: String,
    pub terminal_background: String,
    pub terminal_foreground: String,
    pub terminal_cursor: String,
    pub terminal_selection: String,
    pub terminal_scrollbar: String,
    pub terminal_scrollbar_hover: String,
    #[serde(default = "default_terminal_font_family")]
    pub terminal_font_family: String,
    #[serde(default = "default_terminal_scrollbar_width")]
    pub terminal_scrollbar_width: u32,
}

impl ThemePalette {
    pub fn dark_defaults() -> Self {
        Self {
            app_bg: "#002b36".to_string(),
            panel_bg: "#073642".to_string(),
            border_color: "rgba(147, 161, 161, 0.20)".to_string(),
            border_soft: "rgba(147, 161, 161, 0.28)".to_string(),
            text_main: "#93a1a1".to_string(),
            text_sub: "#657b83".to_string(),
            text_soft: "#839496".to_string(),
            hover_bg: "rgba(147, 161, 161, 0.10)".to_string(),
            selected_bg: "rgba(147, 161, 161, 0.08)".to_string(),
            selected_text: "#93a1a1".to_string(),
            button_bg: "rgba(147, 161, 161, 0.12)".to_string(),
            button_hover: "rgba(147, 161, 161, 0.20)".to_string(),
            button_text: "#93a1a1".to_string(),
            alert_bg: "#7f1d1d".to_string(),
            alert_border: "#b91c1c".to_string(),
            alert_text: "#fecaca".to_string(),
            accent: "#268bd2".to_string(),
            terminal_background: "#002b36".to_string(),
            terminal_foreground: "#839496".to_string(),
            terminal_cursor: "#93a1a1".to_string(),
            terminal_selection: "#073642".to_string(),
            terminal_scrollbar: "rgba(88, 110, 117, 0.48)".to_string(),
            terminal_scrollbar_hover: "rgba(88, 110, 117, 0.66)".to_string(),
            terminal_font_family: default_terminal_font_family(),
            terminal_scrollbar_width: default_terminal_scrollbar_width(),
        }
    }

    pub fn light_defaults() -> Self {
        Self {
            app_bg: "#f7f7f8".to_string(),
            panel_bg: "#f3f3f5".to_string(),
            border_color: "rgba(88, 96, 105, 0.14)".to_string(),
            border_soft: "rgba(88, 96, 105, 0.20)".to_string(),
            text_main: "#4d5560".to_string(),
            text_sub: "#7a838f".to_string(),
            text_soft: "#6d7580".to_string(),
            hover_bg: "rgba(88, 96, 105, 0.08)".to_string(),
            selected_bg: "rgba(88, 96, 105, 0.06)".to_string(),
            selected_text: "#4d5560".to_string(),
            button_bg: "rgba(88, 96, 105, 0.08)".to_string(),
            button_hover: "rgba(88, 96, 105, 0.13)".to_string(),
            button_text: "#6d7580".to_string(),
            alert_bg: "#fee2e2".to_string(),
            alert_border: "#fca5a5".to_string(),
            alert_text: "#7f1d1d".to_string(),
            accent: "#6b87d6".to_string(),
            terminal_background: "#fafafb".to_string(),
            terminal_foreground: "#4f5a63".to_string(),
            terminal_cursor: "#4f5a63".to_string(),
            terminal_selection: "#e9eaed".to_string(),
            terminal_scrollbar: "rgba(88, 96, 105, 0.34)".to_string(),
            terminal_scrollbar_hover: "rgba(88, 96, 105, 0.52)".to_string(),
            terminal_font_family: default_terminal_font_family(),
            terminal_scrollbar_width: default_terminal_scrollbar_width(),
        }
    }
}

impl Default for ThemePalette {
    fn default() -> Self {
        Self::dark_defaults()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClaudeSettings {
    #[serde(default)]
    pub use_custom_startup_args: bool,
    #[serde(default = "default_custom_startup_args")]
    pub custom_startup_args: String,
}

impl Default for ClaudeSettings {
    fn default() -> Self {
        Self {
            use_custom_startup_args: false,
            custom_startup_args: default_custom_startup_args(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct IntegrationSettings {
    #[serde(default = "default_external_terminal")]
    pub default_external_terminal: String,
    #[serde(default = "default_external_editor")]
    pub default_external_editor: String,
}

impl Default for IntegrationSettings {
    fn default() -> Self {
        Self {
            default_external_terminal: default_external_terminal(),
            default_external_editor: default_external_editor(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiSettings {
    #[serde(default)]
    pub sidebar_collapsed: bool,
    pub layout: LayoutSettings,
    pub window: WindowSettings,
    pub project_tree: ProjectTreeSettings,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LayoutSettings {
    #[serde(default = "default_sidebar_width")]
    pub sidebar_width: u32,
    #[serde(default = "default_main_width")]
    pub main_width: u32,
    #[serde(default = "default_terminal_height")]
    pub terminal_height: u32,
}

impl Default for LayoutSettings {
    fn default() -> Self {
        Self {
            sidebar_width: default_sidebar_width(),
            main_width: default_main_width(),
            terminal_height: default_terminal_height(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowSettings {
    #[serde(default = "default_window_width")]
    pub width: u32,
    #[serde(default = "default_window_height")]
    pub height: u32,
}

impl Default for WindowSettings {
    fn default() -> Self {
        Self {
            width: default_window_width(),
            height: default_window_height(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectTreeSettings {
    #[serde(default)]
    pub expanded_projects: HashMap<String, bool>,
    #[serde(default)]
    pub show_all_sessions: HashMap<String, bool>,
    #[serde(default)]
    pub project_order: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LastOpenedSession {
    /// App-session registry id (models::app_session::AppSession::id).
    pub app_session_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionSettings {
    #[serde(default = "default_restore_last_opened_session")]
    pub restore_last_opened_session: bool,
    #[serde(default)]
    pub last_opened: Option<LastOpenedSession>,
}

impl Default for SessionSettings {
    fn default() -> Self {
        Self {
            restore_last_opened_session: default_restore_last_opened_session(),
            last_opened: None,
        }
    }
}

impl ThemePalette {
    /// Sets one CSS-string color field by name. Used when composing built-in
    /// presets; unknown names are ignored.
    pub fn set_field(&mut self, field: &str, value: &str) {
        match field {
            "app_bg" => self.app_bg = value.to_string(),
            "panel_bg" => self.panel_bg = value.to_string(),
            "border_color" => self.border_color = value.to_string(),
            "border_soft" => self.border_soft = value.to_string(),
            "text_main" => self.text_main = value.to_string(),
            "text_sub" => self.text_sub = value.to_string(),
            "text_soft" => self.text_soft = value.to_string(),
            "hover_bg" => self.hover_bg = value.to_string(),
            "selected_bg" => self.selected_bg = value.to_string(),
            "selected_text" => self.selected_text = value.to_string(),
            "button_bg" => self.button_bg = value.to_string(),
            "button_hover" => self.button_hover = value.to_string(),
            "button_text" => self.button_text = value.to_string(),
            "alert_bg" => self.alert_bg = value.to_string(),
            "alert_border" => self.alert_border = value.to_string(),
            "alert_text" => self.alert_text = value.to_string(),
            "accent" => self.accent = value.to_string(),
            "terminal_background" => self.terminal_background = value.to_string(),
            "terminal_foreground" => self.terminal_foreground = value.to_string(),
            "terminal_cursor" => self.terminal_cursor = value.to_string(),
            "terminal_selection" => self.terminal_selection = value.to_string(),
            "terminal_scrollbar" => self.terminal_scrollbar = value.to_string(),
            _ => {}
        }
    }
}
