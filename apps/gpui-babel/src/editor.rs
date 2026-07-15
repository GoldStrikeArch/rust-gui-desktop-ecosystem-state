//! Hand-rolled MULTILINE plain-text editor for gpui 0.2.2.
//!
//! gpui ships no text-input widget. This extends the officially sanctioned
//! approach (the bundled `examples/input.rs`, a 746-line single-line input)
//! to multiple logical lines:
//! - `EntityInputHandler` gives OS text-input integration (typing, IME marked
//!   text / CJK composition, dictation) — the same protocol Zed uses,
//! - actions + key bindings (context "Editor") give caret movement, selection
//!   (shift+arrows), Home/End, select-all, clipboard, backspace/delete,
//! - a custom `Element` shapes each logical line via `TextSystem::shape_line`
//!   and paints selection quads, the text runs, the caret, and the IME
//!   underline itself,
//! - grapheme-cluster boundaries (unicode-segmentation, same as input.rs) so
//!   arrows/backspace treat 👨‍👩‍👧‍👦 as one unit.
//!
//! Deliberate limits (recorded in FRICTION.md): no soft-wrap (logical lines
//! only, long lines clip), no word/line double-click selection, no
//! scroll-caret-into-view, caret does not blink, no undo.

#![allow(dead_code)]

use std::ops::Range;

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, FocusHandle, Focusable, GlobalElementId, Hsla, KeyBinding, LayoutId,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point,
    ShapedLine, SharedString, Style, TextRun, UTF16Selection, UnderlineStyle, Window, actions,
    div, fill, point, prelude::*, px, relative, rgba, size,
};
use unicode_segmentation::UnicodeSegmentation;

actions!(
    editor,
    [
        Backspace,
        Delete,
        Newline,
        Left,
        Right,
        Up,
        Down,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        SelectAll,
        Home,
        End,
        Copy,
        Cut,
        Paste,
        ShowCharacterPalette,
    ]
);

/// Key bindings for the "Editor" context. Call once at app startup.
pub fn bind_editor_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, Some("Editor")),
        KeyBinding::new("delete", Delete, Some("Editor")),
        KeyBinding::new("enter", Newline, Some("Editor")),
        KeyBinding::new("left", Left, Some("Editor")),
        KeyBinding::new("right", Right, Some("Editor")),
        KeyBinding::new("up", Up, Some("Editor")),
        KeyBinding::new("down", Down, Some("Editor")),
        KeyBinding::new("shift-left", SelectLeft, Some("Editor")),
        KeyBinding::new("shift-right", SelectRight, Some("Editor")),
        KeyBinding::new("shift-up", SelectUp, Some("Editor")),
        KeyBinding::new("shift-down", SelectDown, Some("Editor")),
        KeyBinding::new("cmd-a", SelectAll, Some("Editor")),
        KeyBinding::new("home", Home, Some("Editor")),
        KeyBinding::new("end", End, Some("Editor")),
        KeyBinding::new("cmd-left", Home, Some("Editor")),
        KeyBinding::new("cmd-right", End, Some("Editor")),
        KeyBinding::new("cmd-c", Copy, Some("Editor")),
        KeyBinding::new("cmd-x", Cut, Some("Editor")),
        KeyBinding::new("cmd-v", Paste, Some("Editor")),
        KeyBinding::new("ctrl-cmd-space", ShowCharacterPalette, Some("Editor")),
    ]);
}

/// Layout captured at paint time, used for mouse mapping and caret geometry.
struct EditorLayout {
    /// One shaped line per logical (\n-separated) line.
    lines: Vec<ShapedLine>,
    /// Byte offset of the start of each logical line in `content`.
    line_starts: Vec<usize>,
    bounds: Bounds<Pixels>,
    line_height: Pixels,
}

pub struct Editor {
    pub focus_handle: FocusHandle,
    pub content: String,
    placeholder: SharedString,
    /// Selection as byte offsets into `content` (caret = empty range).
    selected_range: Range<usize>,
    selection_reversed: bool,
    /// IME composition range (underlined).
    marked_range: Option<Range<usize>>,
    last_layout: Option<EditorLayout>,
    is_selecting: bool,
    /// Preferred x for up/down runs through short lines.
    goal_x: Option<Pixels>,
}

impl Editor {
    pub fn new(cx: &mut Context<Self>, initial: &str, placeholder: &str) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            content: initial.to_string(),
            placeholder: SharedString::from(placeholder.to_string()),
            selected_range: initial.len()..initial.len(),
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            is_selecting: false,
            goal_x: None,
        }
    }

    pub fn set_content(&mut self, text: &str, cx: &mut Context<Self>) {
        self.content = text.to_string();
        self.selected_range = 0..0;
        self.selection_reversed = false;
        self.marked_range = None;
        self.goal_x = None;
        cx.notify();
    }

    // ---- line bookkeeping -------------------------------------------------

    /// Byte offset of the start of each logical line.
    fn line_starts(&self) -> Vec<usize> {
        let mut starts = vec![0];
        for (ix, b) in self.content.bytes().enumerate() {
            if b == b'\n' {
                starts.push(ix + 1);
            }
        }
        starts
    }

    /// (line index, line byte range excluding '\n') containing `offset`.
    fn line_of_offset(&self, offset: usize) -> (usize, Range<usize>) {
        let starts = self.line_starts();
        let line_ix = match starts.binary_search(&offset) {
            Ok(ix) => ix,
            Err(ix) => ix - 1,
        };
        let start = starts[line_ix];
        let end = starts
            .get(line_ix + 1)
            .map(|next| next - 1) // strip '\n'
            .unwrap_or(self.content.len());
        (line_ix, start..end)
    }

    // ---- caret / selection ------------------------------------------------

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        cx.notify();
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        cx.notify();
    }

    /// Previous grapheme-cluster boundary (treats 👨‍👩‍👧‍👦 as one unit).
    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    /// Next grapheme-cluster boundary.
    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(idx, _)| (idx > offset).then_some(idx))
            .unwrap_or(self.content.len())
    }

    /// Offset one visual line up/down from the caret, keeping the goal x.
    fn vertical_target(&mut self, dir: i32) -> Option<usize> {
        let layout = self.last_layout.as_ref()?;
        let offset = self.cursor_offset();
        let (line_ix, line_range) = self.line_of_offset(offset);
        let x = self
            .goal_x
            .unwrap_or_else(|| layout.lines[line_ix].x_for_index(offset - line_range.start));
        self.goal_x = Some(x);
        let target_ix = line_ix as i64 + dir as i64;
        if target_ix < 0 {
            return Some(0);
        }
        if target_ix as usize >= layout.lines.len() {
            return Some(self.content.len());
        }
        let target_ix = target_ix as usize;
        let within = layout.lines[target_ix].closest_index_for_x(x);
        Some(layout.line_starts[target_ix] + within)
    }

    // ---- action handlers ----------------------------------------------------

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx);
        }
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.selected_range.end), cx);
        } else {
            self.move_to(self.selected_range.end, cx);
        }
    }

    fn up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(offset) = self.vertical_target(-1) {
            self.move_to(offset, cx);
        }
    }

    fn down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(offset) = self.vertical_target(1) {
            self.move_to(offset, cx);
        }
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(offset) = self.vertical_target(-1) {
            self.select_to(offset, cx);
        }
    }

    fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(offset) = self.vertical_target(1) {
            self.select_to(offset, cx);
        }
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx);
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        let (_, range) = self.line_of_offset(self.cursor_offset());
        self.move_to(range.start, cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        let (_, range) = self.line_of_offset(self.cursor_offset());
        self.move_to(range.end, cx);
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        if self.selected_range.is_empty() {
            self.select_to(self.previous_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        if self.selected_range.is_empty() {
            self.select_to(self.next_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn newline(&mut self, _: &Newline, window: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        self.replace_text_in_range(None, "\n", window, cx);
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
            self.replace_text_in_range(None, "", window, cx);
        }
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        // Multiline editor: keep newlines (input.rs replaced them with spaces).
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace_text_in_range(None, &text, window, cx);
        }
    }

    fn show_character_palette(
        &mut self,
        _: &ShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    // ---- mouse -----------------------------------------------------------

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.content.is_empty() {
            return 0;
        }
        let Some(layout) = self.last_layout.as_ref() else {
            return 0;
        };
        if position.y < layout.bounds.top() {
            return 0;
        }
        if position.y >= layout.bounds.top() + layout.line_height * layout.lines.len() as f32 {
            return self.content.len();
        }
        let row = (f32::from((position.y - layout.bounds.top()) / layout.line_height)) as usize;
        let row = row.min(layout.lines.len() - 1);
        let within = layout.lines[row].closest_index_for_x(position.x - layout.bounds.left());
        layout.line_starts[row] + within
    }

    fn on_mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.is_selecting = true;
        self.goal_x = None;
        if !self.focus_handle.is_focused(window) {
            self.focus_handle.focus(window);
        }
        let index = self.index_for_mouse_position(event.position);
        if event.modifiers.shift {
            self.select_to(index, cx);
        } else {
            self.move_to(index, cx);
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            let index = self.index_for_mouse_position(event.position);
            self.select_to(index, cx);
        }
    }

    // ---- UTF-16 conversion (required by EntityInputHandler) ---------------

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;
        for ch in self.content.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }
        utf8_offset
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;
        for ch in self.content.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }
        utf16_offset
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range_utf16.start)..self.offset_from_utf16(range_utf16.end)
    }
}

impl EntityInputHandler for Editor {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.content =
            self.content[0..range.start].to_owned() + new_text + &self.content[range.end..];
        self.selected_range = range.start + new_text.len()..range.start + new_text.len();
        self.marked_range.take();
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.content =
            self.content[0..range.start].to_owned() + new_text + &self.content[range.end..];
        if !new_text.is_empty() {
            self.marked_range = Some(range.start..range.start + new_text.len());
        } else {
            self.marked_range = None;
        }
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .map(|new_range| new_range.start + range.start..new_range.end + range.end)
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        // Used by the OS to position the IME candidate window.
        let layout = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        let (line_ix, line_range) = self.line_of_offset(range.start);
        let line = layout.lines.get(line_ix)?;
        let y = element_bounds.top() + layout.line_height * line_ix as f32;
        let start_x =
            element_bounds.left() + line.x_for_index(range.start - line_range.start);
        let end_in_line = range.end.min(line_range.end) - line_range.start;
        let end_x = element_bounds.left() + line.x_for_index(end_in_line);
        Some(Bounds::from_corners(
            point(start_x, y),
            point(end_x, y + layout.line_height),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let index = self.index_for_mouse_position(point);
        Some(self.offset_to_utf16(index))
    }
}

// ---------------------------------------------------------------------------
// The element that lays out and paints the editor content.
// ---------------------------------------------------------------------------

pub struct EditorElement {
    editor: Entity<Editor>,
}

pub struct EditorPrepaint {
    lines: Vec<ShapedLine>,
    line_starts: Vec<usize>,
    line_height: Pixels,
    selection_quads: Vec<PaintQuad>,
    caret: Option<PaintQuad>,
}

impl IntoElement for EditorElement {
    type Element = Self;
    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for EditorElement {
    type RequestLayoutState = ();
    type PrepaintState = EditorPrepaint;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let line_count = self.editor.read(cx).content.split('\n').count().max(1);
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = (window.line_height() * line_count as f32).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let editor = self.editor.read(cx);
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line_height = window.line_height();

        let content = editor.content.clone();
        let selected_range = editor.selected_range.clone();
        let marked_range = editor.marked_range.clone();
        let cursor_offset = editor.cursor_offset();
        let show_placeholder = content.is_empty() && !editor.placeholder.is_empty();

        let text_color: Hsla = if show_placeholder {
            style.color.opacity(0.35)
        } else {
            style.color
        };

        let display: String = if show_placeholder {
            editor.placeholder.to_string()
        } else {
            content.clone()
        };

        let mut lines = Vec::new();
        let mut line_starts = Vec::new();
        let mut selection_quads = Vec::new();
        let mut caret = None;
        let mut start = 0usize;

        for (line_ix, line_text) in display.split('\n').enumerate() {
            let line_len = line_text.len();
            let line_range = start..start + line_len;
            line_starts.push(start);
            let y = bounds.top() + line_height * line_ix as f32;

            // Build runs: base + IME underline where marked_range overlaps.
            let base_run = |len: usize| TextRun {
                len,
                font: style.font(),
                color: text_color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let runs: Vec<TextRun> = match (show_placeholder, &marked_range) {
                (false, Some(marked)) if marked.start < line_range.end && marked.end > line_range.start => {
                    let m_start = marked.start.max(line_range.start) - line_range.start;
                    let m_end = marked.end.min(line_range.end) - line_range.start;
                    let mut runs = Vec::new();
                    if m_start > 0 {
                        runs.push(base_run(m_start));
                    }
                    runs.push(TextRun {
                        underline: Some(UnderlineStyle {
                            color: Some(text_color),
                            thickness: px(1.),
                            wavy: false,
                        }),
                        ..base_run(m_end - m_start)
                    });
                    if line_len > m_end {
                        runs.push(base_run(line_len - m_end));
                    }
                    runs
                }
                _ => vec![base_run(line_len)],
            };

            let shaped = window.text_system().shape_line(
                SharedString::from(line_text.to_string()),
                font_size,
                &runs,
                None,
            );

            if !show_placeholder {
                // Selection quad(s) for this line.
                if !selected_range.is_empty()
                    && selected_range.start < line_range.end + 1
                    && selected_range.end > line_range.start
                {
                    let sel_start = selected_range.start.max(line_range.start) - line_range.start;
                    let sel_end = selected_range.end.min(line_range.end) - line_range.start;
                    let x0 = shaped.x_for_index(sel_start);
                    let mut x1 = shaped.x_for_index(sel_end);
                    // If the selection continues past this line's newline, show
                    // a small stub so empty/fully-selected lines are visible.
                    if selected_range.end > line_range.end {
                        x1 += px(4.);
                    }
                    let (lo, hi) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
                    selection_quads.push(fill(
                        Bounds::from_corners(
                            point(bounds.left() + lo, y),
                            point(bounds.left() + hi.max(lo + px(2.)), y + line_height),
                        ),
                        rgba(0x3b82f655),
                    ));
                }

                // Caret.
                if selected_range.is_empty()
                    && cursor_offset >= line_range.start
                    && cursor_offset <= line_range.end
                {
                    let x = shaped.x_for_index(cursor_offset - line_range.start);
                    caret = Some(fill(
                        Bounds::new(
                            point(bounds.left() + x, y + px(1.)),
                            size(px(2.), line_height - px(2.)),
                        ),
                        text_color,
                    ));
                }
            }

            lines.push(shaped);
            start += line_len + 1; // +1 for '\n'
        }

        if show_placeholder {
            // Caret at origin when empty.
            caret = Some(fill(
                Bounds::new(
                    point(bounds.left(), bounds.top() + px(1.)),
                    size(px(2.), line_height - px(2.)),
                ),
                style.color,
            ));
            line_starts = vec![0];
        }

        EditorPrepaint {
            lines,
            line_starts,
            line_height,
            selection_quads,
            caret,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.editor.read(cx).focus_handle.clone();
        // Register this element as the OS text-input target (IME etc.).
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.editor.clone()),
            cx,
        );

        for quad in prepaint.selection_quads.drain(..) {
            window.paint_quad(quad);
        }
        for (ix, line) in prepaint.lines.iter().enumerate() {
            let origin = point(
                bounds.left(),
                bounds.top() + prepaint.line_height * ix as f32,
            );
            line.paint(origin, prepaint.line_height, window, cx).ok();
        }
        if focus_handle.is_focused(window)
            && let Some(caret) = prepaint.caret.take()
        {
            window.paint_quad(caret);
        }

        let lines = std::mem::take(&mut prepaint.lines);
        let line_starts = std::mem::take(&mut prepaint.line_starts);
        let line_height = prepaint.line_height;
        self.editor.update(cx, |editor, _| {
            editor.last_layout = Some(EditorLayout {
                lines,
                line_starts,
                bounds,
                line_height,
            });
        });
    }
}

impl Render for Editor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context("Editor")
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::newline))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::show_character_palette))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .w_full()
            .child(EditorElement { editor: cx.entity() })
    }
}

impl Focusable for Editor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
