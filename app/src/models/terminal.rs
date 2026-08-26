use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TerminalApp {
    Terminal,
    ITerm2,
    WezTerm,
    Alacritty,
    Warp,
    Ghostty,
    Kitty,
    Tabby,
}

impl TerminalApp {
    pub fn display_name(&self) -> &str {
        match self {
            TerminalApp::Terminal => "Terminal",
            TerminalApp::ITerm2 => "iTerm2",
            TerminalApp::WezTerm => "WezTerm",
            TerminalApp::Alacritty => "Alacritty",
            TerminalApp::Warp => "Warp",
            TerminalApp::Ghostty => "Ghostty",
            TerminalApp::Kitty => "Kitty",
            TerminalApp::Tabby => "Tabby",
        }
    }

    #[allow(dead_code)]
    pub fn app_name(&self) -> &str {
        match self {
            TerminalApp::Terminal => "Terminal",
            TerminalApp::ITerm2 => "iTerm",
            TerminalApp::WezTerm => "WezTerm",
            TerminalApp::Alacritty => "Alacritty",
            TerminalApp::Warp => "Warp",
            TerminalApp::Ghostty => "Ghostty",
            TerminalApp::Kitty => "kitty",
            TerminalApp::Tabby => "Tabby",
        }
    }

    pub fn from_display_name(name: &str) -> Option<Self> {
        match name {
            "Terminal" => Some(TerminalApp::Terminal),
            "iTerm2" => Some(TerminalApp::ITerm2),
            "WezTerm" => Some(TerminalApp::WezTerm),
            "Alacritty" => Some(TerminalApp::Alacritty),
            "Warp" => Some(TerminalApp::Warp),
            "Ghostty" => Some(TerminalApp::Ghostty),
            "Kitty" => Some(TerminalApp::Kitty),
            "Tabby" => Some(TerminalApp::Tabby),
            _ => None,
        }
    }

    pub fn all() -> Vec<Self> {
        vec![
            TerminalApp::Terminal,
            TerminalApp::ITerm2,
            TerminalApp::WezTerm,
            TerminalApp::Alacritty,
            TerminalApp::Warp,
            TerminalApp::Ghostty,
            TerminalApp::Kitty,
            TerminalApp::Tabby,
        ]
    }
}
