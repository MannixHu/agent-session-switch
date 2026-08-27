# Agent Session Switch

> macOS 轻量桌面 AI 编码代理会话切换器：支持 **Claude Code、Codex CLI、oh my pi (omp)**。
> 基于 **Rust + [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) + [alacritty_terminal](https://github.com/alacritty/alacritty/tree/master/alacritty_terminal)** 原生构建，支持项目分组、会话恢复与内嵌终端。

[English](./README.md) | 中文（当前）

---

## 项目简介

`Agent Session Switch` 关注的核心不是替代命令行，而是把「会话管理」这件事做得更直观、更可恢复：

- 用 GUI 组织项目和会话层级
- 用内嵌终端跨 agent 恢复会话（`claude --resume` / `codex resume` / `omp -r`）
- 用配置文件驱动应用行为，便于人工和 AI 同时管理

---

## 设计理念

这个 App 的设计目标不是"做一个更重的 IDE"，而是做一个**更轻、更稳、更不打断思路**的会话工作台：

- **沉浸式优先**：界面尽量克制，视觉层级清晰，但不过度强调装饰
- **减少干扰**：默认弱化非关键信息，让注意力始终落在当前任务与终端输出
- **终端为核心**：CLI 仍是第一执行入口，GUI 只负责组织、切换与管理
- **会话管理高于花哨功能**：快速定位项目、快速切换 session、快速恢复上下文
- **配置驱动**：尽可能通过配置文件管理行为，方便个人定制和 AI 自动化协作

一句话：**保留 CLI 的原生体验，消除会话管理的摩擦。**

---

## 功能亮点

### 1) 项目与会话管理（多 agent）

- 项目**仅手动添加**：不会从 CLI 历史自动导入任何项目——添加哪个目录，侧边栏就显示哪个
- 侧边栏**只显示在本应用里创建的会话**（记录在应用自己的 `sessions.json` 注册表中），CLI 磁盘上的既有会话一律不读取、不展示
  - 新建 Claude 会话时预先绑定 id（`claude --session-id <uuid>`）
  - Codex / oh my pi 会话在终端关闭时自动回填真实 id
- ChatGPT 风格侧边栏：项目与会话统一搜索、一键新建会话、项目快捷操作（新建 Claude / Codex / oh my pi 会话、外部终端/编辑器打开、移除）
- 会话行带 agent 徽标、显示名与修改时间；支持重命名、停止、删除（删除会同时清理 CLI 的底层会话文件）

### 2) 跨 agent 会话恢复

- Claude：`claude --resume <id>`；Codex：`codex resume <id>`；oh my pi：`omp -r <id>`，均带 `|| <agent>` 优雅回退到同目录新会话
- 可配置 Claude 启动参数（默认可选 `--dangerously-skip-permissions`）
- 启动时自动恢复上次打开的会话（任意 agent）

### 3) 内嵌终端

- 基于 `alacritty_terminal`（Alacritty 的终端核心）+ GPUI 渲染
- 多标签终端、输出流式渲染、尺寸自适应、1 万行回滚缓冲、光标闪烁
- 选区与复制粘贴（`Cmd+C` / `Cmd+V` / `Cmd+A`）、括号粘贴模式
- `Cmd+点击` 打开终端输出中的 http(s) 链接
- 内嵌 shell 继承登录 shell 的 PATH（mise / volta / homebrew 均可用）

### 4) 配置驱动（对 AI 友好）

- 设置持久化到 `preferences.json`（与旧版 Tauri 方案的 schema 完全兼容）
- 主题/语言/布局/窗口尺寸/会话恢复均可配置
- 菜单提供 `打开配置文件` + `重新加载配置`，无需重启即可热更新

### 5) 主题与语言

- 浅色 / 深色 / 跟随系统三种模式，内置 Default 与 Everforest 两套色板
- 完整的中英文界面（`zh-CN` / `en-US`）

### 6) macOS 菜单集成

应用菜单包含：

- `设置…`（`Cmd+,`）
- `打开配置文件`（系统默认应用打开）
- `重新加载配置`（热加载到当前界面）
- `检查更新…`（GitHub Releases，SHA256 校验下载）
- `新建终端`（`Cmd+T`）、`快速新建 Claude 会话`（`Cmd+N`）、`新建 Codex 会话`、`新建 oh my pi 会话`、`收起/展开侧边栏`（`Cmd+B`）

---

## 架构

- **UI 框架**：GPUI（Zed 的 GPU 加速 Rust UI 框架）
- **终端仿真**：alacritty_terminal + 自带 PTY 事件循环
- **数据持久化**：JSON 文件（`projects.json`、`preferences.json`）
- **更新检查**：GitHub Releases API + SHA256 校验，全部在后台线程执行

```text
app/src/
  main.rs                     # 启动：字体、菜单、快捷键、窗口
  app.rs                      # 主界面：侧边栏、标签页、弹窗、动作
  terminal.rs                 # alacritty_terminal 与 GPUI 的集成
  theme.rs                    # 色板（默认/Everforest）→ GPUI 颜色
  i18n.rs                     # 中英文词典
  ui.rs                       # 共享控件（文本输入框、按钮）
  services/
    agent_session_service.rs  # 多 agent 会话发现（claude/codex/omp）
    claude_session_service.rs # Claude 专属索引处理
    ...                       # 设置/项目/存储/更新/编辑器
  models/                     # 数据模型（app_settings、claude session 等）
  utils/                      # 外部终端集成
```

### 数据文件

- `projects.json`
- `preferences.json`

macOS 默认数据目录：

`~/Library/Application Support/CloudCodeSessionManager/`

> 旧版（Tauri 方案）写入的设置可直接读取——主题、语言、别名、布局与窗口尺寸自动延续。

---

## 配置项（简表）

所有设置都存储在 `preferences.json`，可从应用菜单 `打开配置文件` 直接编辑。

常用键：

- `appearance.theme_preference`：主题模式（`light | dark | system`）
- `appearance.language`：界面语言（`zh-CN | en-US`）
- `appearance.theme_preset`：色板预设（`default | everforest`）
- `claude.use_custom_startup_args` / `claude.custom_startup_args`：Claude 启动参数
- `integrations.default_external_terminal` / `integrations.default_external_editor`：外部工具
- `ui.sidebar_collapsed` / `ui.layout` / `ui.window`：侧边栏/布局/窗口尺寸
- `sessions.restore_last_opened_session` / `sessions.last_opened`：启动恢复行为

编辑后点击菜单 `重新加载配置` 即可热更新。

---

## 快速开始

### 环境要求

- Rust 1.85+（2024 edition）
- macOS 13+
- 建议：已安装 `claude` CLI
- Xcode 及 Metal 工具链组件（`xcodebuild -downloadComponent MetalToolchain`）

### 本地运行

```bash
cargo run --manifest-path app/Cargo.toml
```

### 构建发布包

```bash
cargo build --release --manifest-path app/Cargo.toml
bash scripts/bundle-app.sh release   # 生成 AgentSessionSwitch.app（ad-hoc 签名）
```

---

## CI 与发布

### CI（`.github/workflows/build.yml`）

在 `main/develop` 推送与 PR 时：

- `cargo check --all-targets`
- `cargo clippy --all-targets -- -D warnings`
- `cargo fmt --check`
- `cargo test`

### 发布（`.github/workflows/release.yml`）

在 `v*` tag 上：

- 构建并发布 macOS 双架构产物：
  - `arm64`
  - `x64`（Intel）
- 组装 `AgentSessionSwitch.app`、ad-hoc 签名并打包 DMG
- 自动生成发布说明与 `SHA256SUMS`
- 发布说明基于上一个 release 与当前 tag 之间的 commits/PRs（`generate_release_notes: true`）

---

## 许可证

基于 [MIT License](./LICENSE) 发布。
