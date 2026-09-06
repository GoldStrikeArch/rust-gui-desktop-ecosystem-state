//! "Ledger" — business-forms / numeric-input probe for gpui 0.2.2 (SPEC-10).
//!
//! What gpui gives us for a form:
//! - **tab order**: `cx.focus_handle().tab_index(n).tab_stop(true)` +
//!   `window.focus_next()/focus_prev()` (bundled `examples/tab_stop.rs`) — a
//!   real tab-stop map, sorted by index, maintained across frames.
//! - **font features**: `Styled::font(Font { features: FontFeatures(vec![("tnum",1)]), .. })`
//!   reaches CoreText (`platform/mac/open_type.rs::apply_features_and_fallbacks`),
//!   so tabular figures are a first-class request. Measured in the self-test.
//! - **clipboard**: `cx.read_from_clipboard()` / `write_to_clipboard`.
//! - **focus ring / hit testing / layout**: styled `div()`s, as everywhere else.
//!
//! What it does not give us, and is hand-rolled here:
//! - any text input at all (`src/cell.rs`, ~110 lines: caret, filtered typing),
//! - number parsing/formatting and locale (~90 lines, no `icu`/`rust_decimal`;
//!   money is `i64` minor units so every total is exact),
//! - dropdown, date mask, slider, checkbox, validation, undo, TSV clipboard,
//! - accessibility: gpui 0.2.2 has **no** AccessKit and no NSAccessibility
//!   implementation, so the window exposes nothing to the OS a11y tree.
//!
//! `LEDGER_SELFTEST=1` drives the same functions the UI events call and prints
//! `SELFTEST DONE pass=N fail=M`, then exits.

mod cell;

use std::time::Duration;

use cell::{Cell, Filter};
use gpui::{
    App, Application, Bounds, ClickEvent, ClipboardItem, Context, DragMoveEvent, FocusHandle, Font,
    FontFeatures, KeyDownEvent, SharedString, TextRun, TitlebarOptions, Window, WindowBounds,
    WindowOptions, div, prelude::*, px, rgb, rgba, size,
};

// ---------------------------------------------------------------------------
// Locale + money.  Money is i64 minor units end to end (no float anywhere).
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Locale {
    EnUs,
    FrFr,
}

impl Locale {
    fn decimal(self) -> char {
        match self {
            Locale::EnUs => '.',
            Locale::FrFr => ',',
        }
    }
    fn group(self) -> char {
        match self {
            Locale::EnUs => ',',
            Locale::FrFr => ' ',
        }
    }
    fn label(self) -> &'static str {
        match self {
            Locale::EnUs => "en-US",
            Locale::FrFr => "fr-FR",
        }
    }
}

/// Group the digits of `s` (an unsigned integer string) in threes.
fn group_digits(s: &str, sep: char) -> String {
    let mut out = String::new();
    let n = s.len();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (n - i) % 3 == 0 {
            out.push(sep);
        }
        out.push(c);
    }
    out
}

/// `123456` -> `1,234.56` / `1 234,56`. Always two fraction digits, so a
/// right-aligned column with tabular figures lines up on the separator.
fn fmt_money(v: i64, l: Locale) -> String {
    let neg = v < 0;
    let a = v.unsigned_abs();
    let int = group_digits(&(a / 100).to_string(), l.group());
    format!("{}{}{}{:02}", if neg { "-" } else { "" }, int, l.decimal(), a % 100)
}

fn fmt_int(v: i64, l: Locale) -> String {
    let neg = v < 0;
    let int = group_digits(&v.unsigned_abs().to_string(), l.group());
    format!("{}{}", if neg { "-" } else { "" }, int)
}

/// Accepting parser: strips currency symbols and any kind of space, recognises
/// the accounting negative `(12.50)`, and works out which of `.` / `,` is the
/// decimal separator (the *last* one when both appear, otherwise the locale's
/// unless a grouping interpretation is the only sane one).
fn parse_money(src: &str, l: Locale) -> Option<i64> {
    let mut s: String = src
        .chars()
        .filter(|c| !matches!(c, '$' | '€' | '£' | '¥' | ' ' | '\u{a0}' | '\u{202f}' | '\t'))
        .collect();
    if s.is_empty() {
        return None;
    }
    let mut neg = false;
    if s.starts_with('(') && s.ends_with(')') {
        neg = true;
        s = s[1..s.len() - 1].to_string();
    }
    if let Some(r) = s.strip_prefix('-') {
        neg = !neg;
        s = r.to_string();
    } else if let Some(r) = s.strip_prefix('+') {
        s = r.to_string();
    }
    if s.is_empty() || !s.chars().all(|c| c.is_ascii_digit() || c == '.' || c == ',') {
        return None;
    }
    let (dot, comma) = (s.rfind('.'), s.rfind(','));
    let dec = match (dot, comma) {
        (Some(d), Some(c)) => Some(d.max(c)),
        (Some(d), None) => (l.decimal() == '.' || s.len() - d - 1 != 3).then_some(d),
        (None, Some(c)) => (l.decimal() == ',' || s.len() - c - 1 != 3).then_some(c),
        (None, None) => None,
    };
    let (int_part, frac) = match dec {
        Some(p) => (&s[..p], &s[p + 1..]),
        None => (&s[..], ""),
    };
    if !frac.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let digits: String = int_part.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() && frac.is_empty() {
        return None;
    }
    let units: i64 = if digits.is_empty() { 0 } else { digits.parse().ok()? };
    // pad / round the fraction to exactly two places
    let cents: i64 = match frac.len() {
        0 => 0,
        1 => frac.parse::<i64>().ok()? * 10,
        2 => frac.parse().ok()?,
        _ => {
            let two: i64 = frac[..2].parse().ok()?;
            let next = frac.as_bytes()[2] - b'0';
            two + if next >= 5 { 1 } else { 0 }
        }
    };
    let v = units.checked_mul(100)?.checked_add(cents)?;
    Some(if neg { -v } else { v })
}

fn parse_int(src: &str) -> Option<i64> {
    let digits: String = src.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() || src.chars().any(|c| !c.is_ascii_digit() && !matches!(c, ',' | ' ' | '.' | '\u{a0}')) {
        return None;
    }
    digits.parse().ok()
}

fn valid_date(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    let num = |r: std::ops::Range<usize>| s[r].parse::<u32>().ok();
    let (Some(y), Some(m), Some(d)) = (num(0..4), num(5..7), num(8..10)) else {
        return false;
    };
    if !(1..=12).contains(&m) || d == 0 {
        return false;
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let dim = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    d <= dim[(m - 1) as usize]
}

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

const CATEGORIES: [&str; 6] = ["Travel", "Meals", "Hardware", "Software", "Lodging", "Other"];
const ROWS: usize = 12;

#[derive(Clone, PartialEq)]
struct Row {
    date: String,
    desc: String,
    cat: usize,
    qty: i64,
    price: i64, // minor units, may be negative (refunds)
    reimb: bool,
}

impl Row {
    fn amount(&self) -> i64 {
        self.qty * self.price
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Field {
    Date,
    Desc,
    Cat,
    Qty,
    Price,
    Reimb,
}

const FIELDS: [Field; 6] = [Field::Date, Field::Desc, Field::Cat, Field::Qty, Field::Price, Field::Reimb];

impl Field {
    fn label(self) -> &'static str {
        match self {
            Field::Date => "Date",
            Field::Desc => "Description",
            Field::Cat => "Category",
            Field::Qty => "Qty",
            Field::Price => "Unit price",
            Field::Reimb => "Reimb",
        }
    }
    fn width(self) -> f32 {
        match self {
            Field::Date => 104.,
            Field::Desc => 196.,
            Field::Cat => 118.,
            Field::Qty => 54.,
            Field::Price => 104.,
            Field::Reimb => 66.,
        }
    }
    fn numeric(self) -> bool {
        matches!(self, Field::Qty | Field::Price)
    }
}

/// Everything Tab can land on, in reading order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Target {
    LocaleBtn,
    Cell(usize, Field),
    Vat,
    Slider,
    Save,
}

#[derive(Clone)]
struct Snap {
    rows: Vec<Row>,
    vat: i64,
}

/// Typed drag payload for the VAT slider (gpui routes DnD by payload type).
#[derive(Clone)]
struct VatDrag;
struct NoGhost;
impl Render for NoGhost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

struct Ledger {
    rows: Vec<Row>,
    /// Focus handles in tab order.
    tab: Vec<(Target, FocusHandle)>,
    /// The target that currently has focus (synced from the focus handles at
    /// the top of every render, which is also where cell commits happen).
    active: Option<Target>,
    /// Text buffer for the active editable target.
    edit: Option<Cell>,
    /// (target, message) for a value that would not parse — the model keeps
    /// its previous value and the cell shows the offending text in red.
    bad: Option<(Target, String)>,
    locale: Locale,
    /// VAT percent × 100 (so 20.00 % is 2000). Range 0 ..= 2500.
    vat: i64,
    /// Row whose category dropdown is open, plus its type-ahead state.
    dd: Option<usize>,
    dd_query: String,
    dd_hl: usize,
    undo: Vec<Snap>,
    redo: Vec<Snap>,
    status: SharedString,
    /// Every focus transition, for the Tab-order walk evidence.
    focus_log: Vec<String>,
}

fn seed() -> Vec<Row> {
    let r = |date: &str, desc: &str, cat: usize, qty: i64, price: i64, reimb: bool| Row {
        date: date.into(),
        desc: desc.into(),
        cat,
        qty,
        price,
        reimb,
    };
    vec![
        r("2026-01-07", "Train Paris–Lyon", 0, 2, 8450, true),
        r("2026-01-08", "Team dinner", 1, 1, 21390, true),
        r("2026-01-09", "USB-C dock", 2, 1, 12999, false),
        r("2026-01-12", "Editor licence", 3, 3, 9900, true),
        r("2026-01-14", "Hotel Lyon", 4, 2, 13950, true),
        r("2026-01-15", "Airport taxi", 0, 1, 4275, true),
        r("2026-01-19", "Monitor arm", 2, 2, 6499, false),
        r("2026-01-21", "Refund: cancelled seat", 0, 1, -3400, true),
        r("2026-01-23", "Coffee with client", 1, 4, 385, false),
        r("2026-01-26", "Cloud storage", 3, 1, 1200, true),
        r("2026-01-28", "Conference ticket", 5, 1, 45000, true),
        r("2026-01-30", "Printer paper", 5, 5, 799, false),
    ]
}

// ---------------------------------------------------------------------------
// Tabular figures: the SPEC-10 font-feature question
// ---------------------------------------------------------------------------

fn tabular_font() -> Font {
    let mut f = gpui::font(".SystemUIFont");
    // OpenType feature request — reaches CoreText on macOS.
    f.features = FontFeatures(std::sync::Arc::new(vec![
        ("tnum".to_string(), 1),
        ("lnum".to_string(), 1),
    ]));
    f
}

// ---------------------------------------------------------------------------

impl Ledger {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut tab = vec![(Target::LocaleBtn, cx.focus_handle().tab_index(1).tab_stop(true))];
        for r in 0..ROWS {
            for (c, f) in FIELDS.iter().enumerate() {
                let ix = (10 + r * 6 + c) as isize;
                tab.push((Target::Cell(r, *f), cx.focus_handle().tab_index(ix).tab_stop(true)));
            }
        }
        for (i, t) in [Target::Vat, Target::Slider, Target::Save].into_iter().enumerate() {
            tab.push((t, cx.focus_handle().tab_index(900 + i as isize).tab_stop(true)));
        }
        tab[0].1.focus(window);
        if std::env::var("LEDGER_SELFTEST").is_ok() {
            Self::spawn_selftest(cx);
        }
        Self {
            rows: seed(),
            tab,
            active: None,
            edit: None,
            bad: None,
            locale: Locale::EnUs,
            vat: 2000,
            dd: None,
            dd_query: String::new(),
            dd_hl: 0,
            undo: Vec::new(),
            redo: Vec::new(),
            status: "Tab / Shift-Tab: next cell · Enter / Shift-Enter: down / up · ⌘Z undo · ⌘C/⌘V row TSV".into(),
            focus_log: Vec::new(),
        }
    }

    fn handle(&self, t: Target) -> &FocusHandle {
        &self.tab.iter().find(|(x, _)| *x == t).unwrap().1
    }

    // -- totals ------------------------------------------------------------

    fn subtotal(&self) -> i64 {
        self.rows.iter().map(|r| r.amount()).sum()
    }
    fn vat_amount(&self) -> i64 {
        // round-half-up on the minor unit; vat is percent × 100
        let n = self.subtotal() * self.vat;
        let (q, r) = (n / 10_000, n % 10_000);
        q + if r.abs() * 2 >= 10_000 { r.signum() } else { 0 }
    }
    fn total(&self) -> i64 {
        self.subtotal() + self.vat_amount()
    }

    // -- validation --------------------------------------------------------

    fn errors(&self) -> Vec<(usize, Field, String)> {
        let mut out = Vec::new();
        for (i, r) in self.rows.iter().enumerate() {
            if !valid_date(&r.date) {
                out.push((i, Field::Date, "date must be YYYY-MM-DD".into()));
            }
            let n = r.desc.chars().count();
            if n == 0 {
                out.push((i, Field::Desc, "description is required".into()));
            } else if n > 60 {
                out.push((i, Field::Desc, format!("description is {n} chars (max 60)")));
            }
            if !(1..=999).contains(&r.qty) {
                out.push((i, Field::Qty, "qty must be 1–999".into()));
            }
            if !(-9_999_999..=9_999_999).contains(&r.price) {
                out.push((i, Field::Price, "price must be ±99 999.99".into()));
            }
        }
        out
    }

    fn err_for(&self, row: usize, f: Field) -> Option<String> {
        if let Some((Target::Cell(r, bf), m)) = &self.bad
            && *r == row
            && *bf == f
        {
            return Some(m.clone());
        }
        self.errors()
            .into_iter()
            .find(|(r, x, _)| *r == row && *x == f)
            .map(|(_, _, m)| m)
    }

    // -- text of a cell as the model sees it -------------------------------

    fn cell_text(&self, row: usize, f: Field) -> String {
        let r = &self.rows[row];
        match f {
            Field::Date => r.date.clone(),
            Field::Desc => r.desc.clone(),
            Field::Cat => CATEGORIES[r.cat].to_string(),
            Field::Qty => fmt_int(r.qty, self.locale),
            Field::Price => fmt_money(r.price, self.locale),
            Field::Reimb => if r.reimb { "yes" } else { "no" }.into(),
        }
    }

    fn snapshot(&mut self) {
        self.undo.push(Snap {
            rows: self.rows.clone(),
            vat: self.vat,
        });
        self.redo.clear();
        if self.undo.len() > 64 {
            self.undo.remove(0);
        }
    }

    // -- focus sync: commit the cell we left, start editing the one we entered

    fn sync_focus(&mut self, window: &Window, cx: &mut Context<Self>) {
        let now = self
            .tab
            .iter()
            .find(|(_, h)| h.is_focused(window))
            .map(|(t, _)| *t);
        if now == self.active {
            return;
        }
        if let Some(prev) = self.active {
            self.commit(prev, cx);
        }
        self.active = now;
        self.dd = None;
        self.edit = match now {
            Some(Target::Cell(r, f)) if f != Field::Reimb && f != Field::Cat => {
                Some(Cell::new(self.cell_text(r, f)))
            }
            Some(Target::Vat) => Some(Cell::new(fmt_money(self.vat, self.locale))),
            _ => None,
        };
        if let Some(t) = now {
            let line = match t {
                Target::Cell(r, f) => format!("FOCUS row{r} {:?}", f),
                other => format!("FOCUS {other:?}"),
            };
            println!("{line}");
            self.focus_log.push(line);
        }
    }

    /// Parse + store the buffer for `t`. Invalid input keeps the previous
    /// committed value and records an inline error.
    fn commit(&mut self, t: Target, cx: &mut Context<Self>) {
        let Some(buf) = self.edit.take() else { return };
        let text = buf.text.trim().to_string();
        let l = self.locale;
        let ok = match t {
            Target::Vat => match parse_money(&text, l) {
                Some(v) if (0..=2500).contains(&v) => {
                    if v != self.vat {
                        self.snapshot();
                        self.vat = v;
                    }
                    true
                }
                _ => false,
            },
            Target::Cell(r, Field::Date) => {
                if self.rows[r].date != text {
                    self.snapshot();
                    self.rows[r].date = text.clone();
                }
                valid_date(&text)
            }
            Target::Cell(r, Field::Desc) => {
                if self.rows[r].desc != text {
                    self.snapshot();
                    self.rows[r].desc = text.clone();
                }
                true
            }
            Target::Cell(r, Field::Qty) => match parse_int(&text) {
                Some(v) => {
                    if self.rows[r].qty != v {
                        self.snapshot();
                        self.rows[r].qty = v;
                    }
                    true
                }
                None => false,
            },
            Target::Cell(r, Field::Price) => match parse_money(&text, l) {
                Some(v) => {
                    if self.rows[r].price != v {
                        self.snapshot();
                        self.rows[r].price = v;
                    }
                    true
                }
                None => false,
            },
            _ => true,
        };
        self.bad = if ok {
            None
        } else {
            Some((t, format!("cannot read \u{201c}{text}\u{201d} as a number")))
        };
        cx.notify();
    }

    // -- keyboard ----------------------------------------------------------

    fn step(&mut self, up: bool, big: bool, cx: &mut Context<Self>) {
        let Some(t) = self.active else { return };
        let d = match t {
            Target::Cell(_, Field::Qty) => 1,
            Target::Cell(_, Field::Price) | Target::Vat => 1,
            _ => return,
        } * if big { 10 } else { 1 }
            * if up { 1 } else { -1 };
        self.snapshot();
        match t {
            Target::Cell(r, Field::Qty) => self.rows[r].qty = (self.rows[r].qty + d).max(0),
            Target::Cell(r, Field::Price) => self.rows[r].price += d,
            Target::Vat => self.vat = (self.vat + d).clamp(0, 2500),
            _ => {}
        }
        self.bad = None;
        self.edit = Some(Cell::new(match t {
            Target::Cell(r, Field::Qty) => fmt_int(self.rows[r].qty, self.locale),
            Target::Cell(r, Field::Price) => fmt_money(self.rows[r].price, self.locale),
            _ => fmt_money(self.vat, self.locale),
        }));
        cx.notify();
    }

    fn move_row(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Target::Cell(r, f)) = self.active else { return };
        let n = r as isize + delta;
        if !(0..ROWS as isize).contains(&n) {
            return;
        }
        self.commit(Target::Cell(r, f), cx);
        self.handle(Target::Cell(n as usize, f)).clone().focus(window);
        cx.notify();
    }

    fn undo(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.undo.pop() {
            self.redo.push(Snap {
                rows: self.rows.clone(),
                vat: self.vat,
            });
            self.rows = s.rows;
            self.vat = s.vat;
            self.edit = None;
            self.bad = None;
            self.status = format!("undo ({} left)", self.undo.len()).into();
            cx.notify();
        }
    }

    fn redo(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.redo.pop() {
            self.undo.push(Snap {
                rows: self.rows.clone(),
                vat: self.vat,
            });
            self.rows = s.rows;
            self.vat = s.vat;
            self.edit = None;
            self.bad = None;
            self.status = format!("redo ({} left)", self.redo.len()).into();
            cx.notify();
        }
    }

    fn row_tsv(&self, r: usize) -> String {
        let x = &self.rows[r];
        format!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            x.date,
            x.desc,
            CATEGORIES[x.cat],
            x.qty,
            fmt_money(x.price, self.locale),
            if x.reimb { "yes" } else { "no" }
        )
    }

    fn copy_row(&mut self, cx: &mut Context<Self>) {
        let Some(Target::Cell(r, _)) = self.active else { return };
        let tsv = self.row_tsv(r);
        cx.write_to_clipboard(ClipboardItem::new_string(tsv));
        self.status = format!("copied row {} as TSV", r + 1).into();
        cx.notify();
    }

    /// ⌘V: a tab-separated payload replaces the whole row (SPEC-10 §9);
    /// anything else is inserted into the focused field, where the field's
    /// own parser deals with `$1,234.56` / `1.234,56 €` / `(12.50)` (§2).
    fn paste_row(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|i| i.text()) else {
            return;
        };
        if !text.contains('\t') {
            if let Some(buf) = self.edit.as_mut() {
                buf.text = text.trim().to_string();
                buf.end();
                self.status = format!("pasted \u{201c}{}\u{201d} into the field", text.trim()).into();
                cx.notify();
            }
            return;
        }
        let Some(Target::Cell(r, _)) = self.active else { return };
        let f: Vec<&str> = text.trim_end_matches('\n').split('\t').collect();
        if f.len() < 5 {
            self.status = "clipboard is not a 6-column TSV row".into();
            cx.notify();
            return;
        }
        self.snapshot();
        let row = &mut self.rows[r];
        row.date = f[0].trim().to_string();
        row.desc = f[1].trim().to_string();
        if let Some(i) = CATEGORIES.iter().position(|c| c.eq_ignore_ascii_case(f[2].trim())) {
            row.cat = i;
        }
        let l = self.locale;
        if let Some(q) = parse_int(f[3]) {
            row.qty = q;
        }
        if let Some(p) = parse_money(f[4], l) {
            row.price = p;
        }
        if let Some(v) = f.get(5) {
            row.reimb = matches!(v.trim(), "yes" | "true" | "1");
        }
        self.edit = Some(Cell::new(self.cell_text(r, match self.active {
            Some(Target::Cell(_, fl)) if fl != Field::Cat && fl != Field::Reimb => fl,
            _ => Field::Desc,
        })));
        self.status = format!("pasted TSV into row {}", r + 1).into();
        cx.notify();
    }

    fn set_locale(&mut self, l: Locale, cx: &mut Context<Self>) {
        self.locale = l;
        // Re-render the buffer of whatever is being edited in the new locale.
        if let Some(t) = self.active {
            self.edit = match t {
                Target::Cell(r, f) if f == Field::Qty || f == Field::Price => {
                    Some(Cell::new(self.cell_text(r, f)))
                }
                Target::Vat => Some(Cell::new(fmt_money(self.vat, self.locale))),
                _ => self.edit.take(),
            };
        }
        println!("LOCALE {}", l.label());
        cx.notify();
    }

    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let ks = &ev.keystroke;
        let (key, m) = (ks.key.as_str(), ks.modifiers);
        if std::env::var("LEDGER_KEYLOG").is_ok() {
            println!("[key] {key:?} char={:?} active={:?} edit={}", ks.key_char, self.active, self.edit.is_some());
        }

        // Clipboard / undo first (they are modifier combinations).
        if m.platform {
            match key {
                "z" if m.shift => return self.redo(cx),
                "z" => return self.undo(cx),
                "c" => return self.copy_row(cx),
                "v" => return self.paste_row(cx),
                _ => return,
            }
        }

        // NOTE: gpui synthesises `ClickEvent::Keyboard` when Space/Enter is
        // pressed on a focused element that has an `on_click` handler, so
        // buttons and the checkbox are keyboard-activatable for free — and
        // handling those keys here as well fires the action twice.
        // Category dropdown owns the keyboard while it is open.
        if let Some(r) = self.dd {
            match key {
                "escape" => self.dd = None,
                "down" => self.dd_hl = (self.dd_hl + 1) % CATEGORIES.len(),
                "up" => self.dd_hl = (self.dd_hl + CATEGORIES.len() - 1) % CATEGORIES.len(),
                "enter" => {
                    self.snapshot();
                    self.rows[r].cat = self.dd_hl;
                    self.dd = None;
                }
                "backspace" => {
                    self.dd_query.pop();
                }
                _ => {
                    if let Some(c) = &ks.key_char {
                        self.dd_query.push_str(c);
                        // type-ahead: jump to the first match
                        if let Some(i) = CATEGORIES
                            .iter()
                            .position(|c| c.to_lowercase().starts_with(&self.dd_query.to_lowercase()))
                        {
                            self.dd_hl = i;
                        }
                    }
                }
            }
            return cx.notify();
        }

        match (self.active, key) {
            (_, "tab") if m.shift => window.focus_prev(),
            (_, "tab") => window.focus_next(),
            (Some(Target::Cell(..)), "enter") => self.move_row(if m.shift { -1 } else { 1 }, window, cx),
            (Some(t), "escape") => {
                // revert the uncommitted edit
                self.bad = None;
                self.edit = match t {
                    Target::Cell(r, f) if f != Field::Cat && f != Field::Reimb => {
                        Some(Cell::new(self.cell_text(r, f)))
                    }
                    Target::Vat => Some(Cell::new(fmt_money(self.vat, self.locale))),
                    _ => None,
                };
                cx.notify();
            }
            (Some(Target::Cell(r, Field::Cat)), _) => {
                if key == "down" || ks.key_char.is_some() {
                    self.dd = Some(r);
                    self.dd_hl = self.rows[r].cat;
                    self.dd_query = String::new();
                    if let Some(c) = &ks.key_char
                        && key != "space"
                        && key != "enter"
                    {
                        self.dd_query.push_str(c);
                        if let Some(i) = CATEGORIES
                            .iter()
                            .position(|x| x.to_lowercase().starts_with(&self.dd_query.to_lowercase()))
                        {
                            self.dd_hl = i;
                        }
                    }
                    cx.notify();
                }
            }
            (Some(Target::Slider), "left") | (Some(Target::Slider), "right") => {
                self.snapshot();
                let d = if key == "right" { 100 } else { -100 };
                self.vat = (self.vat + d).clamp(0, 2500);
                cx.notify();
            }
            (Some(t), _) => {
                // A text/numeric cell: run the key through the hand-rolled editor.
                let Some(buf) = self.edit.as_mut() else { return };
                let numeric = matches!(
                    t,
                    Target::Vat | Target::Cell(_, Field::Qty) | Target::Cell(_, Field::Price)
                );
                let date = matches!(t, Target::Cell(_, Field::Date));
                let filter = if date {
                    Filter::Text
                } else if numeric {
                    Filter::Number {
                        decimal: self.locale.decimal(),
                        group: self.locale.group(),
                    }
                } else {
                    Filter::Text
                };
                match key {
                    "left" => buf.left(),
                    "right" => buf.right(),
                    "home" => buf.home(),
                    "end" => buf.end(),
                    "backspace" => {
                        buf.backspace();
                    }
                    "delete" => {
                        buf.delete();
                    }
                    "up" | "down" if numeric => return self.step(key == "up", m.shift, cx),
                    _ => {
                        if let Some(c) = &ks.key_char {
                            if date {
                                // masked YYYY-MM-DD: auto-insert the dashes
                                let at_end = buf.caret == buf.text.len();
                                if c.chars().all(|ch| ch.is_ascii_digit()) && buf.text.len() < 10 {
                                    if at_end && (buf.text.len() == 4 || buf.text.len() == 7) {
                                        buf.insert("-", Filter::Text);
                                    }
                                    buf.insert(c, Filter::Digits(10));
                                } else if c == "-" && at_end && buf.text.len() < 10 {
                                    buf.insert("-", Filter::Text);
                                }
                            } else if !buf.insert(c, filter) {
                                self.status = format!("\u{201c}{c}\u{201d} rejected by the field filter").into();
                            }
                        }
                    }
                }
                cx.notify();
            }
            _ => {}
        }
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let n = self.errors().len();
        self.status = if n == 0 {
            format!("saved {} rows, total {}", ROWS, fmt_money(self.total(), self.locale)).into()
        } else {
            format!("cannot save: {n} error(s)").into()
        };
        cx.notify();
    }

    fn on_slider_drag(&mut self, ev: &DragMoveEvent<VatDrag>, _: &mut Window, cx: &mut Context<Self>) {
        let x = f32::from(ev.event.position.x) - f32::from(ev.bounds.origin.x);
        let w = f32::from(ev.bounds.size.width).max(1.);
        let v = ((x / w).clamp(0., 1.) * 2500.) as i64;
        if v != self.vat {
            self.vat = v;
            cx.notify();
        }
    }
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

const BG: u32 = 0xf7f7f8;
const PANEL: u32 = 0xffffff;
const LINE: u32 = 0xe4e4e7;
const TEXT: u32 = 0x18181b;
const DIM: u32 = 0x6b7280;
const ACCENT: u32 = 0x2563eb;
const RED: u32 = 0xdc2626;

fn btn(id: &'static str, label: String, on: bool) -> gpui::Stateful<gpui::Div> {
    div().id(id).px_2().py_1().rounded_md().text_sm().cursor_pointer().border_1()
        .border_color(rgb(if on { ACCENT } else { LINE }))
        .bg(rgb(if on { ACCENT } else { PANEL }))
        .text_color(rgb(if on { 0xffffff } else { TEXT }))
        .hover(|s| s.opacity(0.85)).child(label)
}

impl Ledger {
    /// One editable cell: caret is drawn *between* two text spans, so a moving
    /// caret needs no text measurement.
    fn text_cell(
        &self,
        t: Target,
        f: Field,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let (row, focused) = match t {
            Target::Cell(r, _) => (r, self.handle(t).is_focused(window)),
            _ => (0, self.handle(t).is_focused(window)),
        };
        let err = match t {
            Target::Cell(r, fl) => self.err_for(r, fl),
            _ => None,
        };
        let editing = focused && self.edit.is_some();
        let (b, a) = match (&self.edit, editing) {
            (Some(c), true) => (c.before().to_string(), c.after().to_string()),
            _ => (self.cell_text(row, f), String::new()),
        };
        let numeric = f.numeric();
        div()
            .id(("cell", row * 8 + FIELDS.iter().position(|x| *x == f).unwrap_or(7)))
            .track_focus(self.handle(t))
            .w(px(f.width())).h(px(24.)).px_1().flex().items_center()
            .when(numeric, |d| d.justify_end())
            .when(numeric, |d| d.font(tabular_font()))
            .text_sm().text_color(rgb(if err.is_some() { RED } else { TEXT }))
            .border_1()
            .border_color(rgb(if err.is_some() { RED } else if focused { ACCENT } else { LINE }))
            .rounded_sm().cursor_text()
            .when(focused && err.is_none(), |d| d.bg(rgba(0x2563eb14)))
            .child(div().child(SharedString::from(b)))
            .when(editing, |d| d.child(div().w(px(1.5)).h(px(15.)).bg(rgb(ACCENT))))
            .child(div().child(SharedString::from(a)))
            .into_any_element()
    }

    fn cat_cell(&self, r: usize, window: &Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        let t = Target::Cell(r, Field::Cat);
        let focused = self.handle(t).is_focused(window);
        let open = self.dd == Some(r);
        let hl = self.dd_hl;
        let items: Vec<gpui::AnyElement> = CATEGORIES
            .iter()
            .enumerate()
            .map(|(i, c)| {
                div().id(("dd", r * 8 + i)).px_2().py_1().text_sm().cursor_pointer()
                    .bg(rgb(if i == hl { ACCENT } else { PANEL }))
                    .text_color(rgb(if i == hl { 0xffffff } else { TEXT }))
                    .child(*c)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.snapshot();
                        this.rows[r].cat = i;
                        this.dd = None;
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .collect();
        div()
            .id(("catcell", r))
            .track_focus(self.handle(t))
            .relative().w(px(Field::Cat.width())).h(px(24.)).px_1().flex().items_center()
            .text_sm().text_color(rgb(TEXT)).border_1()
            .border_color(rgb(if focused { ACCENT } else { LINE }))
            .rounded_sm().cursor_pointer()
            .child(div().flex_1().child(CATEGORIES[self.rows[r].cat]))
            .child(div().text_color(rgb(DIM)).child("\u{25be}"))
            .on_click(cx.listener(move |this, ev: &ClickEvent, _, cx| {
                if matches!(ev, ClickEvent::Keyboard(_)) {
                    return; // Space/Enter is handled in on_key (see FRICTION)
                }
                this.dd = if this.dd == Some(r) { None } else { Some(r) };
                this.dd_hl = this.rows[r].cat;
                this.dd_query = String::new();
                cx.notify();
            }))
            // `deferred()` paints the popup after every ancestor, so it is not
            // covered by the rows drawn later (gpui has no z-index).
            .when(open, |d| {
                d.child(gpui::deferred(
                    gpui::anchored().snap_to_window().child(
                        div().absolute().top(px(24.)).left_0().w(px(Field::Cat.width()))
                            .bg(rgb(PANEL)).border_1().border_color(rgb(LINE)).rounded_md()
                            .shadow_md().occlude().children(items),
                    ),
                ))
            })
            .into_any_element()
    }

    fn reimb_cell(&self, r: usize, window: &Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        let t = Target::Cell(r, Field::Reimb);
        let focused = self.handle(t).is_focused(window);
        let on = self.rows[r].reimb;
        div()
            .id(("reimb", r))
            .track_focus(self.handle(t))
            .w(px(Field::Reimb.width())).h(px(24.)).flex().items_center().justify_center()
            .border_1().border_color(rgba(if focused { 0x2563ebff } else { 0x00000000 })).rounded_sm()
            .cursor_pointer()
            .child(
                div().size(px(14.)).rounded_sm().border_1().border_color(rgb(DIM))
                    .flex().items_center().justify_center()
                    .when(on, |d| d.bg(rgb(ACCENT)).border_color(rgb(ACCENT)))
                    .when(on, |d| d.child(div().text_xs().text_color(rgb(0xffffff)).child("\u{2713}"))),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.snapshot();
                this.rows[r].reimb = !this.rows[r].reimb;
                cx.notify();
            }))
            .into_any_element()
    }
}

impl Render for Ledger {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Commit the cell we left / start editing the one we entered.
        self.sync_focus(window, cx);

        let errs = self.errors();
        let l = self.locale;
        let money = |v: i64| SharedString::from(fmt_money(v, l));

        let header = div().flex().px_2().py_1().bg(rgb(0xf1f5f9)).border_b_1().border_color(rgb(LINE))
            .text_xs().text_color(rgb(DIM))
            .children(FIELDS.iter().take(5).map(|f| {
                div().w(px(f.width())).px_1().flex()
                    .when(f.numeric(), |d| d.justify_end())
                    .child(f.label())
                    .into_any_element()
            }))
            .child(div().w(px(104.)).px_1().flex().justify_end().child("Amount"))
            .child(div().w(px(Field::Reimb.width())).px_1().flex().justify_center().child("Reimb"));

        let rows: Vec<gpui::AnyElement> = (0..ROWS)
            .map(|r| {
                let amount = self.rows[r].amount();
                div().flex().items_center().px_2().h(px(26.)).border_b_1().border_color(rgb(0xf1f5f9))
                    .when(r % 2 == 1, |d| d.bg(rgb(0xfafafa)))
                    .child(self.text_cell(Target::Cell(r, Field::Date), Field::Date, window, cx))
                    .child(self.text_cell(Target::Cell(r, Field::Desc), Field::Desc, window, cx))
                    .child(self.cat_cell(r, window, cx))
                    .child(self.text_cell(Target::Cell(r, Field::Qty), Field::Qty, window, cx))
                    .child(self.text_cell(Target::Cell(r, Field::Price), Field::Price, window, cx))
                    .child(
                        div().w(px(104.)).px_1().flex().justify_end().text_sm()
                            .font(tabular_font())
                            .text_color(rgb(if amount < 0 { RED } else { TEXT }))
                            .child(money(amount)),
                    )
                    .child(self.reimb_cell(r, window, cx))
                    .into_any_element()
            })
            .collect();

        let vat_focus = self.handle(Target::Vat).is_focused(window);
        let vat_txt = match (&self.edit, vat_focus) {
            (Some(c), true) => c.text.clone(),
            _ => fmt_money(self.vat, l),
        };
        let slider_focus = self.handle(Target::Slider).is_focused(window);
        let frac = self.vat as f32 / 2500.;

        let footer = div().flex().items_center().gap_3().px_3().py_2().border_t_1().border_color(rgb(LINE))
            .child(div().text_sm().text_color(rgb(DIM)).child("Subtotal"))
            .child(div().w(px(110.)).flex().justify_end().text_sm().font(tabular_font()).child(money(self.subtotal())))
            .child(div().text_sm().text_color(rgb(DIM)).child("VAT"))
            .child(
                div().id("vat-track").track_focus(self.handle(Target::Slider))
                    .relative().w(px(140.)).h(px(18.)).flex().items_center().cursor_pointer()
                    .border_1().border_color(rgba(if slider_focus { 0x2563ebff } else { 0x00000000 })).rounded_sm()
                    .child(div().w_full().h(px(4.)).rounded_full().bg(rgb(0xd4d4d8)))
                    .child(
                        div().absolute().top(px(4.)).left(px(136. * frac)).size(px(10.))
                            .rounded_full().bg(rgb(ACCENT)),
                    )
                    .on_drag(VatDrag, |_, _, _, cx| cx.new(|_| NoGhost))
                    .on_drag_move(cx.listener(Self::on_slider_drag)),
            )
            .child(
                div().id("vat-field").track_focus(self.handle(Target::Vat))
                    .w(px(64.)).h(px(22.)).px_1().flex().items_center().justify_end()
                    .font(tabular_font()).text_sm().border_1()
                    .border_color(rgb(if vat_focus { ACCENT } else { LINE })).rounded_sm().cursor_text()
                    .child(SharedString::from(vat_txt)),
            )
            .child(div().text_sm().text_color(rgb(DIM)).child("%"))
            .child(div().w(px(110.)).flex().justify_end().text_sm().font(tabular_font()).child(money(self.vat_amount())))
            .child(div().flex_1())
            .child(div().text_sm().font_weight(gpui::FontWeight::SEMIBOLD).child("Total"))
            .child(
                div().w(px(120.)).flex().justify_end().text_sm().font(tabular_font())
                    .font_weight(gpui::FontWeight::SEMIBOLD).child(money(self.total())),
            );

        let summary: Vec<gpui::AnyElement> = errs
            .iter()
            .take(4)
            .map(|(r, f, m)| {
                div().text_xs().text_color(rgb(RED))
                    .child(format!("row {} · {} — {m}", r + 1, f.label()))
                    .into_any_element()
            })
            .collect();

        div()
            .id("ledger-root")
            .size_full().flex().flex_col().bg(rgb(BG)).text_color(rgb(TEXT))
            .on_key_down(cx.listener(Self::on_key))
            .child(
                div().flex().items_center().gap_2().px_3().py_2()
                    .child(
                        btn("locale", format!("Locale: {}", l.label()), false)
                            .track_focus(self.handle(Target::LocaleBtn))
                            .on_click(cx.listener(|this, _, _, cx| {
                                let n = if this.locale == Locale::EnUs { Locale::FrFr } else { Locale::EnUs };
                                this.set_locale(n, cx);
                            })),
                    )
                    .child(div().flex_1().text_xs().text_color(rgb(DIM)).child(self.status.clone()))
                    .child(
                        div().text_xs().text_color(rgb(if errs.is_empty() { DIM } else { RED }))
                            .child(format!("{} error(s)", errs.len())),
                    )
                    .child(
                        btn("save", "Save".to_string(), errs.is_empty())
                            .track_focus(self.handle(Target::Save))
                            .when(!errs.is_empty(), |d| d.opacity(0.45).cursor_default())
                            .on_click(cx.listener(|this, _, _, cx| {
                                if this.errors().is_empty() {
                                    this.save(cx)
                                }
                            })),
                    ),
            )
            .child(
                div().id("table").flex_1().mx_3().overflow_y_scroll().bg(rgb(PANEL))
                    .border_1().border_color(rgb(LINE)).rounded_md()
                    .child(header).children(rows),
            )
            .child(footer)
            .when(!errs.is_empty(), |d| {
                d.child(
                    div().flex().flex_col().gap_1().px_3().pb_2()
                        .child(div().text_xs().text_color(rgb(DIM)).child("Errors"))
                        .children(summary),
                )
            })
    }
}

// ---------------------------------------------------------------------------
// Self-test
// ---------------------------------------------------------------------------

impl Ledger {
    fn spawn_selftest(cx: &mut Context<Self>) {
        let exec = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            exec.timer(Duration::from_millis(1200)).await;
            let mut pass = 0u32;
            let mut fail = 0u32;
            let mut check = |name: &str, ok: bool, detail: String| {
                if ok {
                    pass += 1;
                    println!("PASS {name} :: {detail}");
                } else {
                    fail += 1;
                    println!("FAIL {name} :: {detail}");
                }
            };

            // 1. locale parse/format round trips
            for (input, loc, want) in [
                ("1234.5", Locale::EnUs, 123450i64),
                ("$1,234.56", Locale::EnUs, 123456),
                ("1.234,56 \u{20ac}", Locale::EnUs, 123456),
                ("(12.50)", Locale::EnUs, -1250),
                ("1 234,56", Locale::FrFr, 123456),
                ("-99999,99", Locale::FrFr, -9999999),
                ("1,234", Locale::EnUs, 123400),
            ] {
                let got = parse_money(input, loc);
                check(
                    "parse",
                    got == Some(want),
                    format!("{input:?} @{} -> {got:?} (want {want})", loc.label()),
                );
            }
            check(
                "format_en",
                fmt_money(123450, Locale::EnUs) == "1,234.50",
                fmt_money(123450, Locale::EnUs),
            );
            check(
                "format_fr",
                fmt_money(123450, Locale::FrFr) == "1 234,50",
                fmt_money(123450, Locale::FrFr),
            );
            check("reject_junk", parse_money("12a3", Locale::EnUs).is_none(), "12a3".into());

            // 2. typing filter: "1234.5" survives, letters do not
            let mut c = Cell::new(String::new());
            let f = Filter::Number { decimal: '.', group: ',' };
            let typed = "1234.5".chars().all(|ch| c.insert(&ch.to_string(), f));
            let rejected = !c.insert("a", f) && !c.insert(".", f);
            check("typing_filter", typed && rejected && c.text == "1234.5", c.text.clone());

            // 3. commit + blur formatting through the real code path
            let _ = this.update(cx, |t, cx| {
                t.active = Some(Target::Cell(0, Field::Price));
                t.edit = Some(Cell::new("1234.5".into()));
                t.commit(Target::Cell(0, Field::Price), cx);
            });
            let shown = this
                .update(cx, |t, _| t.cell_text(0, Field::Price))
                .unwrap_or_default();
            check("blur_formats_en", shown == "1,234.50", shown.clone());
            let _ = this.update(cx, |t, cx| t.set_locale(Locale::FrFr, cx));
            let shown_fr = this
                .update(cx, |t, _| t.cell_text(0, Field::Price))
                .unwrap_or_default();
            check("locale_toggle_live", shown_fr == "1 234,50", shown_fr.clone());
            let _ = this.update(cx, |t, cx| t.set_locale(Locale::EnUs, cx));

            // 4. arrow stepping
            let _ = this.update(cx, |t, cx| {
                t.active = Some(Target::Cell(0, Field::Qty));
                t.edit = Some(Cell::new(t.cell_text(0, Field::Qty)));
                t.step(true, false, cx);
                t.step(true, true, cx);
            });
            let q = this.update(cx, |t, _| t.rows[0].qty).unwrap_or(0);
            check("arrow_step", q == 2 + 1 + 10, format!("qty = {q} (2 +1 +10)"));

            // 5. invalid input keeps the previous committed value
            let _ = this.update(cx, |t, cx| {
                t.active = Some(Target::Cell(1, Field::Price));
                t.edit = Some(Cell::new("abc".into()));
                t.commit(Target::Cell(1, Field::Price), cx);
            });
            let (p, bad) = this
                .update(cx, |t, _| (t.rows[1].price, t.bad.is_some()))
                .unwrap_or((0, false));
            check("invalid_keeps_previous", p == 21390 && bad, format!("price {p}, error shown {bad}"));

            // 6. live totals
            let (sub, vat, tot) = this
                .update(cx, |t, _| (t.subtotal(), t.vat_amount(), t.total()))
                .unwrap_or_default();
            check(
                "totals",
                tot == sub + vat && vat == (sub * 20 + 50) / 100,
                format!("subtotal {} vat {} total {}", fmt_money(sub, Locale::EnUs), fmt_money(vat, Locale::EnUs), fmt_money(tot, Locale::EnUs)),
            );

            // 7. validation + Save gating
            let _ = this.update(cx, |t, cx| {
                t.rows[2].desc = String::new();
                cx.notify();
            });
            let n = this.update(cx, |t, _| t.errors().len()).unwrap_or(0);
            check("validation_counts", n == 1, format!("{n} error(s) with an empty description"));
            let _ = this.update(cx, |t, cx| {
                t.rows[2].desc = "USB-C dock".into();
                cx.notify();
            });
            let n = this.update(cx, |t, _| t.errors().len()).unwrap_or(9);
            check("validation_clears", n == 0, format!("{n} error(s)"));

            // 8. undo / redo of a committed cell edit
            let before = this.update(cx, |t, _| t.rows[3].qty).unwrap_or(0);
            let _ = this.update(cx, |t, cx| {
                t.active = Some(Target::Cell(3, Field::Qty));
                t.edit = Some(Cell::new("42".into()));
                t.commit(Target::Cell(3, Field::Qty), cx);
            });
            let mid = this.update(cx, |t, _| t.rows[3].qty).unwrap_or(0);
            let _ = this.update(cx, |t, cx| t.undo(cx));
            let after = this.update(cx, |t, _| t.rows[3].qty).unwrap_or(0);
            let _ = this.update(cx, |t, cx| t.redo(cx));
            let again = this.update(cx, |t, _| t.rows[3].qty).unwrap_or(0);
            check(
                "undo_redo_form_level",
                mid == 42 && after == before && again == 42,
                format!("{before} -> {mid} -> undo {after} -> redo {again}"),
            );

            // 9. TSV round trip through the real clipboard
            let _ = this.update(cx, |t, cx| {
                t.active = Some(Target::Cell(4, Field::Desc));
                t.copy_row(cx);
            });
            let tsv = cx
                .update(|cx| cx.read_from_clipboard().and_then(|i| i.text()))
                .ok()
                .flatten();
            let want = this.update(cx, |t, _| t.row_tsv(4)).unwrap_or_default();
            check("row_copy_tsv", tsv.as_deref() == Some(want.as_str()), format!("{tsv:?}"));
            let _ = cx.update(|cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(
                    "2026-02-02\tPasted row\tMeals\t7\t12.34\tyes".into(),
                ))
            });
            let _ = this.update(cx, |t, cx| {
                t.active = Some(Target::Cell(5, Field::Desc));
                t.paste_row(cx);
            });
            let r5 = this.update(cx, |t, _| t.rows[5].clone()).ok();
            check(
                "row_paste_tsv",
                r5.as_ref()
                    .is_some_and(|r| r.desc == "Pasted row" && r.qty == 7 && r.price == 1234 && r.cat == 1 && r.reimb),
                format!("{:?}", r5.map(|r| (r.date, r.desc, r.cat, r.qty, r.price, r.reimb))),
            );

            // 10. tabular figures: ask the text system directly
            let widths = cx
                .update(|cx| {
                    let Some(win) = cx.windows().first().copied() else {
                        return (0., 0., 0., 0.);
                    };
                    win.update(cx, |_, window, _| {
                        let ts = window.text_system().clone();
                        let measure = |s: &'static str, font: Font| {
                            let run = TextRun {
                                len: s.len(),
                                font,
                                color: gpui::black(),
                                background_color: None,
                                underline: None,
                                strikethrough: None,
                            };
                            f32::from(ts.layout_line(s, px(14.), &[run], None).width)
                        };
                        let plain = gpui::font(".SystemUIFont");
                        (
                            measure("1111111111", plain.clone()),
                            measure("0000000000", plain),
                            measure("1111111111", tabular_font()),
                            measure("0000000000", tabular_font()),
                        )
                    })
                    .unwrap_or((0., 0., 0., 0.))
                })
                .unwrap_or((0., 0., 0., 0.));
            println!(
                "TNUM default: '1'x10 = {:.2}px, '0'x10 = {:.2}px | tnum+lnum: '1'x10 = {:.2}px, '0'x10 = {:.2}px",
                widths.0, widths.1, widths.2, widths.3
            );
            check(
                "tabular_figures_equal_advance",
                (widths.2 - widths.3).abs() < 0.01 && widths.2 > 0.,
                format!("tnum widths {:.2} vs {:.2}", widths.2, widths.3),
            );

            // 11. accessibility probe (expected to be empty — gpui has no a11y)
            println!("A11Y gpui 0.2.2 has no accesskit dependency and no NSAccessibility implementation; see evidence/ax-dump.txt");

            println!("SELFTEST DONE pass={pass} fail={fail}");
            let _ = cx.update(|cx| cx.quit());
        })
        .detach();
    }
}

// ---------------------------------------------------------------------------

fn main() {
    Application::new().run(|cx: &mut App| {
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        let bounds = Bounds::centered(None, size(px(820.), px(600.)), cx);
        cx.open_window(
            WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some(SharedString::from("Ledger (gpui)")),
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| Ledger::new(window, cx)),
        )
        .unwrap();
        cx.activate(true);
    });
}
