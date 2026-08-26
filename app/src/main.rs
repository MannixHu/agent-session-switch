#![recursion_limit = "256"]

mod app;
mod i18n;

pub use theme::CurrentTheme;
mod models;
mod services;
mod terminal;
mod theme;
mod ui;
mod utils;

use std::borrow::Cow;

use gpui::{
    App, AppContext, Bounds, KeyBinding, Menu, MenuItem, TitlebarOptions, WindowBounds,
    WindowOptions, actions, px, size,
};

use crate::app::Dashboard;
use crate::i18n::{AppLanguage, t};
use crate::models::app_settings::AppSettings;
use crate::services::settings_service::SettingsService;

actions!(
    claude_session_switch,
    [
        Quit,
        OpenSettings,
        OpenConfigFile,
        ReloadConfig,
        CheckForUpdates,
        ToggleSidebar,
        NewTerminal,
        NewClaudeSession,
        NewCodexSession,
        NewOhmPiSession
    ]
);

const APP_NAME: &str = "Agent Session Switch";
const MIN_WINDOW_WIDTH: f32 = 720.0;
const MIN_WINDOW_HEIGHT: f32 = 460.0;

/// Rebuild the native menu bar in the active locale (GPUI menus own their
/// labels, so a language change must replace them).
pub fn refresh_menus(cx: &mut App, settings: &AppSettings) {
    let language = AppLanguage::from_str(&settings.appearance.language);
    let main_menu = Menu {
        name: APP_NAME.into(),
        disabled: false,
        items: vec![
            MenuItem::action(t(language, "menu_settings"), OpenSettings),
            MenuItem::action(t(language, "menu_open_config_file"), OpenConfigFile),
            MenuItem::action(t(language, "menu_reload_config"), ReloadConfig),
            MenuItem::action(t(language, "menu_check_for_updates"), CheckForUpdates),
            MenuItem::separator(),
            MenuItem::action("Quit Agent Session Switch", Quit),
        ],
    };
    let file_menu = Menu {
        name: "File".into(),
        disabled: false,
        items: vec![
            MenuItem::action(t(language, "menu_new_terminal_session"), NewTerminal),
            MenuItem::action(t(language, "title_quick_new_session"), NewClaudeSession),
            MenuItem::action(t(language, "menu_new_codex_session"), NewCodexSession),
            MenuItem::action(t(language, "menu_new_omp_session"), NewOhmPiSession),
        ],
    };
    let view_menu = Menu {
        name: "View".into(),
        disabled: false,
        items: vec![MenuItem::action(
            t(
                language,
                if settings.ui.sidebar_collapsed {
                    "title_show_sidebar"
                } else {
                    "title_hide_sidebar"
                },
            ),
            ToggleSidebar,
        )],
    };
    cx.set_menus(vec![main_menu, file_menu, view_menu]);
}

fn register_fonts(cx: &App) {
    let fonts: Vec<Cow<'static, [u8]>> =
        FONT_FILES.iter().map(|font| Cow::Borrowed(*font)).collect();
    if let Err(error) = cx.text_system().add_fonts(fonts) {
        eprintln!("failed to register bundled fonts: {error:?}");
    }
}
static FONT_FILES: &[&[u8]] = &[
    include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-Italic.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-BoldItalic.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-Medium.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-ExtraBold.ttf"),
];

fn main() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .try_init();

    gpui_platform::application().run(|cx: &mut App| {
        register_fonts(cx);

        cx.bind_keys([
            KeyBinding::new("secondary-q", Quit, None),
            KeyBinding::new("secondary-,", OpenSettings, None),
            KeyBinding::new("secondary-b", ToggleSidebar, None),
            KeyBinding::new("secondary-t", NewTerminal, None),
            KeyBinding::new("secondary-n", NewClaudeSession, None),
        ]);

        // Initial window size from persisted settings.
        let settings = SettingsService::new().get_settings().unwrap_or_default();
        let width = settings.ui.window.width.max(MIN_WINDOW_WIDTH as u32) as f32;
        let height = settings.ui.window.height.max(MIN_WINDOW_HEIGHT as u32) as f32;
        refresh_menus(cx, &settings);

        let window = cx
            .open_window(
                WindowOptions {
                    titlebar: Some(TitlebarOptions {
                        title: Some(APP_NAME.into()),
                        ..Default::default()
                    }),
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(width), px(height)),
                        cx,
                    ))),
                    window_min_size: Some(size(px(MIN_WINDOW_WIDTH), px(MIN_WINDOW_HEIGHT))),
                    ..Default::default()
                },
                |window, cx| cx.new(|cx| Dashboard::new(window, cx)),
            )
            .expect("failed to open window");

        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.on_action({
            move |_: &OpenSettings, cx| {
                let _ = window.update(cx, |dashboard, _, cx| dashboard.open_settings(cx));
            }
        });
        cx.on_action({
            move |_: &OpenConfigFile, cx| {
                let _ = window.update(cx, |dashboard, _, cx| {
                    dashboard.open_config_file(cx);
                });
            }
        });
        cx.on_action({
            move |_: &ReloadConfig, cx| {
                let _ = window.update(cx, |dashboard, _, cx| dashboard.reload_config(cx));
            }
        });
        cx.on_action({
            move |_: &CheckForUpdates, cx| {
                let _ = window.update(cx, |dashboard, _, cx| {
                    dashboard.open_update_dialog(cx);
                });
            }
        });
        cx.on_action({
            move |_: &ToggleSidebar, cx| {
                let _ = window.update(cx, |dashboard, _, cx| {
                    dashboard.toggle_sidebar_action(cx);
                });
            }
        });
        cx.on_action({
            move |_: &NewTerminal, cx| {
                let _ = window.update(cx, |dashboard, window, cx| {
                    dashboard.new_terminal_action(window, cx);
                });
            }
        });
        cx.on_action({
            move |_: &NewClaudeSession, cx| {
                let _ = window.update(cx, |dashboard, window, cx| {
                    dashboard.new_agent_session_action(
                        crate::models::agent::AgentKind::Claude,
                        window,
                        cx,
                    );
                });
            }
        });
    });
}
