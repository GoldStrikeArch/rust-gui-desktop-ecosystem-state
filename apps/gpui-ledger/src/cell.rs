//! Single-line text editor state for one table cell.
//!
//! gpui 0.2.2 ships **no text-input widget**. The corpus' existing options are
//! (a) the ~40-line `on_key_down` field used by `apps/gpui-grid` /
//! `apps/gpui-board` (append + backspace only, caret pinned to the end), or
//! (b) `apps/gpui-tray/src/editor.rs`, an 836-line `EntityInputHandler`
//! implementation with real IME, selection and clipboard. (b) is per-widget,
//! and this app has 74 focusable cells, so it was rejected on size.
//!
//! This is (a) extended to what a numeric form actually needs, in ~110 lines:
//! a byte caret with Left/Right/Home/End, Delete as well as Backspace, and a
//! per-field character filter applied *while typing* (the SPEC-10 "typing
//! accepts only characters that can still form a valid number" requirement).
//!
//! Rendering trick that avoids any text measurement: the view draws the buffer
//! as two adjacent `div`s (`before_caret`, `after_caret`) with a 1.5 px caret
//! `div` between them, so a moving caret needs no shaping API.
//!
//! Deliberately absent (recorded in FRICTION.md): selection, word motion,
//! IME/marked text, per-field undo, double-click word select.

/// What a keystroke may still be able to become.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    /// Anything (Description).
    Text,
    /// Digits only, capped length (Qty, and the digits of a date mask).
    Digits(usize),
    /// Optional leading '-', digits, one decimal separator, optional grouping.
    Number { decimal: char, group: char },
}

pub struct Cell {
    pub text: String,
    /// Caret position as a byte offset into `text`.
    pub caret: usize,
}

impl Cell {
    pub fn new(text: String) -> Self {
        let caret = text.len();
        Self { text, caret }
    }

    pub fn before(&self) -> &str {
        &self.text[..self.caret]
    }
    pub fn after(&self) -> &str {
        &self.text[self.caret..]
    }

    fn prev_boundary(&self) -> usize {
        let mut i = self.caret;
        while i > 0 {
            i -= 1;
            if self.text.is_char_boundary(i) {
                break;
            }
        }
        i
    }
    fn next_boundary(&self) -> usize {
        let mut i = self.caret;
        while i < self.text.len() {
            i += 1;
            if self.text.is_char_boundary(i) {
                break;
            }
        }
        i
    }

    pub fn left(&mut self) {
        self.caret = self.prev_boundary();
    }
    pub fn right(&mut self) {
        self.caret = self.next_boundary();
    }
    pub fn home(&mut self) {
        self.caret = 0;
    }
    pub fn end(&mut self) {
        self.caret = self.text.len();
    }

    pub fn backspace(&mut self) -> bool {
        if self.caret == 0 {
            return false;
        }
        let p = self.prev_boundary();
        self.text.replace_range(p..self.caret, "");
        self.caret = p;
        true
    }

    pub fn delete(&mut self) -> bool {
        if self.caret >= self.text.len() {
            return false;
        }
        let n = self.next_boundary();
        self.text.replace_range(self.caret..n, "");
        true
    }

    /// Insert `s` if every char survives the filter *and* the result is still
    /// a prefix of a valid value. Returns false if the input was rejected.
    pub fn insert(&mut self, s: &str, f: Filter) -> bool {
        for ch in s.chars() {
            if !self.accepts(ch, f) {
                return false;
            }
            self.text.insert(self.caret, ch);
            self.caret += ch.len_utf8();
        }
        true
    }

    fn accepts(&self, ch: char, f: Filter) -> bool {
        match f {
            Filter::Text => !ch.is_control(),
            Filter::Digits(max) => ch.is_ascii_digit() && self.text.chars().count() < max,
            Filter::Number { decimal, group } => {
                if ch.is_ascii_digit() {
                    return true;
                }
                if ch == '-' {
                    // only as the very first character
                    return self.caret == 0 && !self.text.starts_with('-');
                }
                if ch == decimal {
                    return !self.text.contains(decimal);
                }
                if ch == group {
                    return self.caret > 0;
                }
                false
            }
        }
    }
}
