//! External editor integration (open a project folder in the user's editor).

use std::path::Path;
use std::process::Command;

fn app_exists(paths: &[&str]) -> bool {
    paths.iter().any(|path| Path::new(path).exists())
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

pub fn detect_available_editors() -> Vec<String> {
    let mut editors = Vec::new();

    if app_exists(&["/Applications/Visual Studio Code.app"]) || command_exists("code") {
        editors.push("VSCode".to_string());
    }
    if app_exists(&["/Applications/Cursor.app"]) || command_exists("cursor") {
        editors.push("Cursor".to_string());
    }
    if app_exists(&["/Applications/Windsurf.app"]) || command_exists("windsurf") {
        editors.push("Windsurf".to_string());
    }
    if app_exists(&["/Applications/Zed.app"]) || command_exists("zed") {
        editors.push("Zed".to_string());
    }
    if app_exists(&["/Applications/Sublime Text.app"]) || command_exists("subl") {
        editors.push("Sublime Text".to_string());
    }

    if editors.is_empty() {
        editors.push("VSCode".to_string());
    }

    editors
}

pub fn open_project_in_editor(project_path: &str, editor_app: &str) -> Result<(), String> {
    let normalized_path = project_path.trim().to_string();
    if normalized_path.is_empty() {
        return Err("Project path cannot be empty".to_string());
    }
    if !Path::new(&normalized_path).is_dir() {
        return Err(format!(
            "Project path is not a directory: {}",
            normalized_path
        ));
    }

    let selected_editor = editor_app.trim();
    if selected_editor.is_empty() {
        return Err("Editor cannot be empty".to_string());
    }

    let editor = selected_editor.to_string();
    let (command_name, cli, app_name): (&str, &str, &str) = match editor.as_str() {
        "VSCode" => ("code", "code", "Visual Studio Code"),
        "Cursor" => ("cursor", "cursor", "Cursor"),
        "Windsurf" => ("windsurf", "windsurf", "Windsurf"),
        "Zed" => ("zed", "zed", "Zed"),
        "Sublime Text" => ("subl", "subl", "Sublime Text"),
        _ => return Err(format!("Unknown editor: {}", selected_editor)),
    };
    let _ = command_name;

    let result = if command_exists(cli) {
        Command::new(cli).arg(&normalized_path).spawn()
    } else {
        Command::new("open")
            .args(["-a", app_name, &normalized_path])
            .spawn()
    };

    result.map_err(|error| format!("Failed to open project in {}: {}", selected_editor, error))?;
    Ok(())
}
