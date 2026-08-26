use std::collections::HashSet;
use std::sync::Mutex;

use crate::models::app_settings::{AppSettings, LastOpenedSession, ThemePalette};
use crate::services::storage_service::StorageService;

const SETTINGS_VERSION: u32 = 11;
const DEFAULT_CUSTOM_STARTUP_ARGS: &str = "--dangerously-skip-permissions";

pub struct SettingsService {
    settings: Mutex<AppSettings>,
}

impl SettingsService {
    pub fn new() -> Self {
        let settings = StorageService::read::<AppSettings>(&StorageService::preferences_file())
            .map(Self::normalize)
            .unwrap_or_else(|_| AppSettings::default());

        if let Err(error) = StorageService::write(&StorageService::preferences_file(), &settings) {
            log::warn!(
                "Failed to persist normalized settings at startup: {}",
                error
            );
        }

        Self {
            settings: Mutex::new(settings),
        }
    }

    pub fn get_settings(&self) -> Result<AppSettings, String> {
        let settings = self.settings.lock().map_err(|e| e.to_string())?;
        Ok(settings.clone())
    }

    pub fn set_settings(&self, settings: AppSettings) -> Result<AppSettings, String> {
        let mut guard = self.settings.lock().map_err(|e| e.to_string())?;
        *guard = Self::normalize(settings);

        StorageService::write(&StorageService::preferences_file(), &*guard)
            .map_err(|e| format!("Failed to save settings: {}", e))?;

        Ok(guard.clone())
    }

    fn normalize_non_empty(value: &str, fallback: &str) -> String {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            fallback.to_string()
        } else {
            trimmed.to_string()
        }
    }

    fn normalize_scrollbar_width(value: u32, fallback: u32) -> u32 {
        let normalized = if value == 0 { fallback } else { value };

        match normalized {
            4 | 5 | 6 | 8 => normalized,
            _ => fallback,
        }
    }

    fn normalize_palette(palette: &mut ThemePalette, defaults: &ThemePalette) {
        palette.app_bg = Self::normalize_non_empty(&palette.app_bg, &defaults.app_bg);
        palette.panel_bg = Self::normalize_non_empty(&palette.panel_bg, &defaults.panel_bg);
        palette.border_color =
            Self::normalize_non_empty(&palette.border_color, &defaults.border_color);
        palette.border_soft =
            Self::normalize_non_empty(&palette.border_soft, &defaults.border_soft);
        palette.text_main = Self::normalize_non_empty(&palette.text_main, &defaults.text_main);
        palette.text_sub = Self::normalize_non_empty(&palette.text_sub, &defaults.text_sub);
        palette.text_soft = Self::normalize_non_empty(&palette.text_soft, &defaults.text_soft);
        palette.hover_bg = Self::normalize_non_empty(&palette.hover_bg, &defaults.hover_bg);
        palette.selected_bg =
            Self::normalize_non_empty(&palette.selected_bg, &defaults.selected_bg);
        palette.selected_text =
            Self::normalize_non_empty(&palette.selected_text, &defaults.selected_text);
        palette.button_bg = Self::normalize_non_empty(&palette.button_bg, &defaults.button_bg);
        palette.button_hover =
            Self::normalize_non_empty(&palette.button_hover, &defaults.button_hover);
        palette.button_text =
            Self::normalize_non_empty(&palette.button_text, &defaults.button_text);
        palette.alert_bg = Self::normalize_non_empty(&palette.alert_bg, &defaults.alert_bg);
        palette.alert_border =
            Self::normalize_non_empty(&palette.alert_border, &defaults.alert_border);
        palette.alert_text = Self::normalize_non_empty(&palette.alert_text, &defaults.alert_text);
        palette.accent = Self::normalize_non_empty(&palette.accent, &defaults.accent);
        palette.terminal_background =
            Self::normalize_non_empty(&palette.terminal_background, &defaults.terminal_background);
        palette.terminal_foreground =
            Self::normalize_non_empty(&palette.terminal_foreground, &defaults.terminal_foreground);
        palette.terminal_cursor =
            Self::normalize_non_empty(&palette.terminal_cursor, &defaults.terminal_cursor);
        palette.terminal_selection =
            Self::normalize_non_empty(&palette.terminal_selection, &defaults.terminal_selection);
        palette.terminal_scrollbar =
            Self::normalize_non_empty(&palette.terminal_scrollbar, &defaults.terminal_scrollbar);
        palette.terminal_scrollbar_hover = Self::normalize_non_empty(
            &palette.terminal_scrollbar_hover,
            &defaults.terminal_scrollbar_hover,
        );
        palette.terminal_font_family = Self::normalize_non_empty(
            &palette.terminal_font_family,
            &defaults.terminal_font_family,
        );
        palette.terminal_scrollbar_width = Self::normalize_scrollbar_width(
            palette.terminal_scrollbar_width,
            defaults.terminal_scrollbar_width,
        );
    }

    fn palette_matches(palette: &ThemePalette, other: &ThemePalette) -> bool {
        palette.app_bg == other.app_bg
            && palette.panel_bg == other.panel_bg
            && palette.border_color == other.border_color
            && palette.border_soft == other.border_soft
            && palette.text_main == other.text_main
            && palette.text_sub == other.text_sub
            && palette.text_soft == other.text_soft
            && palette.hover_bg == other.hover_bg
            && palette.selected_bg == other.selected_bg
            && palette.selected_text == other.selected_text
            && palette.button_bg == other.button_bg
            && palette.button_hover == other.button_hover
            && palette.button_text == other.button_text
            && palette.alert_bg == other.alert_bg
            && palette.alert_border == other.alert_border
            && palette.alert_text == other.alert_text
            && palette.accent == other.accent
            && palette.terminal_background == other.terminal_background
            && palette.terminal_foreground == other.terminal_foreground
            && palette.terminal_cursor == other.terminal_cursor
            && palette.terminal_selection == other.terminal_selection
            && palette.terminal_scrollbar == other.terminal_scrollbar
            && palette.terminal_scrollbar_hover == other.terminal_scrollbar_hover
            && palette.terminal_font_family == other.terminal_font_family
            && palette.terminal_scrollbar_width == other.terminal_scrollbar_width
    }

    fn legacy_light_defaults_v3() -> ThemePalette {
        ThemePalette {
            app_bg: "#fefcf1".to_string(),
            panel_bg: "#fbf8ec".to_string(),
            border_color: "rgba(70, 86, 98, 0.12)".to_string(),
            border_soft: "rgba(70, 86, 98, 0.16)".to_string(),
            text_main: "#465662".to_string(),
            text_sub: "#6a7a86".to_string(),
            text_soft: "#5f707c".to_string(),
            hover_bg: "rgba(70, 86, 98, 0.08)".to_string(),
            selected_bg: "rgba(70, 86, 98, 0.07)".to_string(),
            selected_text: "#465662".to_string(),
            button_bg: "rgba(70, 86, 98, 0.08)".to_string(),
            button_hover: "rgba(70, 86, 98, 0.12)".to_string(),
            button_text: "#5f707c".to_string(),
            alert_bg: "#fee2e2".to_string(),
            alert_border: "#fca5a5".to_string(),
            alert_text: "#7f1d1d".to_string(),
            accent: "#3a94c5".to_string(),
            terminal_background: "#fefcf1".to_string(),
            terminal_foreground: "#465662".to_string(),
            terminal_cursor: "#465662".to_string(),
            terminal_selection: "#f0ede4".to_string(),
            terminal_scrollbar: "rgba(70, 86, 98, 0.42)".to_string(),
            terminal_scrollbar_hover: "rgba(70, 86, 98, 0.58)".to_string(),
            terminal_font_family: String::new(),
            terminal_scrollbar_width: 0,
        }
    }

    fn legacy_dark_defaults_v7() -> ThemePalette {
        ThemePalette {
            app_bg: "#0f172a".to_string(),
            panel_bg: "#121d31".to_string(),
            border_color: "rgba(255, 255, 255, 0.06)".to_string(),
            border_soft: "rgba(255, 255, 255, 0.08)".to_string(),
            text_main: "#e2e8f0".to_string(),
            text_sub: "#64748b".to_string(),
            text_soft: "#94a3b8".to_string(),
            hover_bg: "rgba(255, 255, 255, 0.05)".to_string(),
            selected_bg: "rgba(255, 255, 255, 0.045)".to_string(),
            selected_text: "#e2e8f0".to_string(),
            button_bg: "rgba(255, 255, 255, 0.06)".to_string(),
            button_hover: "rgba(255, 255, 255, 0.1)".to_string(),
            button_text: "#94a3b8".to_string(),
            alert_bg: "#7f1d1d".to_string(),
            alert_border: "#b91c1c".to_string(),
            alert_text: "#fecaca".to_string(),
            accent: "#3b82f6".to_string(),
            terminal_background: "#0f172a".to_string(),
            terminal_foreground: "#e2e8f0".to_string(),
            terminal_cursor: "#e2e8f0".to_string(),
            terminal_selection: "#334155".to_string(),
            terminal_scrollbar: "rgba(100, 116, 139, 0.48)".to_string(),
            terminal_scrollbar_hover: "rgba(100, 116, 139, 0.66)".to_string(),
            terminal_font_family: String::new(),
            terminal_scrollbar_width: 0,
        }
    }

    fn legacy_light_defaults_v7() -> ThemePalette {
        ThemePalette {
            app_bg: "#fffef8".to_string(),
            panel_bg: "#fcfaf4".to_string(),
            border_color: "rgba(70, 86, 98, 0.10)".to_string(),
            border_soft: "rgba(70, 86, 98, 0.14)".to_string(),
            text_main: "#4d5c68".to_string(),
            text_sub: "#70808b".to_string(),
            text_soft: "#667784".to_string(),
            hover_bg: "rgba(70, 86, 98, 0.07)".to_string(),
            selected_bg: "rgba(70, 86, 98, 0.06)".to_string(),
            selected_text: "#4d5c68".to_string(),
            button_bg: "rgba(70, 86, 98, 0.07)".to_string(),
            button_hover: "rgba(70, 86, 98, 0.10)".to_string(),
            button_text: "#667784".to_string(),
            alert_bg: "#fee2e2".to_string(),
            alert_border: "#fca5a5".to_string(),
            alert_text: "#7f1d1d".to_string(),
            accent: "#4aa3cf".to_string(),
            terminal_background: "#fffef8".to_string(),
            terminal_foreground: "#4d5c68".to_string(),
            terminal_cursor: "#4d5c68".to_string(),
            terminal_selection: "#f2efe7".to_string(),
            terminal_scrollbar: "rgba(70, 86, 98, 0.36)".to_string(),
            terminal_scrollbar_hover: "rgba(70, 86, 98, 0.52)".to_string(),
            terminal_font_family: String::new(),
            terminal_scrollbar_width: 0,
        }
    }

    fn legacy_light_defaults_v8() -> ThemePalette {
        ThemePalette {
            app_bg: "#f5f3ee".to_string(),
            panel_bg: "#f2efe8".to_string(),
            border_color: "rgba(92, 106, 114, 0.14)".to_string(),
            border_soft: "rgba(92, 106, 114, 0.20)".to_string(),
            text_main: "#5c6a72".to_string(),
            text_sub: "#708089".to_string(),
            text_soft: "#66757d".to_string(),
            hover_bg: "rgba(92, 106, 114, 0.08)".to_string(),
            selected_bg: "rgba(92, 106, 114, 0.07)".to_string(),
            selected_text: "#5c6a72".to_string(),
            button_bg: "rgba(92, 106, 114, 0.09)".to_string(),
            button_hover: "rgba(92, 106, 114, 0.13)".to_string(),
            button_text: "#66757d".to_string(),
            alert_bg: "#fee2e2".to_string(),
            alert_border: "#fca5a5".to_string(),
            alert_text: "#7f1d1d".to_string(),
            accent: "#7fbbb3".to_string(),
            terminal_background: "#efebd4".to_string(),
            terminal_foreground: "#5c6a72".to_string(),
            terminal_cursor: "#f57d26".to_string(),
            terminal_selection: "#eaedc8".to_string(),
            terminal_scrollbar: "rgba(92, 106, 114, 0.40)".to_string(),
            terminal_scrollbar_hover: "rgba(92, 106, 114, 0.56)".to_string(),
            terminal_font_family: String::new(),
            terminal_scrollbar_width: 0,
        }
    }

    fn legacy_light_defaults_v9() -> ThemePalette {
        ThemePalette {
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
            terminal_background: "#f6f6f7".to_string(),
            terminal_foreground: "#4f5a63".to_string(),
            terminal_cursor: "#4f5a63".to_string(),
            terminal_selection: "#e9eaed".to_string(),
            terminal_scrollbar: "rgba(88, 96, 105, 0.34)".to_string(),
            terminal_scrollbar_hover: "rgba(88, 96, 105, 0.52)".to_string(),
            terminal_font_family: String::new(),
            terminal_scrollbar_width: 0,
        }
    }

    fn normalize(mut settings: AppSettings) -> AppSettings {
        let previous_version = settings.version;
        settings.version = SETTINGS_VERSION;

        settings.appearance.theme_preference = match settings
            .appearance
            .theme_preference
            .trim()
            .to_lowercase()
            .as_str()
        {
            "light" => "light".to_string(),
            "dark" => "dark".to_string(),
            _ => "system".to_string(),
        };

        settings.appearance.theme_preset = match settings
            .appearance
            .theme_preset
            .trim()
            .to_lowercase()
            .as_str()
        {
            "everforest" => "everforest".to_string(),
            _ => "default".to_string(),
        };

        settings.appearance.language = match settings.appearance.language.trim() {
            "en-US" => "en-US".to_string(),
            _ => "zh-CN".to_string(),
        };

        let dark_defaults = ThemePalette::dark_defaults();
        let light_defaults = ThemePalette::light_defaults();

        if previous_version < SETTINGS_VERSION {
            let legacy_light_defaults_v3 = Self::legacy_light_defaults_v3();
            let legacy_light_defaults_v7 = Self::legacy_light_defaults_v7();
            let legacy_light_defaults_v8 = Self::legacy_light_defaults_v8();
            let legacy_light_defaults_v9 = Self::legacy_light_defaults_v9();
            let legacy_dark_defaults_v7 = Self::legacy_dark_defaults_v7();

            if Self::palette_matches(
                &settings.appearance.theme_palettes.light,
                &legacy_light_defaults_v3,
            ) || Self::palette_matches(
                &settings.appearance.theme_palettes.light,
                &legacy_light_defaults_v7,
            ) || Self::palette_matches(
                &settings.appearance.theme_palettes.light,
                &legacy_light_defaults_v8,
            ) || Self::palette_matches(
                &settings.appearance.theme_palettes.light,
                &legacy_light_defaults_v9,
            ) {
                settings.appearance.theme_palettes.light = light_defaults.clone();
            }

            if Self::palette_matches(
                &settings.appearance.theme_palettes.dark,
                &legacy_dark_defaults_v7,
            ) {
                settings.appearance.theme_palettes.dark = dark_defaults.clone();
            }
        }

        Self::normalize_palette(&mut settings.appearance.theme_palettes.dark, &dark_defaults);
        Self::normalize_palette(
            &mut settings.appearance.theme_palettes.light,
            &light_defaults,
        );

        if settings.claude.custom_startup_args.trim().is_empty() {
            settings.claude.custom_startup_args = DEFAULT_CUSTOM_STARTUP_ARGS.to_string();
        }

        settings.integrations.default_external_terminal =
            Self::normalize_non_empty(&settings.integrations.default_external_terminal, "Terminal");
        settings.integrations.default_external_editor =
            Self::normalize_non_empty(&settings.integrations.default_external_editor, "VSCode");

        settings.ui.layout.sidebar_width = settings.ui.layout.sidebar_width.clamp(200, 600);
        settings.ui.layout.main_width = settings.ui.layout.main_width.clamp(300, 1000);
        settings.ui.layout.terminal_height = settings.ui.layout.terminal_height.clamp(100, 800);
        settings.ui.window.width = settings.ui.window.width.clamp(860, 5200);
        settings.ui.window.height = settings.ui.window.height.clamp(560, 3200);

        settings
            .ui
            .project_tree
            .expanded_projects
            .retain(|key, _| !key.trim().is_empty());
        settings
            .ui
            .project_tree
            .show_all_sessions
            .retain(|key, _| !key.trim().is_empty());

        let mut seen_project_ids = HashSet::new();
        settings.ui.project_tree.project_order = settings
            .ui
            .project_tree
            .project_order
            .iter()
            .filter_map(|project_id| {
                let trimmed = project_id.trim();
                if trimmed.is_empty() {
                    return None;
                }

                let normalized = trimmed.to_string();
                if seen_project_ids.insert(normalized.clone()) {
                    Some(normalized)
                } else {
                    None
                }
            })
            .collect();

        settings.sessions.aliases = settings
            .sessions
            .aliases
            .into_iter()
            .filter_map(|(key, value)| {
                let trimmed_key = key.trim();
                let trimmed_value = value.trim();

                if trimmed_key.is_empty() || trimmed_value.is_empty() {
                    return None;
                }

                Some((trimmed_key.to_string(), trimmed_value.to_string()))
            })
            .collect();

        settings.sessions.hidden = settings
            .sessions
            .hidden
            .into_iter()
            .filter_map(|(key, value)| {
                let trimmed_key = key.trim();

                if trimmed_key.is_empty() || !value {
                    return None;
                }

                Some((trimmed_key.to_string(), true))
            })
            .collect();

        settings.sessions.last_opened = settings.sessions.last_opened.and_then(|last_opened| {
            let project_path = last_opened.project_path.trim();
            let session_id = last_opened.session_id.trim();

            if project_path.is_empty() || session_id.is_empty() {
                return None;
            }

            Some(LastOpenedSession {
                project_path: project_path.to_string(),
                session_id: session_id.to_string(),
            })
        });

        settings
    }
}

impl Default for SettingsService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::SettingsService;
    use crate::models::app_settings::AppSettings;

    #[test]
    fn normalize_adds_default_theme_preset_for_legacy_settings() {
        let legacy_settings: AppSettings = serde_json::from_value(serde_json::json!({
            "version": 10,
            "appearance": {
                "theme_preference": "light",
                "language": "zh-CN",
                "theme_palettes": {}
            },
            "claude": {},
            "integrations": {},
            "ui": {},
            "sessions": {}
        }))
        .expect("legacy settings should deserialize");

        let normalized = SettingsService::normalize(legacy_settings);
        let serialized = serde_json::to_value(&normalized).expect("settings should serialize");

        assert_eq!(serialized["appearance"]["theme_preset"], "default");
    }

    #[test]
    fn normalize_preserves_everforest_theme_preset() {
        let settings_with_everforest: AppSettings = serde_json::from_value(serde_json::json!({
            "version": 10,
            "appearance": {
                "theme_preference": "system",
                "theme_preset": "everforest",
                "language": "en-US",
                "theme_palettes": {}
            },
            "claude": {},
            "integrations": {},
            "ui": {},
            "sessions": {}
        }))
        .expect("settings should deserialize");

        let normalized = SettingsService::normalize(settings_with_everforest);
        let serialized = serde_json::to_value(&normalized).expect("settings should serialize");

        assert_eq!(serialized["appearance"]["theme_preset"], "everforest");
    }
}
