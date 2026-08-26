use std::path::Path;
use std::process::Command;

use crate::models::terminal::TerminalApp;

pub fn detect_available_terminals() -> Vec<TerminalApp> {
    let detected: Vec<TerminalApp> = TerminalApp::all()
        .into_iter()
        .filter(is_terminal_installed)
        .collect();

    if detected.is_empty() {
        vec![TerminalApp::Terminal]
    } else {
        detected
    }
}

pub fn is_terminal_installed(terminal: &TerminalApp) -> bool {
    match terminal {
        TerminalApp::Terminal => app_exists(&["/Applications/Utilities/Terminal.app"]),
        TerminalApp::ITerm2 => {
            app_exists(&["/Applications/iTerm.app", "/Applications/iTerm2.app"])
                || command_exists("iterm2")
        }
        TerminalApp::WezTerm => {
            app_exists(&["/Applications/WezTerm.app"]) || command_exists("wezterm")
        }
        TerminalApp::Alacritty => {
            app_exists(&["/Applications/Alacritty.app"]) || command_exists("alacritty")
        }
        TerminalApp::Warp => app_exists(&["/Applications/Warp.app"]) || command_exists("warp"),
        TerminalApp::Ghostty => {
            app_exists(&["/Applications/Ghostty.app"]) || command_exists("ghostty")
        }
        TerminalApp::Kitty => {
            app_exists(&["/Applications/kitty.app", "/Applications/Kitty.app"])
                || command_exists("kitty")
        }
        TerminalApp::Tabby => app_exists(&["/Applications/Tabby.app"]) || command_exists("tabby"),
    }
}

pub fn open_terminal_with_path(terminal: TerminalApp, path: &str) -> Result<(), String> {
    match terminal {
        TerminalApp::Terminal => open_app_with_path("Terminal", path),
        TerminalApp::ITerm2 => {
            let script = format!(
                r#"tell application "iTerm"
    activate
    create window with default profile
    tell current window
        tell current session
            write text "cd \"{}\""
        end tell
    end tell
end tell"#,
                path.replace("\"", "\\\"")
            );

            Command::new("osascript")
                .arg("-e")
                .arg(&script)
                .spawn()
                .map_err(|error| format!("Failed to open iTerm2: {}", error))?;
            Ok(())
        }
        TerminalApp::WezTerm => {
            if let Some(wezterm_path) = find_wezterm_path() {
                Command::new(&wezterm_path)
                    .arg("start")
                    .arg("--cwd")
                    .arg(path)
                    .spawn()
                    .map_err(|error| format!("Failed to open WezTerm: {}", error))?;
                Ok(())
            } else {
                open_app_with_path("WezTerm", path)
            }
        }
        TerminalApp::Alacritty => {
            if command_exists("alacritty") {
                Command::new("alacritty")
                    .arg("--working-directory")
                    .arg(path)
                    .spawn()
                    .map_err(|error| format!("Failed to open Alacritty: {}", error))?;
                Ok(())
            } else {
                open_app_with_path("Alacritty", path)
            }
        }
        TerminalApp::Warp => open_app_with_path("Warp", path),
        TerminalApp::Ghostty => {
            if command_exists("ghostty") {
                Command::new("ghostty")
                    .arg("--working-directory")
                    .arg(path)
                    .spawn()
                    .map_err(|error| format!("Failed to open Ghostty: {}", error))?;
                Ok(())
            } else {
                open_app_with_path("Ghostty", path)
            }
        }
        TerminalApp::Kitty => {
            if let Some(kitty_path) = find_kitty_path() {
                Command::new(&kitty_path)
                    .arg("--directory")
                    .arg(path)
                    .spawn()
                    .map_err(|error| format!("Failed to open Kitty: {}", error))?;
                Ok(())
            } else {
                open_app_with_path("kitty", path)
            }
        }
        TerminalApp::Tabby => open_app_with_path("Tabby", path),
    }
}

fn open_app_with_path(app_name: &str, path: &str) -> Result<(), String> {
    Command::new("open")
        .arg("-a")
        .arg(app_name)
        .arg(path)
        .spawn()
        .map_err(|error| format!("Failed to open {}: {}", app_name, error))?;
    Ok(())
}

fn command_exists(command: &str) -> bool {
    let normalized = command.trim();
    if normalized.is_empty() || normalized.contains(char::is_whitespace) {
        return false;
    }

    Command::new("which")
        .arg(normalized)
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn app_exists(paths: &[&str]) -> bool {
    paths.iter().any(|path| Path::new(path).exists())
}

fn find_in_standard_paths(paths: &[&str], command_name: &str) -> Option<String> {
    for path in paths {
        if Path::new(path).exists() {
            return Some(path.to_string());
        }
    }

    if let Ok(output) = Command::new("which").arg(command_name).output() {
        if output.status.success() {
            if let Ok(path) = String::from_utf8(output.stdout) {
                return Some(path.trim().to_string());
            }
        }
    }

    None
}

fn find_wezterm_path() -> Option<String> {
    find_in_standard_paths(
        &[
            "/usr/local/bin/wezterm",
            "/opt/homebrew/bin/wezterm",
            "/usr/bin/wezterm",
        ],
        "wezterm",
    )
}

fn find_kitty_path() -> Option<String> {
    find_in_standard_paths(
        &[
            "/usr/local/bin/kitty",
            "/opt/homebrew/bin/kitty",
            "/usr/bin/kitty",
        ],
        "kitty",
    )
}
