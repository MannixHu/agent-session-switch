//! Embedded terminal: alacritty_terminal's grid + event loop wired to GPUI
//! rendering. Sessions spawn either a plain login shell or a
//! `claude ... -r <session_id>` resume command, matching the previous
//! xterm.js + portable-pty behavior.

use std::borrow::Cow;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, EventLoopSender, Msg};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Direction, Line, Point as TerminalPoint, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::search::{RegexIter, RegexSearch};
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::tty::{self, Shell};
use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};
use crossbeam_channel::{Receiver, Sender, unbounded};
use gpui::prelude::*;
use gpui::{
    App, AppContext, Bounds, ClipboardItem, Context, FocusHandle, Focusable, FontWeight,
    KeyDownEvent, Keystroke, Modifiers, MouseButton, MouseDownEvent, MouseExitEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, Point, Render, ScrollDelta, ScrollWheelEvent, StyledText,
    TextRun, UnderlineStyle, Window, canvas, div, font, px,
};
use parking_lot::Mutex;

use crate::theme::{Theme, rgb_to_hsla};

pub const TERMINAL_FONT_SIZE: f32 = 13.0;
pub const TERMINAL_CELL_HEIGHT: f32 = 18.0;
const TERMINAL_CELL_WIDTH: f32 = 7.8;
const TERMINAL_PADDING_X: f32 = 10.0;
const TERMINAL_PADDING_Y: f32 = 8.0;
const TERMINAL_MIN_COLUMNS: usize = 20;
const TERMINAL_MIN_ROWS: usize = 8;
const TERMINAL_SCROLLBACK_LINES: usize = 10_000;
const TERMINAL_CURSOR_BLINK_INTERVAL: Duration = Duration::from_millis(500);
const TERMINAL_POLL_INTERVAL: Duration = Duration::from_millis(24);

#[rustfmt::skip]
const TERMINAL_LINK_REGEX: &str = "((ipfs:|ipns:|magnet:|mailto:|gemini://|gopher://|https://|http://|news:|file:|git://|ssh:|ftp://)|\
                                    (/|~/|\\./|\\.\\./|[A-Za-z0-9._@%+~-]+/))\
                                   [^\u{0000}-\u{001F}\u{007F}-\u{009F}<>\"\\s{-}\\^⟨⟩`\\\\]+";

fn terminal_font(weight: FontWeight) -> gpui::Font {
    let mut f = font("JetBrains Mono");
    f.weight = weight;
    f
}

/// How the PTY child is launched.
#[derive(Debug, Clone)]
pub enum TerminalLaunch {
    /// Plain interactive login shell in the working directory.
    Plain,
    /// Resume an agent session in the working directory, falling back to a
    /// fresh session of the same agent. An empty `session_id` skips the
    /// resume attempt and just starts the agent. `claude_args` only applies
    /// to Claude.
    AgentResume {
        agent: crate::models::agent::AgentKind,
        session_id: String,
        claude_args: Vec<String>,
    },
}

/// Tracks the active terminal theme for OSC color queries.
static TERMINAL_DARK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

enum TerminalUiEvent {
    Title(String),
    ResetTitle,
    ClipboardStore(String),
    ClipboardLoad(Arc<dyn Fn(&str) -> String + Send + Sync>),
    Exited,
}

#[derive(Clone)]
struct TerminalEventProxy {
    dirty: Arc<AtomicBool>,
    sender: Arc<OnceLock<EventLoopSender>>,
    ui_events: Sender<TerminalUiEvent>,
    window_size: Arc<Mutex<WindowSize>>,
}

impl EventListener for TerminalEventProxy {
    fn send_event(&self, event: Event) {
        match event {
            Event::Wakeup | Event::MouseCursorDirty | Event::CursorBlinkingChange => {
                self.dirty.store(true, Ordering::Release);
            }
            Event::Title(title) => {
                let _ = self.ui_events.send(TerminalUiEvent::Title(title));
            }
            Event::ResetTitle => {
                let _ = self.ui_events.send(TerminalUiEvent::ResetTitle);
            }
            Event::ClipboardStore(_, text) => {
                let _ = self.ui_events.send(TerminalUiEvent::ClipboardStore(text));
            }
            Event::ClipboardLoad(_, formatter) => {
                let _ = self
                    .ui_events
                    .send(TerminalUiEvent::ClipboardLoad(formatter));
            }
            Event::PtyWrite(text) => {
                if let Some(sender) = self.sender.get() {
                    let _ = sender.send(Msg::Input(std::borrow::Cow::Owned(text.into_bytes())));
                }
            }
            Event::TextAreaSizeRequest(formatter) => {
                if let Some(sender) = self.sender.get() {
                    let _ = sender.send(Msg::Input(std::borrow::Cow::Owned(
                        formatter(*self.window_size.lock()).into_bytes(),
                    )));
                }
            }
            Event::ColorRequest(index, formatter) => {
                if let Some(sender) = self.sender.get() {
                    let is_dark = TERMINAL_DARK.load(Ordering::Acquire);
                    let rgb = terminal_rgb(index, is_dark);
                    let _ = sender.send(Msg::Input(std::borrow::Cow::Owned(
                        formatter(rgb).into_bytes(),
                    )));
                }
            }
            Event::Bell => {}
            Event::Exit | Event::ChildExit(_) => {
                let _ = self.ui_events.send(TerminalUiEvent::Exited);
                self.dirty.store(true, Ordering::Release);
            }
        }
    }
}

struct TerminalDimensions {
    columns: usize,
    rows: usize,
}

impl Dimensions for TerminalDimensions {
    fn total_lines(&self) -> usize {
        self.rows
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.columns
    }
}

/// A live PTY: the shared term grid, the channel to write into it, and the
/// dirty flag the render loop polls.
struct TerminalSession {
    term: Arc<FairMutex<Term<TerminalEventProxy>>>,
    sender: EventLoopSender,
    dirty: Arc<AtomicBool>,
    ui_events: Receiver<TerminalUiEvent>,
    window_size: Arc<Mutex<WindowSize>>,
    grid_size: (usize, usize),
    url_regex: RegexSearch,
}

impl TerminalSession {
    fn new(
        working_directory: &Path,
        launch: &TerminalLaunch,
        columns: usize,
        rows: usize,
    ) -> Result<Self, String> {
        let columns = columns.max(TERMINAL_MIN_COLUMNS);
        let rows = rows.max(TERMINAL_MIN_ROWS);
        let window_size = WindowSize {
            num_lines: rows.min(u16::MAX as usize) as u16,
            num_cols: columns.min(u16::MAX as usize) as u16,
            cell_width: TERMINAL_CELL_WIDTH.round() as u16,
            cell_height: TERMINAL_CELL_HEIGHT.round() as u16,
        };
        let shared_window_size = Arc::new(Mutex::new(window_size));
        let dirty = Arc::new(AtomicBool::new(true));
        let sender_slot = Arc::new(OnceLock::new());
        let (ui_event_tx, ui_events) = unbounded();
        let url_regex = RegexSearch::new(TERMINAL_LINK_REGEX)
            .map_err(|error| format!("compile terminal link regex: {error}"))?;
        let proxy = TerminalEventProxy {
            dirty: dirty.clone(),
            sender: sender_slot.clone(),
            ui_events: ui_event_tx,
            window_size: shared_window_size.clone(),
        };

        let config = Config {
            scrolling_history: TERMINAL_SCROLLBACK_LINES,
            ..Default::default()
        };
        let dimensions = TerminalDimensions { columns, rows };
        let term = Arc::new(FairMutex::new(Term::new(
            config,
            &dimensions,
            proxy.clone(),
        )));

        let (program, args) = shell_command(launch);
        let mut options = tty::Options {
            shell: Some(Shell::new(program, args)),
            working_directory: Some(working_directory.to_path_buf()),
            drain_on_exit: false,
            ..Default::default()
        };
        let inherited_term = std::env::var("TERM").unwrap_or_default();
        let resolved_term = if inherited_term.trim().is_empty() || inherited_term == "dumb" {
            "xterm-256color".to_string()
        } else {
            inherited_term
        };
        options.env.insert("TERM".into(), resolved_term);
        options.env.insert("COLORTERM".into(), "truecolor".into());
        // Avoid nested Claude Code detection inside embedded terminals.
        options.env.insert("CLAUDECODE".into(), "0".into());
        options.env.insert("PATH".into(), runtime_path());

        let pty = tty::new(&options, window_size, 0).map_err(|error| {
            format!("spawn terminal in {}: {error}", working_directory.display())
        })?;
        log::info!(
            "terminal spawned cwd={} launch={:?} cols={} rows={}",
            working_directory.display(),
            launch,
            columns,
            rows
        );
        let event_loop = EventLoop::new(term.clone(), proxy, pty, false, false)
            .map_err(|error| format!("create terminal event loop: {error}"))?;
        let sender = event_loop.channel();
        sender_slot
            .set(sender.clone())
            .map_err(|_| "initialize terminal sender".to_string())?;
        event_loop.spawn();

        Ok(Self {
            term,
            sender,
            dirty,
            ui_events,
            window_size: shared_window_size,
            grid_size: (columns, rows),
            url_regex,
        })
    }

    fn write(&self, bytes: impl Into<Cow<'static, [u8]>>) {
        let bytes = bytes.into();
        if !bytes.is_empty() {
            let _ = self.sender.send(Msg::Input(bytes));
        }
    }

    fn resize(&mut self, columns: usize, rows: usize, cell_width: f32) {
        let columns = columns.max(TERMINAL_MIN_COLUMNS);
        let rows = rows.max(TERMINAL_MIN_ROWS);
        if self.grid_size == (columns, rows) {
            return;
        }

        self.grid_size = (columns, rows);
        let dimensions = TerminalDimensions { columns, rows };
        self.term.lock().resize(dimensions);
        let size = WindowSize {
            num_lines: rows.min(u16::MAX as usize) as u16,
            num_cols: columns.min(u16::MAX as usize) as u16,
            cell_width: cell_width.round() as u16,
            cell_height: TERMINAL_CELL_HEIGHT.round() as u16,
        };
        *self.window_size.lock() = size;
        let _ = self.sender.send(Msg::Resize(size));
        self.dirty.store(true, Ordering::Release);
    }

    fn mode(&self) -> TermMode {
        *self.term.lock().mode()
    }

    fn scroll(&self, lines: i32) {
        if lines == 0 {
            return;
        }
        self.term.lock().scroll_display(Scroll::Delta(lines));
        self.dirty.store(true, Ordering::Release);
    }

    fn take_dirty(&self) -> bool {
        self.dirty.swap(false, Ordering::AcqRel)
    }

    fn selected_text(&self) -> Option<String> {
        self.term
            .lock()
            .selection_to_string()
            .filter(|text| !text.is_empty())
    }

    fn link_at(&mut self, point: TerminalPoint) -> Option<String> {
        let term = self.term.lock();
        let line_start = TerminalPoint::new(point.line, Column(0));
        let line_end = TerminalPoint::new(point.line, term.last_column());
        let line = &term.grid()[point.line];
        let mut text = String::new();
        for column in 0..=term.last_column().0 {
            let cell = &line[Column(column)];
            if cell.flags.contains(Flags::WIDE_CHAR_SPACER)
                || cell.flags.contains(Flags::LEADING_WIDE_CHAR_SPACER)
                || cell.flags.contains(Flags::HIDDEN)
            {
                text.push(' ');
            } else {
                text.push(cell.c);
            }
            if let Some(zerowidth) = cell.zerowidth() {
                for grapheme in zerowidth {
                    text.push(*grapheme);
                }
            }
        }
        let clicked_offset = point.column.0;
        for range in RegexIter::new(
            line_start,
            line_end,
            Direction::Right,
            &term,
            &mut self.url_regex,
        ) {
            let start_column = range.start().column.0;
            let end_column = range.end().column.0;
            if clicked_offset >= start_column && clicked_offset <= end_column {
                return Some(term.bounds_to_string(*range.start(), *range.end()));
            }
        }
        None
    }

    fn snapshot(
        &self,
        theme: &Theme,
        selection_color: gpui::Hsla,
        text_color: gpui::Hsla,
        cursor: TerminalCursorStyle,
    ) -> TerminalSnapshot {
        TERMINAL_DARK.store(theme.is_dark, Ordering::Release);
        let term = self.term.lock();
        let content = term.renderable_content();
        let columns = self.grid_size.0;
        let rows = self.grid_size.1;
        let selection = content.selection;
        let cursor_row = content.cursor.point.line.0 + content.display_offset as i32;
        let cursor_column = content.cursor.point.column.0;
        let cursor_hidden = matches!(
            content.cursor.shape,
            alacritty_terminal::vte::ansi::CursorShape::Hidden
        );
        let outline_cursor = (!cursor_hidden
            && cursor == TerminalCursorStyle::Outline
            && (0..rows as i32).contains(&cursor_row)
            && cursor_column < columns)
            .then_some((cursor_row as usize, cursor_column));
        let mut cells = vec![TerminalCell::blank(theme); columns * rows];

        for indexed in content.display_iter {
            let row = indexed.point.line.0 + content.display_offset as i32;
            let column = indexed.point.column.0;
            if row < 0 || row as usize >= rows || column >= columns {
                continue;
            }
            let cell = indexed.cell;
            let mut text = if cell.flags.contains(Flags::WIDE_CHAR_SPACER)
                || cell.flags.contains(Flags::LEADING_WIDE_CHAR_SPACER)
                || cell.flags.contains(Flags::HIDDEN)
            {
                " ".to_owned()
            } else {
                cell.c.to_string()
            };
            if let Some(zerowidth) = cell.zerowidth() {
                text.extend(zerowidth);
            }

            let mut foreground = resolve_color(cell.fg, content.colors, theme, true, text_color);
            let mut background = resolve_color(cell.bg, content.colors, theme, false, text_color);
            if cell.flags.contains(Flags::INVERSE) {
                std::mem::swap(&mut foreground, &mut background);
            }
            if cell.flags.contains(Flags::DIM) {
                if theme.is_dark {
                    foreground.l *= 0.7;
                } else {
                    foreground.l = 1.0 - (1.0 - foreground.l) * 0.7;
                }
            }
            if selection.is_some_and(|selection| selection.contains(indexed.point)) {
                background = selection_color;
                foreground = text_color;
            }
            if cursor == TerminalCursorStyle::Solid
                && !cursor_hidden
                && row == cursor_row
                && column == cursor_column
            {
                background = text_color;
                foreground = theme.terminal_background;
            }

            cells[row as usize * columns + column] = TerminalCell {
                text,
                foreground,
                background,
                bold: cell.flags.contains(Flags::BOLD),
                italic: cell.flags.contains(Flags::ITALIC),
                underline: cell.flags.intersects(Flags::ALL_UNDERLINES),
            };
        }

        let mut rendered_rows = Vec::with_capacity(rows);
        for row in cells.chunks(columns) {
            let mut text = String::new();
            let mut runs: Vec<TerminalRun> = Vec::new();
            for cell in row {
                let len = cell.text.len();
                text.push_str(&cell.text);
                let style = TerminalRunStyle {
                    foreground: cell.foreground,
                    background: cell.background,
                    bold: cell.bold,
                    italic: cell.italic,
                    underline: cell.underline,
                };
                if let Some(run) = runs.last_mut().filter(|run| run.style == style) {
                    run.len += len;
                } else {
                    runs.push(TerminalRun { len, style });
                }
            }
            rendered_rows.push(TerminalRow { text, runs });
        }

        TerminalSnapshot {
            rows: rendered_rows,
            outline_cursor,
        }
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = self.sender.send(Msg::Shutdown);
    }
}

#[derive(Clone)]
struct TerminalCell {
    text: String,
    foreground: gpui::Hsla,
    background: gpui::Hsla,
    bold: bool,
    italic: bool,
    underline: bool,
}

impl TerminalCell {
    fn blank(theme: &Theme) -> Self {
        Self {
            text: " ".into(),
            foreground: theme.terminal_foreground,
            background: theme.terminal_background,
            bold: false,
            italic: false,
            underline: false,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
struct TerminalRunStyle {
    foreground: gpui::Hsla,
    background: gpui::Hsla,
    bold: bool,
    italic: bool,
    underline: bool,
}

struct TerminalRun {
    len: usize,
    style: TerminalRunStyle,
}

struct TerminalRow {
    text: String,
    runs: Vec<TerminalRun>,
}

struct TerminalSnapshot {
    rows: Vec<TerminalRow>,
    outline_cursor: Option<(usize, usize)>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum TerminalCursorStyle {
    Solid,
    Outline,
    Hidden,
}

fn terminal_cursor_style(focused: bool, blink_visible: bool) -> TerminalCursorStyle {
    if !focused {
        TerminalCursorStyle::Outline
    } else if blink_visible {
        TerminalCursorStyle::Solid
    } else {
        TerminalCursorStyle::Hidden
    }
}

/// Simple blink driver: flips visibility on a timer while the terminal holds
/// keyboard focus.
struct TerminalCursorBlink {
    visible: bool,
    generation: usize,
}

impl TerminalCursorBlink {
    fn new() -> Self {
        Self {
            visible: true,
            generation: 0,
        }
    }

    fn run(&mut self, cx: &mut Context<Self>) {
        self.generation += 1;
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(TERMINAL_CURSOR_BLINK_INTERVAL)
                    .await;
                let Ok(alive) = this.update(cx, |blink, cx| {
                    if blink.generation != generation {
                        return false;
                    }
                    blink.visible = !blink.visible;
                    cx.notify();
                    true
                }) else {
                    return;
                };
                if !alive {
                    return;
                }
            }
        })
        .detach();
    }

    fn reset(&mut self, cx: &mut Context<Self>) {
        self.visible = true;
        self.run(cx);
        cx.notify();
    }
}

/// The GPUI view owning one terminal session.
pub struct TerminalView {
    session: Option<TerminalSession>,
    error: Option<String>,
    focus_handle: FocusHandle,
    #[allow(dead_code)]
    pub working_directory: PathBuf,
    #[allow(dead_code)]
    pub launch: TerminalLaunch,
    dyn_title: Option<String>,
    exited: bool,
    scroll_accumulator: f32,
    measured_cell_width: Option<f32>,
    grid_bounds: Option<Bounds<Pixels>>,
    grid_bounds_capture: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    selecting: bool,
    hovered_link: Option<String>,
    cursor_blink: gpui::Entity<TerminalCursorBlink>,
    cursor_focus_tracking_started: bool,
    subscriptions: Vec<gpui::Subscription>,
}

impl TerminalView {
    pub fn new(working_directory: PathBuf, launch: TerminalLaunch, cx: &mut Context<Self>) -> Self {
        let spawn_directory = working_directory.clone();
        let spawn_launch = launch.clone();
        cx.spawn(async move |this, cx| {
            let started = cx
                .background_executor()
                .spawn(async move { TerminalSession::new(&spawn_directory, &spawn_launch, 80, 24) })
                .await;
            if this
                .update(cx, |this, cx| {
                    match started {
                        Ok(session) => this.session = Some(session),
                        Err(error) => this.error = Some(error),
                    }
                    cx.notify();
                })
                .is_err()
            {
                return;
            }
            loop {
                cx.background_executor().timer(TERMINAL_POLL_INTERVAL).await;
                if this
                    .update(cx, |this, cx| {
                        if this.poll(cx) {
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();

        let cursor_blink = cx.new(|_| TerminalCursorBlink::new());
        let mut subscriptions = vec![cx.observe(&cursor_blink, |_, _, cx| cx.notify())];
        subscriptions.push(cx.observe_global::<crate::CurrentTheme>(|_, cx| cx.notify()));

        Self {
            session: None,
            error: None,
            focus_handle: cx.focus_handle(),
            working_directory,
            launch,
            dyn_title: None,
            exited: false,
            scroll_accumulator: 0.0,
            measured_cell_width: None,
            grid_bounds: None,
            grid_bounds_capture: std::rc::Rc::new(std::cell::Cell::new(None)),
            selecting: false,
            hovered_link: None,
            cursor_blink,
            cursor_focus_tracking_started: false,
            subscriptions,
        }
    }

    /// Terminal title reported by OSC escape sequences, if any.
    pub fn dyn_title(&self) -> Option<&str> {
        self.dyn_title.as_deref()
    }

    #[allow(dead_code)]
    pub fn has_exited(&self) -> bool {
        self.exited
    }

    #[allow(dead_code)]
    pub fn send_text(&mut self, text: &str) {
        if let Some(session) = &self.session {
            session.write(text.as_bytes().to_vec());
            session.dirty.store(true, Ordering::Release);
        }
    }

    fn poll(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(session) = &self.session else {
            return false;
        };
        let mut changed = session.take_dirty();
        while let Ok(event) = session.ui_events.try_recv() {
            changed = true;
            match event {
                TerminalUiEvent::Title(title) => self.dyn_title = Some(title),
                TerminalUiEvent::ResetTitle => self.dyn_title = None,
                TerminalUiEvent::ClipboardStore(text) => {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
                TerminalUiEvent::ClipboardLoad(formatter) => {
                    let text = cx
                        .read_from_clipboard()
                        .and_then(|item| item.text())
                        .unwrap_or_default();
                    session.write(formatter(&text).into_bytes());
                }
                TerminalUiEvent::Exited => self.exited = true,
            }
        }
        changed
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.cursor_blink.update(cx, |blink, cx| blink.reset(cx));
        let keystroke = &event.keystroke;

        if clipboard_modifier_pressed(&keystroke.modifiers)
            && keystroke.key.eq_ignore_ascii_case("c")
        {
            self.copy_selection(cx);
            window.prevent_default();
            cx.stop_propagation();
            return;
        }
        if clipboard_modifier_pressed(&keystroke.modifiers)
            && keystroke.key.eq_ignore_ascii_case("a")
        {
            self.select_all(cx);
            window.prevent_default();
            cx.stop_propagation();
            return;
        }

        let Some(session) = &self.session else {
            return;
        };
        if clipboard_modifier_pressed(&keystroke.modifiers)
            && keystroke.key.eq_ignore_ascii_case("v")
        {
            if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                session.term.lock().selection = None;
                session.write(bracketed_paste(&text, session.mode()));
                session.dirty.store(true, Ordering::Release);
                window.prevent_default();
                cx.stop_propagation();
            }
            return;
        }

        if let Some(bytes) = terminal_key_bytes(keystroke, session.mode()) {
            session.term.lock().selection = None;
            session.write(bytes);
            session.dirty.store(true, Ordering::Release);
            window.prevent_default();
            cx.stop_propagation();
        }
    }

    fn cell_width(&self) -> f32 {
        self.measured_cell_width.unwrap_or(TERMINAL_CELL_WIDTH)
    }

    fn grid_point_for_position(
        &self,
        position: Point<Pixels>,
        clamp_to_grid: bool,
    ) -> Option<(TerminalPoint, Side)> {
        let bounds = self.grid_bounds?;
        let session = self.session.as_ref()?;
        let display_offset = session.term.lock().grid().display_offset();
        Some(terminal_grid_point(
            bounds,
            position,
            self.cell_width(),
            session.grid_size.0,
            session.grid_size.1,
            display_offset,
            clamp_to_grid,
        ))
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle, cx);
        let Some((point, side)) = self.grid_point_for_position(event.position, false) else {
            return;
        };

        if primary_modifier_pressed(&event.modifiers)
            && let Some(link) = self
                .session
                .as_mut()
                .and_then(|session| session.link_at(point))
        {
            self.selecting = false;
            self.hovered_link = Some(link.clone());
            if link.starts_with("http://") || link.starts_with("https://") {
                cx.open_url(&link);
            }
            window.prevent_default();
            cx.stop_propagation();
            cx.notify();
            return;
        }

        let Some(session) = &self.session else {
            return;
        };

        let mut term = session.term.lock();
        if event.modifiers.shift
            && let Some(selection) = term.selection.as_mut()
        {
            selection.update(point, side);
        } else {
            let selection_type = match event.click_count {
                2 => SelectionType::Semantic,
                count if count >= 3 => SelectionType::Lines,
                _ => SelectionType::Simple,
            };
            term.selection = Some(Selection::new(selection_type, point, side));
        }
        drop(term);

        self.selecting = true;
        session.dirty.store(true, Ordering::Release);
        cx.stop_propagation();
        cx.notify();
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        let hover_changed = if self.selecting {
            self.set_hovered_link(None)
        } else {
            self.refresh_hovered_link(primary_modifier_pressed(&event.modifiers), event.position)
        };
        if !self.selecting || event.pressed_button != Some(MouseButton::Left) {
            if hover_changed {
                cx.notify();
            }
            return;
        }
        let Some((point, side)) = self.grid_point_for_position(event.position, true) else {
            return;
        };
        let Some(session) = &self.session else {
            return;
        };

        if let Some(selection) = session.term.lock().selection.as_mut() {
            selection.update(point, side);
            session.dirty.store(true, Ordering::Release);
            cx.stop_propagation();
            cx.notify();
        } else if hover_changed {
            cx.notify();
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.selecting = false;
    }

    fn on_mouse_exit(&mut self, _: &MouseExitEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.set_hovered_link(None) {
            cx.notify();
        }
    }

    fn refresh_hovered_link(&mut self, command_pressed: bool, position: Point<Pixels>) -> bool {
        let link = if command_pressed {
            self.grid_point_for_position(position, false)
                .and_then(|(point, _)| self.session.as_mut()?.link_at(point))
        } else {
            None
        };
        self.set_hovered_link(link)
    }

    fn set_hovered_link(&mut self, link: Option<String>) -> bool {
        if self.hovered_link == link {
            return false;
        }
        self.hovered_link = link;
        true
    }

    fn selected_text(&self) -> Option<String> {
        self.session.as_ref()?.selected_text()
    }

    fn copy_selection(&self, cx: &mut App) {
        if let Some(text) = self.selected_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    fn select_all(&mut self, cx: &mut Context<Self>) {
        let Some(session) = &self.session else {
            return;
        };
        let mut term = session.term.lock();
        let start = TerminalPoint::new(term.topmost_line(), Column(0));
        let end = TerminalPoint::new(term.bottommost_line(), term.last_column());
        let mut selection = Selection::new(SelectionType::Simple, start, Side::Left);
        selection.update(end, Side::Right);
        term.selection = Some(selection);
        drop(term);
        session.dirty.store(true, Ordering::Release);
        cx.notify();
    }

    fn on_scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = &self.session else {
            return;
        };
        let delta = match event.delta {
            ScrollDelta::Pixels(delta) => f32::from(delta.y) / TERMINAL_CELL_HEIGHT,
            ScrollDelta::Lines(delta) => delta.y,
        };
        self.scroll_accumulator += delta;
        let lines = self.scroll_accumulator.trunc() as i32;
        if lines != 0 {
            self.scroll_accumulator -= lines as f32;
            session.scroll(lines);
            cx.stop_propagation();
            cx.notify();
        }
    }

    fn ensure_cursor_focus_tracking(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.cursor_focus_tracking_started {
            return;
        }
        self.cursor_focus_tracking_started = true;

        let focus_handle = self.focus_handle.clone();
        self.subscriptions.extend([
            cx.observe_window_activation(window, |terminal, window, cx| {
                terminal.update_cursor_blinking(window, cx);
            }),
            cx.on_focus(&focus_handle, window, |terminal, window, cx| {
                terminal.update_cursor_blinking(window, cx);
            }),
            cx.on_blur(&focus_handle, window, |terminal, window, cx| {
                terminal.update_cursor_blinking(window, cx);
            }),
        ]);
        self.update_cursor_blinking(window, cx);
    }

    fn update_cursor_blinking(&mut self, window: &Window, cx: &mut Context<Self>) {
        let focused = window.is_window_active() && self.focus_handle.is_focused(window);
        self.cursor_blink.update(cx, |blink, cx| {
            if focused {
                blink.reset(cx);
            } else {
                blink.generation += 1;
                blink.visible = true;
                cx.notify();
            }
        });
    }

    fn render_scrollbar(&self, theme: &Theme) -> Option<impl IntoElement> {
        let session = self.session.as_ref()?;
        let term = session.term.lock();
        let history = term.grid().history_size();
        let display_offset = term.grid().display_offset();
        let rows = session.grid_size.1;
        if history == 0 {
            return None;
        }
        let fraction = display_offset as f32 / history as f32;
        let track_height = rows as f32 * TERMINAL_CELL_HEIGHT;
        let bar_height = (track_height * rows as f32 / (history + rows) as f32).max(24.0);
        let max_top = (track_height - bar_height).max(0.0);
        let top = fraction * max_top;
        Some(
            div()
                .absolute()
                .right_1()
                .top(px(top))
                .w(px(3.0))
                .h(px(bar_height))
                .rounded_sm()
                .bg(theme.border_soft),
        )
    }
}

impl Focusable for TerminalView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.ensure_cursor_focus_tracking(window, cx);
        let theme = crate::CurrentTheme::get(cx);
        let cell_width = *self.measured_cell_width.get_or_insert_with(|| {
            let text_system = cx.text_system();
            let font_id = text_system.resolve_font(&terminal_font(FontWeight::default()));
            text_system
                .advance(font_id, px(TERMINAL_FONT_SIZE), 'm')
                .map_or(TERMINAL_CELL_WIDTH, |advance| f32::from(advance.width))
        });

        // Size the grid from the bounds captured by last frame's canvas; the
        // first frame falls back to a sane default and self-corrects.
        self.grid_bounds = self.grid_bounds_capture.get().or(self.grid_bounds);
        let grid_bounds = self.grid_bounds;
        let (columns, rows) = match grid_bounds {
            Some(bounds) => (
                (((f32::from(bounds.size.width) - TERMINAL_PADDING_X * 2.0) / cell_width)
                    .floor()
                    .max(TERMINAL_MIN_COLUMNS as f32)) as usize,
                (((f32::from(bounds.size.height) - TERMINAL_PADDING_Y * 2.0)
                    / TERMINAL_CELL_HEIGHT)
                    .floor()
                    .max(TERMINAL_MIN_ROWS as f32)) as usize,
            ),
            None => (80, 24),
        };

        let terminal_focused = window.is_window_active() && self.focus_handle.is_focused(window);
        let cursor_style =
            terminal_cursor_style(terminal_focused, self.cursor_blink.read(cx).visible);
        if let Some(session) = self.session.as_mut() {
            session.resize(columns, rows, cell_width);
        }
        if self.selecting {
            self.set_hovered_link(None);
        } else {
            self.refresh_hovered_link(
                primary_modifier_pressed(&window.modifiers()),
                window.mouse_position(),
            );
        }

        let selection_color = theme.terminal_selection;
        let text_color = theme.terminal_foreground;
        let snapshot = self
            .session
            .as_ref()
            .map(|session| session.snapshot(&theme, selection_color, text_color, cursor_style));

        let mut screen = div()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .overflow_hidden()
            .flex()
            .flex_col()
            .relative()
            .cursor_text()
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_exit(cx.listener(Self::on_mouse_exit))
            .on_scroll_wheel(cx.listener(Self::on_scroll_wheel));

        if self.hovered_link.is_some() {
            screen = screen.cursor_pointer();
        }

        if let Some(snapshot) = snapshot {
            let TerminalSnapshot {
                rows: snapshot_rows,
                outline_cursor,
            } = snapshot;
            for row in snapshot_rows {
                let runs = row
                    .runs
                    .into_iter()
                    .map(|run| {
                        let weight = if run.style.bold {
                            FontWeight::BOLD
                        } else {
                            FontWeight::default()
                        };
                        let mut run_font = terminal_font(weight);
                        if run.style.italic {
                            run_font.style = gpui::FontStyle::Italic;
                        }
                        TextRun {
                            len: run.len,
                            font: run_font,
                            color: run.style.foreground,
                            background_color: Some(run.style.background),
                            underline: run.style.underline.then_some(UnderlineStyle {
                                thickness: px(1.0),
                                color: Some(run.style.foreground),
                                wavy: false,
                            }),
                            strikethrough: None,
                        }
                    })
                    .collect::<Vec<_>>();
                screen = screen.child(
                    div()
                        .h(px(TERMINAL_CELL_HEIGHT))
                        .flex_none()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_size(px(TERMINAL_FONT_SIZE))
                        .line_height(px(TERMINAL_CELL_HEIGHT))
                        .child(StyledText::new(row.text).with_runs(runs)),
                );
            }
            if let Some((row, column)) = outline_cursor {
                screen = screen.child(
                    div()
                        .absolute()
                        .left(px(column as f32 * cell_width))
                        .top(px(row as f32 * TERMINAL_CELL_HEIGHT))
                        .w(px(cell_width))
                        .h(px(TERMINAL_CELL_HEIGHT))
                        .border_1()
                        .border_color(theme.terminal_cursor),
                );
            }
        } else {
            screen = screen.child(
                div()
                    .p(px(12.0))
                    .text_size(px(13.0))
                    .text_color(if self.error.is_some() {
                        theme.alert_text
                    } else {
                        theme.text_sub
                    })
                    .child(self.error.clone().unwrap_or_else(|| "...".to_string())),
            );
        }

        let bounds_capture = self.grid_bounds_capture.clone();
        screen = screen.child(
            canvas(
                move |bounds, _, _| bounds_capture.set(Some(bounds)),
                |_, _, _, _| {},
            )
            .absolute()
            .inset_0(),
        );

        let scrollbar = self.render_scrollbar(&theme);

        div()
            .id("claude-terminal")
            .key_context("Terminal")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .size_full()
            .min_h_0()
            .min_w_0()
            .flex()
            .flex_col()
            .bg(theme.terminal_background)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .px(px(TERMINAL_PADDING_X))
                    .py(px(TERMINAL_PADDING_Y))
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .relative()
                    .child(screen)
                    .children(scrollbar),
            )
    }
}

#[inline]
fn primary_modifier_pressed(modifiers: &Modifiers) -> bool {
    modifiers.secondary()
}

#[inline]
fn clipboard_modifier_pressed(modifiers: &Modifiers) -> bool {
    if cfg!(target_os = "macos") {
        modifiers.secondary() && !modifiers.control && !modifiers.alt
    } else {
        modifiers.control && modifiers.shift && !modifiers.alt && !modifiers.platform
    }
}

fn terminal_grid_point(
    bounds: Bounds<Pixels>,
    position: Point<Pixels>,
    cell_width: f32,
    columns: usize,
    rows: usize,
    display_offset: usize,
    clamp_to_grid: bool,
) -> (TerminalPoint, Side) {
    let mut x = f32::from(position.x - bounds.origin.x) / cell_width;
    let mut y = f32::from(position.y - bounds.origin.y) / TERMINAL_CELL_HEIGHT;
    if clamp_to_grid {
        x = x.clamp(0.0, columns as f32 - 0.01);
        y = y.clamp(0.0, rows as f32 - 0.01);
    }
    let side = if x.fract() > 0.5 {
        Side::Right
    } else {
        Side::Left
    };
    let column = (x.floor() as usize).min(columns.saturating_sub(1));
    let line_offset = y.floor() as i32;
    let line = Line(-(display_offset as i32 - line_offset));
    (TerminalPoint::new(line, Column(column)), side)
}

fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn is_fish_shell(shell_path: &str) -> bool {
    Path::new(shell_path)
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.eq_ignore_ascii_case("fish"))
        .unwrap_or(false)
}

/// Build the (program, args) pair handed to the PTY, matching the previous
/// launch flow: plain terminals run the login shell; Claude sessions run the
/// resume script with a fallback to a fresh `claude` in the same directory.
fn shell_command(launch: &TerminalLaunch) -> (String, Vec<String>) {
    let shell = std::env::var("SHELL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "/bin/zsh".to_string());

    use crate::models::agent::AgentKind;
    match launch {
        TerminalLaunch::Plain => (shell, vec!["-i".into(), "-l".into()]),
        TerminalLaunch::AgentResume {
            agent,
            session_id,
            claude_args,
        } => {
            let mut base_parts = vec![match agent {
                AgentKind::Claude => "claude".to_string(),
                AgentKind::Codex => "codex".to_string(),
                AgentKind::OhMyPi => "omp".to_string(),
            }];
            if *agent == AgentKind::Claude {
                for arg in claude_args.iter().map(|value| value.trim()) {
                    if !arg.is_empty() {
                        base_parts.push(shell_quote(arg));
                    }
                }
            }
            let base_command = base_parts.join(" ");
            let resume_parts = if session_id.trim().is_empty() {
                None
            } else {
                let mut parts = base_parts.clone();
                match agent {
                    AgentKind::Claude => {
                        parts.push("-r".to_string());
                    }
                    AgentKind::Codex => {
                        parts.insert(1, "resume".to_string());
                    }
                    AgentKind::OhMyPi => {
                        parts.push("-r".to_string());
                    }
                }
                parts.push(shell_quote(session_id));
                Some(parts.join(" "))
            };
            let script = match resume_parts {
                Some(resume_command) if is_fish_shell(&shell) => {
                    format!("{}; or {}", resume_command, base_command)
                }
                Some(resume_command) => format!("{} || {}", resume_command, base_command),
                None => base_command,
            };
            (shell, vec!["-i".into(), "-l".into(), "-c".into(), script])
        }
    }
}

/// The PATH terminals are spawned with: what the user's login shell provides
/// (covers mise/volta/asdf shims), then the GUI process PATH, then the
/// standard macOS locations.
fn runtime_path() -> String {
    static RUNTIME_PATH: OnceLock<String> = OnceLock::new();
    RUNTIME_PATH
        .get_or_init(|| {
            let mut ordered: Vec<String> = Vec::new();
            let mut seen: HashSet<String> = HashSet::new();
            let mut push = |candidate: &str| {
                let trimmed = candidate.trim();
                if !trimmed.is_empty() && seen.insert(trimmed.to_string()) {
                    ordered.push(trimmed.to_string());
                }
            };

            let shell = std::env::var("SHELL")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "/bin/zsh".to_string());
            if let Ok(output) = std::process::Command::new(&shell)
                .args(["-l", "-c", r#"printf %s "$PATH""#])
                .output()
            {
                if output.status.success() {
                    let login_path = String::from_utf8_lossy(&output.stdout).to_string();
                    for entry in login_path.split(':') {
                        push(entry);
                    }
                }
            }
            for entry in std::env::var("PATH").unwrap_or_default().split(':') {
                push(entry);
            }
            for entry in [
                "/opt/homebrew/bin",
                "/opt/homebrew/sbin",
                "/usr/local/bin",
                "/usr/bin",
                "/bin",
                "/usr/sbin",
                "/sbin",
                "/Library/Apple/usr/bin",
            ] {
                push(entry);
            }
            if let Some(home) = dirs::home_dir() {
                for relative in [
                    ".local/bin",
                    ".local/share/mise/shims",
                    ".npm-global/bin",
                    "Library/pnpm",
                    ".bun/bin",
                    ".volta/bin",
                    ".cargo/bin",
                ] {
                    push(&home.join(relative).to_string_lossy());
                }
            }
            ordered.join(":")
        })
        .clone()
}

fn bracketed_paste(text: &str, mode: TermMode) -> Vec<u8> {
    if mode.contains(TermMode::BRACKETED_PASTE) {
        format!("\x1b[200~{}\x1b[201~", text.replace('\r', "")).into_bytes()
    } else {
        text.replace('\r', "").into_bytes()
    }
}

fn terminal_key_bytes(keystroke: &Keystroke, mode: TermMode) -> Option<Vec<u8>> {
    let modifiers = &keystroke.modifiers;
    let key = keystroke.key.as_str();

    if modifiers.platform {
        #[cfg(target_os = "macos")]
        return match key {
            "left" => Some(vec![0x01]),
            "right" => Some(vec![0x05]),
            "backspace" => Some(vec![0x15]),
            _ => None,
        };
        #[cfg(not(target_os = "macos"))]
        return None;
    }

    let modifier = 1
        + u8::from(modifiers.shift)
        + u8::from(modifiers.alt) * 2
        + u8::from(modifiers.control) * 4;
    let app_cursor = mode.contains(TermMode::APP_CURSOR);
    let special = match key {
        "enter" | "return" => Some("\r".to_owned()),
        "tab" if modifiers.shift => Some("\x1b[Z".to_owned()),
        "tab" => Some("\t".to_owned()),
        "backspace" => Some("\x7f".to_owned()),
        "escape" => Some("\x1b".to_owned()),
        "up" => Some(cursor_sequence('A', modifier, app_cursor)),
        "down" => Some(cursor_sequence('B', modifier, app_cursor)),
        "right" => Some(cursor_sequence('C', modifier, app_cursor)),
        "left" => Some(cursor_sequence('D', modifier, app_cursor)),
        "home" => Some(csi_sequence('H', modifier)),
        "end" => Some(csi_sequence('F', modifier)),
        "insert" => Some(tilde_sequence(2, modifier)),
        "delete" | "forwarddelete" => Some(tilde_sequence(3, modifier)),
        "pageup" => Some(tilde_sequence(5, modifier)),
        "pagedown" => Some(tilde_sequence(6, modifier)),
        "f1" => Some(function_sequence('P', modifier)),
        "f2" => Some(function_sequence('Q', modifier)),
        "f3" => Some(function_sequence('R', modifier)),
        "f4" => Some(function_sequence('S', modifier)),
        "f5" => Some(tilde_sequence(15, modifier)),
        "f6" => Some(tilde_sequence(17, modifier)),
        "f7" => Some(tilde_sequence(18, modifier)),
        "f8" => Some(tilde_sequence(19, modifier)),
        "f9" => Some(tilde_sequence(20, modifier)),
        "f10" => Some(tilde_sequence(21, modifier)),
        "f11" => Some(tilde_sequence(23, modifier)),
        "f12" => Some(tilde_sequence(24, modifier)),
        _ => None,
    };
    if let Some(special) = special {
        return Some(special.into_bytes());
    }

    let text = keystroke
        .key_char
        .as_deref()
        .or_else(|| (key.chars().count() == 1).then_some(key))?;
    let mut bytes = if modifiers.control {
        control_bytes(text)?
    } else {
        text.as_bytes().to_vec()
    };
    if modifiers.alt {
        bytes.insert(0, 0x1b);
    }
    Some(bytes)
}

fn control_bytes(text: &str) -> Option<Vec<u8>> {
    let byte = match text {
        " " => b' ',
        "-" => b'-',
        "/" => b'/',
        _ => {
            let mut chars = text.chars();
            let first = chars.next()?;
            if first.is_ascii_uppercase() || first.is_ascii_lowercase() {
                first.to_ascii_uppercase() as u8
            } else {
                return None;
            }
        }
    };
    Some(vec![byte & 0x1f])
}

fn cursor_sequence(final_byte: char, modifier: u8, app_cursor: bool) -> String {
    if modifier == 1 {
        format!("\x1b{}{}", if app_cursor { 'O' } else { '[' }, final_byte)
    } else {
        format!("\x1b[1;{modifier}{final_byte}")
    }
}

fn csi_sequence(final_byte: char, modifier: u8) -> String {
    if modifier == 1 {
        format!("\x1b[{final_byte}")
    } else {
        format!("\x1b[1;{modifier}{final_byte}")
    }
}

fn function_sequence(final_byte: char, modifier: u8) -> String {
    if modifier == 1 {
        format!("\x1bO{final_byte}")
    } else {
        format!("\x1b[1;{modifier}{final_byte}")
    }
}

fn tilde_sequence(number: u8, modifier: u8) -> String {
    if modifier == 1 {
        format!("\x1b[{number}~")
    } else {
        format!("\x1b[{number};{modifier}~")
    }
}

fn resolve_color(
    color: Color,
    colors: &alacritty_terminal::term::color::Colors,
    theme: &Theme,
    foreground: bool,
    default_text: gpui::Hsla,
) -> gpui::Hsla {
    let rgb = match color {
        Color::Spec(color) => Some(color),
        Color::Indexed(index) => {
            colors[index as usize].or_else(|| Some(terminal_rgb(index as usize, theme.is_dark)))
        }
        Color::Named(NamedColor::Foreground | NamedColor::BrightForeground) => None,
        Color::Named(NamedColor::Background) => return theme.terminal_background,
        Color::Named(NamedColor::Cursor) => return theme.terminal_cursor,
        Color::Named(named) => {
            colors[named as usize].or_else(|| Some(terminal_rgb(named as usize, theme.is_dark)))
        }
    };
    rgb.map(|rgb| rgb_to_hsla(rgb.r, rgb.g, rgb.b, 1.0))
        .unwrap_or(if foreground {
            default_text
        } else {
            theme.terminal_background
        })
}

/// ANSI palette when the program has not installed its own colors: Tomorrow
/// Night on dark surfaces, Tomorrow on light.
fn terminal_rgb(index: usize, is_dark: bool) -> Rgb {
    const ANSI_DARK: [u32; 16] = [
        0x1d1f21, 0xcc6666, 0xb5bd68, 0xf0c674, 0x81a2be, 0xb294bb, 0x8abeb7, 0xc5c8c6, 0x666666,
        0xd54e53, 0xb9ca4a, 0xe7c547, 0x7aa6da, 0xc397d8, 0x70c0b1, 0xeaeaea,
    ];
    const ANSI_LIGHT: [u32; 16] = [
        0x000000, 0xc82829, 0x718c00, 0xeab700, 0x4271ae, 0x8959a8, 0x3e999f, 0xc7c7c7, 0x8e909c,
        0xc82829, 0x718c00, 0xeab700, 0x4271ae, 0x8959a8, 0x3e999f, 0xffffff,
    ];
    let ansi = if is_dark { &ANSI_DARK } else { &ANSI_LIGHT };
    let value = match index {
        0..=15 => ansi[index],
        16..=231 => {
            let index = index - 16;
            let channel = |value: usize| {
                if value == 0 {
                    0
                } else {
                    55 + value as u32 * 40
                }
            };
            let red = channel(index / 36);
            let green = channel((index / 6) % 6);
            let blue = channel(index % 6);
            (red << 16) | (green << 8) | blue
        }
        232..=255 => {
            let value = 8 + (index as u32 - 232) * 10;
            (value << 16) | (value << 8) | value
        }
        value
            if value >= NamedColor::DimBlack as usize && value <= NamedColor::DimWhite as usize =>
        {
            let base = ansi[value - NamedColor::DimBlack as usize];
            let dim = |channel: u32| {
                if is_dark {
                    channel * 2 / 3
                } else {
                    channel + (255 - channel) / 3
                }
            };
            (dim((base >> 16) & 0xff) << 16) | (dim((base >> 8) & 0xff) << 8) | dim(base & 0xff)
        }
        _ => {
            if is_dark {
                0xe5e5e5
            } else {
                0x242424
            }
        }
    };
    Rgb {
        r: (value >> 16) as u8,
        g: (value >> 8) as u8,
        b: value as u8,
    }
}

/// One visible terminal tab: which claude session (if any) it belongs to and
/// the terminal view itself.
pub struct TerminalTab {
    pub key: String,
    pub title: String,
    pub project_path: Option<String>,
    pub session_id: Option<String>,
    pub view: gpui::Entity<TerminalView>,
}
