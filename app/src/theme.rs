//! Theme system: resolves the active palette from preferences.json (custom
//! "default" preset) or the built-in Everforest preset, and converts the
//! stored CSS-style color strings into GPUI colors.

use gpui::{Hsla, WindowAppearance};

use crate::models::app_settings::ThemePalette;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemePreference {
    Light,
    Dark,
    System,
}

impl ThemePreference {
    pub fn from_str(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "dark" => Self::Dark,
            "system" => Self::System,
            _ => Self::Light,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
            Self::System => "system",
        }
    }

    pub fn resolves_to_dark(self, system_appearance: WindowAppearance) -> bool {
        match self {
            Self::System => matches!(
                system_appearance,
                WindowAppearance::Dark | WindowAppearance::VibrantDark
            ),
            Self::Light => false,
            Self::Dark => true,
        }
    }
}

/// Runtime theme: the palette fields the UI actually paints with, already
/// converted to GPUI colors.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Theme {
    pub is_dark: bool,
    pub app_bg: Hsla,
    pub panel_bg: Hsla,
    pub border_color: Hsla,
    pub border_soft: Hsla,
    pub text_main: Hsla,
    pub text_sub: Hsla,
    pub text_soft: Hsla,
    pub hover_bg: Hsla,
    pub selected_bg: Hsla,
    pub selected_text: Hsla,
    pub button_bg: Hsla,
    pub button_hover: Hsla,
    pub button_text: Hsla,
    pub alert_bg: Hsla,
    pub alert_border: Hsla,
    pub alert_text: Hsla,
    pub accent: Hsla,
    pub terminal_background: Hsla,
    pub terminal_foreground: Hsla,
    pub terminal_cursor: Hsla,
    pub terminal_selection: Hsla,
}

impl Theme {
    pub fn from_palette(palette: &ThemePalette, is_dark: bool) -> Self {
        Self {
            is_dark,
            app_bg: parse_color(&palette.app_bg),
            panel_bg: parse_color(&palette.panel_bg),
            border_color: parse_color(&palette.border_color),
            border_soft: parse_color(&palette.border_soft),
            text_main: parse_color(&palette.text_main),
            text_sub: parse_color(&palette.text_sub),
            text_soft: parse_color(&palette.text_soft),
            hover_bg: parse_color(&palette.hover_bg),
            selected_bg: parse_color(&palette.selected_bg),
            selected_text: parse_color(&palette.selected_text),
            button_bg: parse_color(&palette.button_bg),
            button_hover: parse_color(&palette.button_hover),
            button_text: parse_color(&palette.button_text),
            alert_bg: parse_color(&palette.alert_bg),
            alert_border: parse_color(&palette.alert_border),
            alert_text: parse_color(&palette.alert_text),
            accent: parse_color(&palette.accent),
            terminal_background: parse_color(&palette.terminal_background),
            terminal_foreground: parse_color(&palette.terminal_foreground),
            terminal_cursor: parse_color(&palette.terminal_cursor),
            terminal_selection: parse_color(&palette.terminal_selection),
        }
    }
}

/// Parse the CSS-style color strings persisted in preferences.json:
/// `#rgb`, `#rrggbb` and `rgba(r, g, b, a)`.
pub fn parse_color(value: &str) -> Hsla {
    let trimmed = value.trim();

    if let Some(hex) = trimmed.strip_prefix('#') {
        let (r, g, b) = match hex.len() {
            3 => {
                let r = u8::from_str_radix(&hex[0..1].repeat(2), 16);
                let g = u8::from_str_radix(&hex[1..2].repeat(2), 16);
                let b = u8::from_str_radix(&hex[2..3].repeat(2), 16);
                match (r, g, b) {
                    (Ok(r), Ok(g), Ok(b)) => (r, g, b),
                    _ => return fallback_color(),
                }
            }
            6 => {
                let parse = |range: std::ops::Range<usize>| {
                    u8::from_str_radix(hex.get(range.clone()).unwrap_or_default(), 16)
                };
                match (parse(0..2), parse(2..4), parse(4..6)) {
                    (Ok(r), Ok(g), Ok(b)) => (r, g, b),
                    _ => return fallback_color(),
                }
            }
            _ => return fallback_color(),
        };
        return rgb_to_hsla(r, g, b, 1.0);
    }

    let inner = trimmed
        .trim_start_matches("rgba(")
        .trim_start_matches("rgb(")
        .trim_end_matches(')');
    let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
    if parts.len() >= 3 {
        let channel = |index: usize| -> Option<u8> {
            parts.get(index)?.parse::<u8>().ok().or_else(|| {
                parts
                    .get(index)?
                    .parse::<f32>()
                    .ok()
                    .map(|value| value.clamp(0.0, 255.0) as u8)
            })
        };
        let alpha = parts
            .get(3)
            .and_then(|value| value.parse::<f32>().ok())
            .unwrap_or(1.0);
        if let (Some(r), Some(g), Some(b)) = (channel(0), channel(1), channel(2)) {
            return rgb_to_hsla(r, g, b, alpha.clamp(0.0, 1.0));
        }
    }

    fallback_color()
}

fn fallback_color() -> Hsla {
    gpui::black()
}

pub fn rgb_to_hsla(r: u8, g: u8, b: u8, a: f32) -> Hsla {
    let rgba = gpui::rgb(u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b));
    let mut color: Hsla = rgba.into();
    color.a = a;
    color
}

/// The built-in Everforest palettes (mirrors the previous web frontend's
/// `EVERFOREST_THEME_PALETTES`).
pub fn everforest_palette(mode: ThemeMode) -> ThemePalette {
    let mut palette = ThemePalette::dark_defaults();
    let fields: [(&str, (&str, &str)); 22] = [
        ("app_bg", ("#2d353b", "#fefcf1")),
        ("panel_bg", ("#394349", "#fdf9ec")),
        (
            "border_color",
            ("rgba(211, 198, 170, 0.16)", "rgba(92, 106, 114, 0.16)"),
        ),
        (
            "border_soft",
            ("rgba(211, 198, 170, 0.24)", "rgba(92, 106, 114, 0.24)"),
        ),
        ("text_main", ("#d3c6aa", "#465662")),
        ("text_sub", ("#9da9a0", "#5c6a72")),
        ("text_soft", ("#859289", "#829181")),
        (
            "hover_bg",
            ("rgba(167, 192, 128, 0.08)", "rgba(92, 106, 114, 0.07)"),
        ),
        (
            "selected_bg",
            ("rgba(131, 192, 175, 0.14)", "rgba(141, 161, 1, 0.11)"),
        ),
        ("selected_text", ("#e6e1cf", "#465662")),
        (
            "button_bg",
            ("rgba(131, 192, 175, 0.12)", "rgba(141, 161, 1, 0.10)"),
        ),
        (
            "button_hover",
            ("rgba(131, 192, 175, 0.22)", "rgba(141, 161, 1, 0.18)"),
        ),
        ("button_text", ("#d3c6aa", "#465662")),
        ("alert_bg", ("#4a2f33", "#fbe3df")),
        ("alert_border", ("#e67e80", "#f85552")),
        ("alert_text", ("#f4d6d7", "#8b3532")),
        ("accent", ("#7fbbb3", "#8da101")),
        ("terminal_background", ("#2d353b", "#fefcf1")),
        ("terminal_foreground", ("#d3c6aa", "#465662")),
        ("terminal_cursor", ("#d3c6aa", "#465662")),
        ("terminal_selection", ("#475258", "#f0ede4")),
        (
            "terminal_scrollbar",
            ("rgba(133, 146, 137, 0.42)", "rgba(92, 106, 114, 0.30)"),
        ),
    ];
    for (field, (dark, light)) in fields {
        palette.set_field(field, if mode == ThemeMode::Dark { dark } else { light });
    }
    if mode == ThemeMode::Light {
        palette.terminal_scrollbar_hover = "rgba(92, 106, 114, 0.48)".to_string();
        palette.terminal_font_family = "Menlo".to_string();
    } else {
        palette.terminal_scrollbar_hover = "rgba(133, 146, 137, 0.60)".to_string();
    }
    palette
}

/// Resolve the palette actually in effect: Everforest is fully built-in and
/// ignores customized colors stored in preferences.json; the default preset
/// reads them.
pub fn resolve_palette(
    preset: &str,
    preference: ThemePreference,
    custom: &crate::models::app_settings::ThemePalettes,
    system_appearance: WindowAppearance,
) -> ThemePalette {
    let mode = if preference.resolves_to_dark(system_appearance) {
        ThemeMode::Dark
    } else {
        ThemeMode::Light
    };

    if preset.trim().eq_ignore_ascii_case("everforest") {
        everforest_palette(mode)
    } else {
        match mode {
            ThemeMode::Dark => custom.dark.clone(),
            ThemeMode::Light => custom.light.clone(),
        }
    }
}

/// GPUI global carrying the currently resolved theme so every view (terminal
/// included) can read the active palette without prop drilling.
pub struct CurrentTheme(pub Theme);

impl gpui::Global for CurrentTheme {}

impl CurrentTheme {
    pub fn get(cx: &gpui::App) -> Theme {
        cx.try_global::<CurrentTheme>()
            .map(|global| global.0)
            .unwrap_or_else(|| {
                Theme::from_palette(
                    &crate::models::app_settings::ThemePalette::dark_defaults(),
                    true,
                )
            })
    }

    /// Set during the dashboard render; only writes (and notifies observers)
    /// when the theme actually changed.
    pub fn set(theme: &Theme, cx: &mut gpui::App) {
        let changed = cx
            .try_global::<CurrentTheme>()
            .map(|global| global.0 != *theme)
            .unwrap_or(true);
        if changed {
            cx.set_global(CurrentTheme(*theme));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_colors() {
        let color = parse_color("#002b36");
        assert!((color.l - rgb_to_hsla(0x00, 0x2b, 0x36, 1.0).l).abs() < 1e-6);

        let short = parse_color("#fff");
        assert!((short.l - 1.0).abs() < 1e-6);
    }

    #[test]
    fn parses_rgba_colors() {
        let color = parse_color("rgba(147, 161, 161, 0.20)");
        let expected = rgb_to_hsla(147, 161, 161, 0.20);
        assert!((color.l - expected.l).abs() < 1e-6);
        assert!((color.a - 0.20).abs() < 1e-6);
    }

    #[test]
    fn falls_back_on_garbage() {
        let color = parse_color("not-a-color");
        assert_eq!(color, gpui::black());
    }

    #[test]
    fn resolves_theme_mode_from_preference() {
        assert!(ThemePreference::Dark.resolves_to_dark(WindowAppearance::Light));
        assert!(!ThemePreference::Light.resolves_to_dark(WindowAppearance::Dark));
        assert!(ThemePreference::System.resolves_to_dark(WindowAppearance::Dark));
        assert!(!ThemePreference::System.resolves_to_dark(WindowAppearance::Light));
    }
}
