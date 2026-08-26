# Claude Session Switch

> A lightweight macOS desktop app for Claude Code / Claude CLI session switching.
> Built natively with **Rust + [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) + [alacritty_terminal](https://github.com/alacritty/alacritty/tree/master/alacritty_terminal)**, with project grouping, session resume, and an embedded terminal.

English (current) | [中文](./README.zh.md)

---

## Overview

`Claude Session Switch` is not trying to replace your CLI workflow.
It focuses on making session management visual, recoverable, and automation-friendly:

- Organize projects and sessions with a clear UI hierarchy
- Execute real work in embedded terminals running `claude --resume <session_id>`
- Keep behavior config-driven so both humans and AI can manage it

---

## Design philosophy

This app is not trying to become a heavier IDE.
It is designed as a **lighter, calmer, terminal-first** workspace for session-heavy workflows:

- **Immersive by default**: restrained UI with clear hierarchy, avoiding visual noise
- **Low interruption**: non-essential signals stay subtle so focus remains on active work
- **Terminal-centered**: CLI stays the execution core; GUI focuses on organization and switching
- **Session management first**: quickly locate projects, switch sessions, and restore context
- **Config-driven behavior**: settings live in files for personal tuning and AI-assisted automation

In short: **keep native CLI flow, remove session-management friction.**

---

## Feature highlights

### 1) Project and session management

- Sidebar with the project tree scanned from `~/.claude/projects` (plus manually added projects)
- Expand a project to list its Claude sessions (summary labels, modification time)
- Session rename (aliases plus `sessions-index.json`), stop, and delete actions
- Per-project quick actions: new Claude session, open in external terminal / editor, remove

### 2) Claude session resume

- Embedded terminal launch supports `claude --resume <session_id>` with graceful fallback (`|| claude`) to a fresh session in the same directory
- Configurable Claude startup args (optional default `--dangerously-skip-permissions`)
- Startup restore of the last opened session

### 3) Embedded terminal

- Powered by `alacritty_terminal` (the terminal core of Alacritty) rendered with GPUI
- Multi-tab terminals, output streaming, resize, scrollback (10k lines), cursor blink
- Selection + copy/paste (`Cmd+C` / `Cmd+V` / `Cmd+A`), bracketed paste
- `Cmd+Click` opens http(s) links detected in terminal output
- The embedded shell inherits the PATH of your login shell (mise / volta / homebrew shims included)

### 4) Config-driven (AI-friendly)

- Settings persisted to `preferences.json` (schema-compatible with the previous Tauri-based version)
- Theme/language/layout/window size/session restore are configurable
- `Open Config File` + `Reload Config` menu items for hot reload without restarting

### 5) Themes & languages

- Light / Dark / System theme modes with two built-in palettes: Default and Everforest
- Full English and Simplified Chinese UI (`zh-CN` / `en-US`)

### 6) macOS menu integration

App menu includes:

- `Settings…` (`Cmd+,`)
- `Open Config File` (open config in system default app)
- `Reload Config` (hot reload latest config into current UI)
- `Check for Updates…` (GitHub releases, SHA256-verified DMG download)
- `New Terminal` (`Cmd+T`), `Quick new Claude session` (`Cmd+N`), `Toggle Sidebar` (`Cmd+B`)

---

## Architecture

- **UI framework**: GPUI (Zed's GPU-accelerated Rust UI framework)
- **Terminal emulation**: alacritty_terminal + PTY via its own event loop
- **Data persistence**: JSON files (`projects.json`, `preferences.json`)
- **Update check**: GitHub Releases API + SHA256 verification, run off the UI thread

```text
app/src/
  main.rs                     # bootstrap: fonts, menus, keybindings, window
  app.rs                      # dashboard: sidebar, tabs, dialogs, actions
  terminal.rs                 # alacritty_terminal <-> GPUI integration
  theme.rs                    # palettes (default/Everforest) -> GPUI colors
  i18n.rs                     # zh-CN / en-US dictionaries
  ui.rs                       # shared widgets (text field, buttons)
  services/                   # settings/project/claude-session/storage/update/editor
  models/                     # data models (app_settings, claude session, ...)
  utils/                      # external-terminal integration
```

### Data files

- `projects.json`
- `preferences.json`

Default macOS data directory:

`~/Library/Application Support/CloudCodeSessionManager/`

> Settings written by the previous Tauri version are read as-is — theme, language, aliases, layout and window size carry over automatically.

---

## Config keys (brief)

All app settings are stored in `preferences.json`. Use `Open Config File` from the app menu to edit it.

Commonly used keys:

- `appearance.theme_preference`: theme mode (`light | dark | system`)
- `appearance.language`: UI language (`zh-CN | en-US`)
- `appearance.theme_preset`: palette preset (`default | everforest`)
- `claude.use_custom_startup_args` / `claude.custom_startup_args`: Claude startup args
- `integrations.default_external_terminal` / `integrations.default_external_editor`: external tools
- `ui.sidebar_collapsed` / `ui.layout` / `ui.window`: sidebar/layout/window sizing
- `sessions.restore_last_opened_session` / `sessions.last_opened`: startup restore behavior

After editing, click `Reload Config` in the app menu to hot-reload without restarting.

---

## Quick start

### Prerequisites

- Rust 1.85+ (2024 edition)
- macOS 13+
- Recommended: `claude` CLI installed
- Xcode with the Metal toolchain component (`xcodebuild -downloadComponent MetalToolchain`)

### Run locally

```bash
cargo run --manifest-path app/Cargo.toml
```

### Build a release bundle

```bash
cargo build --release --manifest-path app/Cargo.toml
bash scripts/bundle-app.sh release   # assembles ClaudeSessionSwitch.app (ad-hoc signed)
```

---

## CI and release

### CI (`.github/workflows/build.yml`)

On `main/develop` pushes and pull requests:

- `cargo check --all-targets`
- `cargo clippy --all-targets -- -D warnings`
- `cargo fmt --check`
- `cargo test`

### Release (`.github/workflows/release.yml`)

On `v*` tags:

- build and publish macOS binaries for:
  - `arm64`
  - `x64` (Intel)
- assemble `ClaudeSessionSwitch.app`, ad-hoc sign, and package DMGs
- auto-generate release notes and `SHA256SUMS`
- release notes are generated from commits/PRs between the previous release and current tag (`generate_release_notes: true`)

---

## License

Released under the [MIT License](./LICENSE).
