//! Small shared UI primitives: a minimal single-line text input and styled
//! button/label helpers used by the dashboard and dialogs.

use gpui::prelude::*;
use gpui::{
    App, ClipboardItem, Context, FocusHandle, Focusable, KeyDownEvent, Keystroke, MouseButton,
    SharedString, Window, div, px,
};

use crate::theme::Theme;

pub fn label_button(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    theme: &Theme,
    emphasized: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let id = id.into();
    div()
        .id(id.clone())
        .px(px(12.0))
        .py(px(5.0))
        .rounded_md()
        .text_size(px(12.5))
        .flex_none()
        .when(emphasized, |this| {
            this.bg(theme.accent).text_color(gpui::white())
        })
        .when(!emphasized, |this| {
            this.bg(theme.button_bg)
                .text_color(theme.button_text)
                .hover(|this| this.bg(theme.button_hover))
        })
        .child(label.into())
        .on_mouse_down(MouseButton::Left, {
            let id = id.clone();
            move |event, window, cx| {
                let click = ClickEvent {
                    id: id.clone(),
                    double: event.first_mouse || event.click_count >= 2,
                };
                on_click(&click, window, cx);
            }
        })
}

pub struct ClickEvent {
    #[allow(dead_code)]
    pub id: SharedString,
    #[allow(dead_code)]
    pub double: bool,
}

/// Minimal single-line text input: click to focus, caret at end, arrow keys,
/// backspace/delete, cmd+a/c/v/x, enter/escape bubble to on_commit/on_cancel.
type CommitHandler = Box<dyn Fn(&str, &mut Window, &mut App) + 'static>;
type CancelHandler = Box<dyn Fn(&mut Window, &mut App) + 'static>;

pub struct TextField {
    value: String,
    placeholder: String,
    focus_handle: FocusHandle,
    cursor: usize,
    selection: Option<std::ops::Range<usize>>,
    on_commit: Option<CommitHandler>,
    on_cancel: Option<CancelHandler>,
}

impl TextField {
    pub fn new(placeholder: &str, _cx: &mut Context<Self>) -> Self {
        Self {
            value: String::new(),
            placeholder: placeholder.to_string(),
            focus_handle: _cx.focus_handle(),
            cursor: 0,
            selection: None,
            on_commit: None,
            on_cancel: None,
        }
    }

    pub fn new_value(value: &str, placeholder: &str, cx: &mut Context<Self>) -> Self {
        let mut field = Self::new(placeholder, cx);
        field.value = value.to_string();
        field.cursor = value.len();
        field
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn set_value(&mut self, value: String) {
        self.cursor = value.len().clamp(0, self.cursor.max(value.len()));
        self.cursor = value.len();
        self.value = value;
        self.selection = None;
    }

    pub fn set_on_commit(&mut self, handler: impl Fn(&str, &mut Window, &mut App) + 'static) {
        self.on_commit = Some(Box::new(handler));
    }

    pub fn set_on_cancel(&mut self, handler: impl Fn(&mut Window, &mut App) + 'static) {
        self.on_cancel = Some(Box::new(handler));
    }

    fn selected_range(&self) -> Option<std::ops::Range<usize>> {
        self.selection.clone().filter(|range| !range.is_empty())
    }

    fn insert_text(&mut self, text: &str) {
        if let Some(range) = self.selected_range() {
            let start = range.start;
            self.value.replace_range(range, text);
            self.cursor = start + text.len();
        } else {
            let byte = self.value.len().min(self.cursor);
            self.value.insert_str(byte, text);
            self.cursor = byte + text.len();
        }
        self.selection = None;
    }

    fn delete_backward(&mut self) {
        if let Some(range) = self.selected_range() {
            let start = range.start;
            self.value.replace_range(range, "");
            self.cursor = start;
        } else if self.cursor > 0 {
            let mut start = self.cursor - 1;
            while !self.value.is_char_boundary(start) {
                start -= 1;
            }
            self.value.replace_range(start..self.cursor, "");
            self.cursor = start;
        }
        self.selection = None;
    }

    fn delete_forward(&mut self) {
        if let Some(range) = self.selected_range() {
            let start = range.start;
            self.value.replace_range(range, "");
            self.cursor = start;
        } else if self.cursor < self.value.len() {
            let mut end = self.cursor + 1;
            while end < self.value.len() && !self.value.is_char_boundary(end) {
                end += 1;
            }
            self.value.replace_range(self.cursor..end, "");
        }
        self.selection = None;
    }

    fn move_cursor(&mut self, offset: isize, select: bool) {
        let new_cursor =
            (self.cursor as isize + offset).clamp(0, self.value.len() as isize) as usize;
        if select {
            let anchor = self
                .selection
                .as_ref()
                .map(|range| range.start)
                .unwrap_or(self.cursor);
            self.selection = Some(anchor.min(new_cursor)..anchor.max(new_cursor));
        } else {
            self.selection = None;
        }
        self.cursor = new_cursor;
    }

    fn select_all(&mut self) {
        self.selection = Some(0..self.value.len());
        self.cursor = self.value.len();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let keystroke: &Keystroke = &event.keystroke;
        let modifiers = &keystroke.modifiers;
        let key = keystroke.key.as_str();

        if modifiers.secondary() {
            match key {
                k if k.eq_ignore_ascii_case("a") => {
                    self.select_all();
                }
                k if k.eq_ignore_ascii_case("c") => {
                    if let Some(range) = self.selected_range() {
                        let text = self
                            .value
                            .get(range.clone())
                            .unwrap_or_default()
                            .to_string();
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                    }
                }
                k if k.eq_ignore_ascii_case("x") => {
                    if let Some(range) = self.selected_range() {
                        let text = self
                            .value
                            .get(range.clone())
                            .unwrap_or_default()
                            .to_string();
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                        let start = range.start;
                        self.value.replace_range(range, "");
                        self.cursor = start;
                        self.selection = None;
                    }
                }
                k if k.eq_ignore_ascii_case("v") => {
                    if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                        self.insert_text(&text.replace('\n', " "));
                    }
                }
                _ => return,
            }
            window.prevent_default();
            cx.stop_propagation();
            cx.notify();
            return;
        }

        match key {
            "enter" | "return" => {
                if let Some(commit) = &self.on_commit {
                    commit(&self.value.clone(), window, cx);
                }
                window.prevent_default();
                cx.stop_propagation();
            }
            "escape" => {
                if let Some(cancel) = &self.on_cancel {
                    cancel(window, cx);
                }
                window.prevent_default();
                cx.stop_propagation();
            }
            "backspace" => self.delete_backward(),
            "delete" | "forwarddelete" => self.delete_forward(),
            "left" => self.move_cursor(-1, modifiers.shift),
            "right" => self.move_cursor(1, modifiers.shift),
            "home" => self.move_cursor(-(self.cursor as isize), modifiers.shift),
            "end" => self.move_cursor((self.value.len() - self.cursor) as isize, modifiers.shift),
            _ => {
                let Some(text) = keystroke.key_char.as_deref() else {
                    return;
                };
                if text.chars().all(|c| !c.is_control()) {
                    self.insert_text(text);
                } else {
                    return;
                }
            }
        }
        window.prevent_default();
        cx.stop_propagation();
        cx.notify();
    }
}

impl Focusable for TextField {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TextField {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = crate::CurrentTheme::get(cx);
        let value = self.value.clone();
        let has_value = !value.is_empty();
        let selection = self.selected_range();
        let focused = self.focus_handle.is_focused(window);

        // Render text with selection highlight by splitting into runs.
        let before;
        let mut selected = String::new();
        let mut after = String::new();
        match selection.filter(|range| !range.is_empty()) {
            Some(range) => {
                let (start, end) = (range.start.min(value.len()), range.end.min(value.len()));
                before = value.get(0..start).unwrap_or_default().to_string();
                selected = value.get(start..end).unwrap_or_default().to_string();
                after = value.get(end..).unwrap_or_default().to_string();
            }
            None => {
                before = value.clone();
            }
        }

        let prefix_width = px(before.chars().map(|_| 7.0).sum::<f32>());

        div()
            .id("text-field")
            .key_context("TextField")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .flex_1()
            .min_w_0()
            .px(px(8.0))
            .py(px(4.0))
            .rounded_md()
            .bg(theme.panel_bg)
            .border_1()
            .border_color(if focused {
                theme.accent
            } else {
                theme.border_color
            })
            .text_size(px(12.5))
            .text_color(theme.text_main)
            .flex()
            .items_center()
            .overflow_hidden()
            .when(!has_value && !focused, |this| {
                this.text_color(theme.text_sub)
                    .child(self.placeholder.clone())
            })
            .when(has_value || focused, |this| {
                this.child(
                    div()
                        .flex()
                        .items_center()
                        .relative()
                        .child(div().child(before.clone()))
                        .when(!selected.is_empty(), |this| {
                            this.child(
                                div()
                                    .bg(theme.selected_bg)
                                    .rounded_xs()
                                    .child(selected.clone()),
                            )
                        })
                        .child(div().child(after.clone()))
                        .when(focused, move |this| {
                            this.child(
                                div()
                                    .absolute()
                                    .left(prefix_width)
                                    .top(px(2.0))
                                    .bottom(px(2.0))
                                    .w(px(1.0))
                                    .bg(theme.terminal_cursor),
                            )
                        }),
                )
            })
    }
}
