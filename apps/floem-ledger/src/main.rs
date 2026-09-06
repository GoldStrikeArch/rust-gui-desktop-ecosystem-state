//! "Ledger" — forms & numeric-input test (SPEC-10), floem git @ 778bb5f2.
//!
//! Architecture notes (research-relevant):
//! - **Number model**: every editable cell is a `String` `RwSignal` (that is
//!   the only thing `TextInput::new` accepts) plus a committed `Decimal`
//!   `RwSignal`. Typing edits the string; blur (`listener::FocusLost`),
//!   Enter and the arrow-step keys parse it into the `Decimal`; everything
//!   downstream (Amount, Subtotal, VAT, Total, validation) reads only the
//!   `Decimal`s. floem's text input CANNOT be constrained — there is no
//!   input mask, no filter callback, no `on_char`. The nearest thing is an
//!   `Effect` that watches the buffer and writes back a sanitised copy, so
//!   the rejected character appears for one frame; the cursor survives
//!   because `TextInput::event` clamps `cursor_glyph_idx` to the buffer.
//! - **Tabular figures**: floem's `Style` has `font_family`, `font_size`,
//!   `font_weight`, `font_style`, `line_height` — and no font-feature or
//!   font-variation property at all, so `tnum`/`lnum` cannot be requested.
//!   The substitute is a monospaced family plus `text_align(End)`.
//! - **Accessibility**: floem has no AccessKit integration and no a11y tree
//!   of any kind (`grep -r accesskit` over the whole crate returns nothing) —
//!   see FRICTION.md and evidence/ax-dump.txt.

use std::cell::RefCell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use floem::action::{exec_after, focus_window};
use floem::kurbo::Size;
use floem::prelude::*;
use floem::reactive::{Effect, Scope};
use floem::text::Alignment;
use floem::views::dropdown::Dropdown;
use floem::views::slider::{Slider, SliderChanged};
use floem::window::{WindowConfig, WindowId};
use floem::{Application, Clipboard, ViewId};
use rust_decimal::Decimal;
use rust_decimal::prelude::*;

const MONO: &str = "Menlo, Courier New, monospace";
const OK_BORDER: Color = Color::from_rgb8(0x9a, 0x9a, 0xa4);
const ERR: Color = Color::from_rgb8(0xc2, 0x33, 0x2e);
const NEG: Color = Color::from_rgb8(0xc2, 0x33, 0x2e);
const HEAD: Color = Color::from_rgb8(0x80, 0x80, 0x8c);

const CATEGORIES: [&str; 6] = ["Travel", "Meals", "Hardware", "Software", "Office", "Refund"];
const N_ROWS: usize = 12;
// Focusable columns, in reading order: 0 date, 1 description, 2 category,
// 3 qty, 4 unit price, 5 reimbursable (Amount is computed, not focusable).

thread_local! {
    /// `ViewId`s of every focusable cell, `[row][col]`, filled while the view
    /// tree is built. Enter-moves-down needs to reach the cell *below*, which
    /// does not exist yet when the handler closure is created.
    static CELLS: RefCell<Vec<Vec<ViewId>>> = RefCell::new(vec![vec![]; N_ROWS]);
}

fn register_cell(row: usize, col: usize, id: ViewId) {
    CELLS.with(|c| {
        let mut c = c.borrow_mut();
        let r = &mut c[row];
        while r.len() <= col {
            r.push(id);
        }
        r[col] = id;
    });
}

fn cell_id(row: usize, col: usize) -> Option<ViewId> {
    CELLS.with(|c| c.borrow().get(row).and_then(|r| r.get(col)).copied())
}

// ---------------------------------------------------------------------------
// Locale-aware number parsing / formatting (no icu / num-format)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Loc {
    EnUs,
    FrFr,
}

impl Loc {
    fn dec(self) -> char {
        match self {
            Loc::EnUs => '.',
            Loc::FrFr => ',',
        }
    }
    fn group(self) -> char {
        match self {
            Loc::EnUs => ',',
            Loc::FrFr => ' ',
        }
    }
    fn name(self) -> &'static str {
        match self {
            Loc::EnUs => "en-US",
            Loc::FrFr => "fr-FR",
        }
    }
}

/// Format with grouping and a fixed number of decimals.
fn fmt(value: Decimal, dp: u32, loc: Loc) -> String {
    let v = value.round_dp(dp);
    let neg = v.is_sign_negative() && !v.is_zero();
    let plain = format!("{:.*}", dp as usize, v.abs());
    let (int, frac) = match plain.split_once('.') {
        Some((i, f)) => (i.to_string(), f.to_string()),
        None => (plain, String::new()),
    };
    let mut grouped = String::new();
    for (i, ch) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            grouped.push(loc.group());
        }
        grouped.push(ch);
    }
    let mut out = String::new();
    if neg {
        out.push('-');
    }
    out.push_str(&grouped);
    if dp > 0 {
        out.push(loc.dec());
        out.push_str(&frac);
    }
    out
}

/// Tolerant parse: accepts the active locale's own format, the other one, a
/// currency symbol anywhere, and the accounting negative `(12.50)`.
fn parse(text: &str, loc: Loc) -> Option<Decimal> {
    let mut s: String = text
        .chars()
        .filter(|c| !matches!(c, '$' | '€' | '£' | '\u{a0}' | '\u{202f}'))
        .collect();
    s = s.trim().to_string();
    let mut neg = false;
    if s.starts_with('(') && s.ends_with(')') && s.len() > 2 {
        neg = true;
        s = s[1..s.len() - 1].to_string();
    }
    if let Some(rest) = s.strip_prefix('-') {
        neg = !neg;
        s = rest.to_string();
    } else if let Some(rest) = s.strip_prefix('+') {
        s = rest.to_string();
    }
    if s.is_empty() {
        return None;
    }

    // Which of `.` `,` (or a space) is the decimal point?
    let last_dot = s.rfind('.');
    let last_comma = s.rfind(',');
    let dec_pos = match (last_dot, last_comma) {
        // Both present: the RIGHTMOST one is the decimal separator.
        (Some(d), Some(c)) => Some(d.max(c)),
        // Only one, once, with something other than 3 digits behind it, or
        // matching the active locale's decimal separator -> decimal point.
        (Some(d), None) => {
            let after = s.len() - d - 1;
            (s.matches('.').count() == 1 && (after != 3 || loc.dec() == '.')).then_some(d)
        }
        (None, Some(c)) => {
            let after = s.len() - c - 1;
            (s.matches(',').count() == 1 && (after != 3 || loc.dec() == ',')).then_some(c)
        }
        (None, None) => None,
    };

    let (int_part, frac_part) = match dec_pos {
        Some(p) => (&s[..p], &s[p + 1..]),
        None => (&s[..], ""),
    };
    let int_digits: String = int_part.chars().filter(|c| c.is_ascii_digit()).collect();
    if int_part.chars().any(|c| !c.is_ascii_digit() && !matches!(c, '.' | ',' | ' ')) {
        return None;
    }
    if !frac_part.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let joined = if frac_part.is_empty() {
        int_digits
    } else {
        format!("{int_digits}.{frac_part}")
    };
    if joined.is_empty() || joined == "." {
        return None;
    }
    let mut d = Decimal::from_str_exact(&joined).ok()?;
    if neg {
        d.set_sign_negative(true);
    }
    Some(d)
}

/// Characters that can still be part of a number. floem cannot refuse a
/// keystroke, so this is applied *after* the fact by an `Effect`.
fn sanitize(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | ',' | ' ' | '(' | ')'))
        .take(20)
        .collect()
}

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Num {
    /// What the user sees and edits.
    buf: RwSignal<String>,
    /// Last committed value; the only thing arithmetic ever reads.
    val: RwSignal<Decimal>,
    err: RwSignal<bool>,
    dp: u32,
}

#[derive(Clone, Copy)]
struct Row {
    idx: usize,
    date: RwSignal<String>,
    desc: RwSignal<String>,
    cat: RwSignal<String>,
    qty: Num,
    price: Num,
    reimb: RwSignal<bool>,
}

impl Row {
    fn amount(&self) -> Decimal {
        self.qty.val.get() * self.price.val.get()
    }
    fn amount_untracked(&self) -> Decimal {
        self.qty.val.get_untracked() * self.price.val.get_untracked()
    }
    /// (field, message) for every rule this row breaks.
    fn errors(&self) -> Vec<(&'static str, String)> {
        let mut v = Vec::new();
        let d = self.desc.get();
        if d.trim().is_empty() || d.chars().count() > 60 {
            v.push(("Description", "must be 1–60 characters".to_string()));
        }
        if !is_iso_date(&self.date.get()) {
            v.push(("Date", "must be YYYY-MM-DD".to_string()));
        }
        let q = self.qty.val.get();
        if self.qty.err.get() || q < Decimal::ONE || q > Decimal::from(999) || q.fract() != Decimal::ZERO {
            v.push(("Qty", "integer 1–999".to_string()));
        }
        let p = self.price.val.get();
        if self.price.err.get() || p.abs() > Decimal::from_str_exact("99999.99").unwrap() {
            v.push(("Unit price", "−99 999.99 … 99 999.99".to_string()));
        }
        v
    }
}

fn is_iso_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter().enumerate().all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
        && (1..=12).contains(&s[5..7].parse::<u32>().unwrap_or(0))
        && (1..=31).contains(&s[8..10].parse::<u32>().unwrap_or(0))
}

#[derive(Clone)]
struct Undo {
    row: usize,
    field: &'static str,
    before: String,
    after: String,
}

#[derive(Clone, Copy)]
struct Ledger {
    rows: RwSignal<Vec<Row>>,
    loc: RwSignal<Loc>,
    vat: RwSignal<Decimal>,
    vat_buf: RwSignal<String>,
    /// (row, col) of the cell that last gained focus — the target for ⌘C/⌘V.
    focus: RwSignal<Option<(usize, usize)>>,
    undo: RwSignal<Vec<Undo>>,
    redo: RwSignal<Vec<Undo>>,
    status: RwSignal<String>,
    selftest: bool,
}

const SEED: [(&str, &str, &str, i64, &str, bool); N_ROWS] = [
    ("2026-01-04", "Taxi to airport", "Travel", 1, "48.30", true),
    ("2026-01-05", "Team dinner", "Meals", 6, "23.75", true),
    ("2026-01-09", "USB-C dock", "Hardware", 2, "189.99", false),
    ("2026-01-11", "IDE licence", "Software", 4, "99.00", true),
    ("2026-01-15", "Printer paper", "Office", 12, "5.99", false),
    ("2026-01-18", "Returned dock", "Refund", 1, "-189.99", true),
    ("2026-02-02", "Train to Lyon", "Travel", 2, "28.00", true),
    ("2026-02-06", "Client lunch", "Meals", 3, "41.20", true),
    ("2026-02-12", "Mechanical keyboard", "Hardware", 1, "1234.56", false),
    ("2026-02-14", "CI minutes", "Software", 1, "512.35", true),
    ("2026-02-20", "Whiteboard markers", "Office", 8, "3.10", false),
    ("2026-02-25", "Cancelled hotel", "Refund", 1, "-312.00", true),
];

impl Ledger {
    fn new(scope: Scope) -> Self {
        let loc = scope.create_rw_signal(Loc::EnUs);
        let num = |scope: Scope, text: &str, dp: u32| {
            let v = Decimal::from_str_exact(text).unwrap();
            Num {
                buf: scope.create_rw_signal(fmt(v, dp, Loc::EnUs)),
                val: scope.create_rw_signal(v),
                err: scope.create_rw_signal(false),
                dp,
            }
        };
        let rows: Vec<Row> = SEED
            .iter()
            .enumerate()
            .map(|(idx, (date, desc, cat, qty, price, reimb))| Row {
                idx,
                date: scope.create_rw_signal(date.to_string()),
                desc: scope.create_rw_signal(desc.to_string()),
                cat: scope.create_rw_signal(cat.to_string()),
                qty: num(scope, &qty.to_string(), 0),
                price: num(scope, price, 2),
                reimb: scope.create_rw_signal(*reimb),
            })
            .collect();
        Self {
            rows: scope.create_rw_signal(rows),
            loc,
            vat: scope.create_rw_signal(Decimal::from(20)),
            vat_buf: scope.create_rw_signal("20.0".to_string()),
            focus: scope.create_rw_signal(None),
            undo: scope.create_rw_signal(Vec::new()),
            redo: scope.create_rw_signal(Vec::new()),
            status: scope.create_rw_signal(String::from("ready")),
            selftest: std::env::var_os("LEDGER_SELFTEST").is_some(),
        }
    }

    fn row(&self, i: usize) -> Row {
        self.rows.with_untracked(|r| r[i])
    }

    fn subtotal(&self) -> Decimal {
        self.rows.with(|rs| rs.iter().map(|r| r.amount()).sum())
    }
    fn vat_amount(&self) -> Decimal {
        (self.subtotal() * self.vat.get() / Decimal::from(100)).round_dp(2)
    }
    fn total(&self) -> Decimal {
        self.subtotal() + self.vat_amount()
    }

    fn all_errors(&self) -> Vec<String> {
        self.rows.with(|rs| {
            rs.iter()
                .flat_map(|r| {
                    r.errors()
                        .into_iter()
                        .map(move |(f, m)| format!("row {} · {f}: {m}", r.idx + 1))
                })
                .collect()
        })
    }

    /// Parse `cell.buf`, and on success normalise it back into the buffer.
    fn commit(&self, row: usize, field: &'static str, cell: Num) {
        let loc = self.loc.get_untracked();
        let raw = cell.buf.get_untracked();
        if raw.trim().is_empty() {
            cell.err.set(true);
            return;
        }
        match parse(&raw, loc) {
            Some(v) => {
                let v = v.round_dp(cell.dp);
                let before = fmt(cell.val.get_untracked(), cell.dp, loc);
                let after = fmt(v, cell.dp, loc);
                cell.err.set(false);
                cell.val.set(v);
                cell.buf.set(after.clone());
                if before != after {
                    self.push_undo(row, field, before, after);
                }
            }
            None => {
                // Keep the previous committed value, flag the field.
                cell.err.set(true);
                self.status
                    .set(format!("row {} · {field}: cannot parse {raw:?}", row + 1));
            }
        }
    }

    fn revert(&self, cell: Num) {
        cell.err.set(false);
        cell.buf
            .set(fmt(cell.val.get_untracked(), cell.dp, self.loc.get_untracked()));
    }

    fn step(&self, row: usize, field: &'static str, cell: Num, up: bool, shift: bool) {
        let base = if cell.dp == 0 {
            Decimal::ONE
        } else {
            Decimal::from_str_exact("0.01").unwrap()
        };
        let mut delta = base;
        if shift {
            delta *= Decimal::from(10);
        }
        if !up {
            delta = -delta;
        }
        let now = parse(&cell.buf.get_untracked(), self.loc.get_untracked())
            .unwrap_or(cell.val.get_untracked());
        let next = (now + delta).round_dp(cell.dp);
        cell.buf.set(fmt(next, cell.dp, self.loc.get_untracked()));
        self.commit(row, field, cell);
    }

    fn push_undo(&self, row: usize, field: &'static str, before: String, after: String) {
        self.undo.update(|u| {
            u.push(Undo { row, field, before, after });
            if u.len() > 100 {
                u.remove(0);
            }
        });
        self.redo.update(|r| r.clear());
    }

    fn apply(&self, e: &Undo, to_before: bool) {
        let row = self.row(e.row);
        let text = if to_before { &e.before } else { &e.after };
        let loc = self.loc.get_untracked();
        match e.field {
            "Qty" => {
                row.qty.buf.set(text.clone());
                if let Some(v) = parse(text, loc) {
                    row.qty.val.set(v);
                    row.qty.err.set(false);
                }
            }
            "Unit price" => {
                row.price.buf.set(text.clone());
                if let Some(v) = parse(text, loc) {
                    row.price.val.set(v);
                    row.price.err.set(false);
                }
            }
            "Description" => row.desc.set(text.clone()),
            "Date" => row.date.set(text.clone()),
            _ => row.cat.set(text.clone()),
        }
    }

    fn undo_one(&self) {
        if let Some(e) = self.undo.try_update(|u| u.pop()).flatten() {
            self.apply(&e, true);
            self.status.set(format!("undo row {} · {}", e.row + 1, e.field));
            self.redo.update(|r| r.push(e));
        }
    }

    fn redo_one(&self) {
        if let Some(e) = self.redo.try_update(|r| r.pop()).flatten() {
            self.apply(&e, false);
            self.status.set(format!("redo row {} · {}", e.row + 1, e.field));
            self.undo.update(|u| u.push(e));
        }
    }

    /// Re-render every buffer in the newly selected locale (values untouched).
    fn set_locale(&self, loc: Loc) {
        self.loc.set(loc);
        self.rows.with_untracked(|rs| {
            for r in rs {
                r.qty.buf.set(fmt(r.qty.val.get_untracked(), 0, loc));
                r.price.buf.set(fmt(r.price.val.get_untracked(), 2, loc));
            }
        });
        self.vat_buf
            .set(fmt(self.vat.get_untracked(), 1, loc));
        self.status.set(format!("locale {}", loc.name()));
        println!("LOCALE {}", loc.name());
    }

    fn row_tsv(&self, i: usize) -> String {
        let r = self.row(i);
        let loc = self.loc.get_untracked();
        format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}",
            r.date.get_untracked(),
            r.desc.get_untracked(),
            r.cat.get_untracked(),
            fmt(r.qty.val.get_untracked(), 0, loc),
            fmt(r.price.val.get_untracked(), 2, loc),
            fmt(r.amount_untracked(), 2, loc),
            if r.reimb.get_untracked() { "yes" } else { "no" }
        )
    }

    fn copy_row(&self) {
        let Some((i, _)) = self.focus.get_untracked() else { return };
        let tsv = self.row_tsv(i);
        match Clipboard::set_contents(tsv.clone()) {
            Ok(()) => {
                self.status.set(format!("copied row {}", i + 1));
                println!("COPY {tsv}");
            }
            Err(e) => self.status.set(format!("clipboard failed: {e:?}")),
        }
    }

    fn paste_row(&self) {
        let Some((i, _)) = self.focus.get_untracked() else { return };
        let Ok(text) = Clipboard::get_contents() else { return };
        self.paste_tsv(i, &text);
    }

    fn paste_tsv(&self, i: usize, text: &str) {
        let f: Vec<&str> = text.trim_end_matches('\n').split('\t').collect();
        if f.len() < 5 {
            self.status.set("clipboard is not a TSV row".into());
            return;
        }
        let r = self.row(i);
        r.date.set(f[0].to_string());
        r.desc.set(f[1].to_string());
        if CATEGORIES.contains(&f[2]) {
            r.cat.set(f[2].to_string());
        }
        r.qty.buf.set(f[3].to_string());
        r.price.buf.set(f[4].to_string());
        if let Some(v) = f.get(6) {
            r.reimb.set(*v == "yes");
        }
        self.commit(i, "Qty", r.qty);
        self.commit(i, "Unit price", r.price);
        self.status.set(format!("pasted into row {}", i + 1));
        println!("PASTE row={} {text}", i + 1);
    }

    /// Move focus one row up/down within the same column (spreadsheet Enter).
    fn move_focus(&self, down: bool) {
        let Some((row, col)) = self.focus.get_untracked() else { return };
        let next = if down { row + 1 } else { row.wrapping_sub(1) };
        if next < N_ROWS
            && let Some(id) = cell_id(next, col)
        {
            id.request_focus();
        }
    }
}

// ---------------------------------------------------------------------------
// Views
// ---------------------------------------------------------------------------

const W: [f64; 7] = [104.0, 196.0, 124.0, 66.0, 108.0, 108.0, 92.0];
const HEADERS: [&str; 7] = [
    "Date",
    "Description",
    "Category",
    "Qty",
    "Unit price",
    "Amount",
    "Reimb.",
];

fn main() {
    let ledger = Ledger::new(Scope::new());
    let pos = std::env::var("LEDGER_POS").ok().and_then(|s| {
        let v: Vec<f64> = s.split(',').filter_map(|t| t.trim().parse().ok()).collect();
        (v.len() == 2).then(|| floem::kurbo::Point::new(v[0], v[1]))
    });
    let mut config = WindowConfig::default()
        .title("Ledger (floem)")
        .size(Size::new(820.0, 560.0));
    if let Some(p) = pos {
        config = config.position(p);
    }
    Application::new()
        .window(move |id| app_view(ledger, id), Some(config))
        .run();
}

fn app_view(ledger: Ledger, _window_id: WindowId) -> impl IntoView {
    raise_watch();
    if ledger.selftest {
        selftest(ledger);
    }

    let toolbar = Stack::horizontal((
        Label::new("Locale").style(|s| s.font_size(12.0).color(HEAD)),
        Button::new("en-US")
            .action(move || ledger.set_locale(Loc::EnUs))
            .style(move |s| s.apply_if(ledger.loc.get() == Loc::EnUs, |s| s.font_bold())),
        Button::new("fr-FR")
            .action(move || ledger.set_locale(Loc::FrFr))
            .style(move |s| s.apply_if(ledger.loc.get() == Loc::FrFr, |s| s.font_bold())),
        Empty::new().style(|s| s.width(16.0)),
        Label::new("VAT %").style(|s| s.font_size(12.0).color(HEAD)),
        Slider::new_ranged(move || ledger.vat.get().to_f64().unwrap_or(0.0), 0.0..=25.0)
            // A floem slider has no intrinsic height — without an explicit
            // one it lays out 0 px tall and is simply invisible.
            .style(|s| s.width(180.0).height(20.0))
            .on_event_stop(SliderChanged::listener(), move |_, state| {
                let v = Decimal::from_f64(state.value).unwrap_or_default().round_dp(1);
                if v != ledger.vat.get_untracked() {
                    ledger.vat.set(v);
                    ledger.vat_buf.set(fmt(v, 1, ledger.loc.get_untracked()));
                }
            }),
        // Linked numeric field: the slider writes it, it writes the slider.
        TextInput::new(ledger.vat_buf)
            .style(|s| {
                s.width(64.0)
                    .padding(4.0)
                    .font_family(MONO.to_string())
                    .text_align(Alignment::End)
            })
            .on_event_stop(listener::FocusLost, move |_, _| commit_vat(ledger))
            .on_event_stop(TextInputEnter::listener(), move |_, _| commit_vat(ledger)),
        Empty::new().style(|s| s.flex_grow(1.0)),
        Button::new("Copy row")
            .action(move || ledger.copy_row())
            .style(|s| s.font_size(12.0)),
        Button::new("Paste row")
            .action(move || ledger.paste_row())
            .style(|s| s.font_size(12.0)),
        Label::derived(move || {
            let n = ledger.all_errors().len();
            if n == 0 { "Save".to_string() } else { format!("Save ({n} errors)") }
        })
        .style(move |s| {
            let ok = ledger.all_errors().is_empty();
            s.padding_horiz(12.0)
                .padding_vert(5.0)
                .border(1.0)
                .border_radius(6.0)
                .border_color(if ok { OK_BORDER } else { ERR })
                .apply_if(!ok, |s| s.color(ERR))
        })
        .on_event_stop(listener::Click, move |_, _| {
            if ledger.all_errors().is_empty() {
                ledger.status.set("saved".into());
                println!("SAVE ok");
            }
        }),
    ))
    .style(|s| s.gap(6.0).items_center().width_full());

    let header = Stack::horizontal_from_iter(
        HEADERS
            .iter()
            .enumerate()
            .map(|(i, h)| {
                Label::new(*h)
                    .style(|s| s.font_size(11.0).color(HEAD))
                    .container()
                    .style(move |s| {
                        s.width(W[i])
                            .padding_horiz(4.0)
                            .apply_if((3..=5).contains(&i), |s| s.justify_end())
                    })
                    .into_any()
            })
            .collect::<Vec<_>>(),
    )
    .style(|s| s.gap(4.0).padding_vert(4.0).width_full());

    let body = Stack::vertical_from_iter(
        (0..N_ROWS)
            .map(|i| row_view(ledger, i).into_any())
            .collect::<Vec<_>>(),
    )
    .style(|s| s.flex_col().gap(2.0).width_full())
    .scroll()
    .style(|s| s.width_full().flex_grow(1.0).min_height(0.0));

    let money = move |value: Box<dyn Fn() -> Decimal>| {
        Label::derived(move || fmt(value(), 2, ledger.loc.get()))
            .style(|s| s.font_family(MONO.to_string()).font_size(13.0))
            .container()
            .style(|s| s.width(120.0).justify_end())
    };

    let footer = Stack::horizontal((
        Empty::new().style(|s| s.flex_grow(1.0)),
        Label::new("Subtotal").style(|s| s.font_size(12.0).color(HEAD)),
        money(Box::new(move || ledger.subtotal())),
        Label::derived(move || format!("VAT {}%", fmt(ledger.vat.get(), 1, ledger.loc.get())))
            .style(|s| s.font_size(12.0).color(HEAD)),
        money(Box::new(move || ledger.vat_amount())),
        Label::new("Total").style(|s| s.font_size(13.0).font_bold()),
        money(Box::new(move || ledger.total())),
    ))
    .style(|s| s.gap(8.0).items_center().width_full().padding_vert(4.0));

    let errors = dyn_container(
        move || ledger.all_errors(),
        move |list| {
            if list.is_empty() {
                Label::derived(move || ledger.status.get())
                    .style(|s| s.font_size(12.0).color(HEAD))
                    .into_any()
            } else {
                let n = list.len();
                Stack::vertical_from_iter(
                    list.into_iter()
                        .take(4)
                        .map(|e| Label::new(e).style(|s| s.font_size(12.0).color(ERR)).into_any())
                        .collect::<Vec<_>>(),
                )
                .style(move |s| s.flex_col().apply_if(n > 4, |s| s.height(70.0)))
                .into_any()
            }
        },
    )
    .style(|s| s.width_full().height(72.0));

    Stack::vertical((toolbar, header, body, footer, errors))
        .style(|s| s.flex_col().gap(6.0).padding(10.0).size_full())
        // App-level shortcuts. `KeyDown` falls back to the listener registry
        // for shortcut-like (modified) keys, so this fires wherever focus is.
        .on_event_stop(listener::KeyDown, move |_, e| {
            let m = e.modifiers;
            if !(m.meta() || m.ctrl()) {
                return;
            }
            match &e.key {
                Key::Character(c) if c.as_str() == "z" && m.shift() => ledger.redo_one(),
                Key::Character(c) if c.as_str() == "z" => ledger.undo_one(),
                Key::Character(c) if c.as_str() == "Z" => ledger.redo_one(),
                Key::Character(c) if c.as_str() == "c" => ledger.copy_row(),
                Key::Character(c) if c.as_str() == "v" => ledger.paste_row(),
                _ => {}
            }
        })
}

fn commit_vat(ledger: Ledger) {
    let loc = ledger.loc.get_untracked();
    match parse(&ledger.vat_buf.get_untracked(), loc) {
        Some(v) => {
            let v = v.clamp(Decimal::ZERO, Decimal::from(25)).round_dp(1);
            ledger.vat.set(v);
            ledger.vat_buf.set(fmt(v, 1, loc));
        }
        None => ledger.vat_buf.set(fmt(ledger.vat.get_untracked(), 1, loc)),
    }
}

fn row_view(ledger: Ledger, i: usize) -> impl IntoView {
    let row = ledger.row(i);

    // --- Date: hand-rolled mask (floem has no date picker and no mask API).
    let date_input = TextInput::new(row.date).into_view();
    register_cell(i, 0, date_input.id());
    Effect::new(move |prev: Option<String>| {
        let text = row.date.get();
        let masked = mask_date(&text);
        if prev.is_some() && masked != text {
            row.date.set(masked.clone());
        }
        masked
    });
    let date = date_input
        .style(move |s| {
            s.width(W[0])
                .padding(4.0)
                .font_family(MONO.to_string())
                .font_size(13.0)
                .apply_if(!is_iso_date(&row.date.get()), |s| s.border(1.5).border_color(ERR))
        })
        .on_event_cont(listener::FocusGained, move |_, _| focused(ledger, i, 0))
        .on_event_stop(listener::KeyDown, move |_, e| nav_key(ledger, e));

    // --- Description
    let desc_input = TextInput::new(row.desc).into_view();
    register_cell(i, 1, desc_input.id());
    let desc = desc_input
        .placeholder("required")
        .style(move |s| {
            let d = row.desc.get();
            s.width(W[1])
                .padding(4.0)
                .font_size(13.0)
                .apply_if(d.trim().is_empty() || d.chars().count() > 60, |s| {
                    s.border(1.5).border_color(ERR)
                })
        })
        .on_event_cont(listener::FocusGained, move |_, _| focused(ledger, i, 1))
        .on_event_stop(listener::KeyDown, move |_, e| nav_key(ledger, e));

    // --- Category dropdown + hand-rolled type-ahead
    let cat = Dropdown::new_rw(row.cat, CATEGORIES.map(String::from))
        .into_view()
        .style(move |s| s.width(W[2]).font_size(13.0).keyboard_navigable())
        .on_event_cont(listener::FocusGained, move |_, _| focused(ledger, i, 2))
        .on_event_stop(listener::KeyDown, move |_, e| {
            if let Key::Character(c) = &e.key
                && let Some(ch) = c.chars().next()
                && let Some(hit) = CATEGORIES
                    .iter()
                    .find(|c| c.to_lowercase().starts_with(&ch.to_lowercase().to_string()))
            {
                row.cat.set(hit.to_string());
                println!("TYPEAHEAD {ch} -> {hit}");
            } else {
                nav_key(ledger, e);
            }
        });
    register_cell(i, 2, cat.id());

    let qty = num_cell(ledger, i, 3, "Qty", row.qty);
    let price = num_cell(ledger, i, 4, "Unit price", row.price);

    // --- Amount: computed, and split at the separator so the decimal points
    //     line up even when the integer parts differ in width.
    let amount = Stack::horizontal((
        Label::derived(move || {
            let text = fmt(row.amount(), 2, ledger.loc.get());
            text.rsplit_once(ledger.loc.get().dec())
                .map(|(a, _)| a.to_string())
                .unwrap_or(text)
        })
        .style(|s| s.font_family(MONO.to_string()).font_size(13.0))
        .container()
        .style(|s| s.flex_grow(1.0).justify_end()),
        Label::derived(move || {
            let loc = ledger.loc.get();
            let text = fmt(row.amount(), 2, loc);
            text.rsplit_once(loc.dec())
                .map(|(_, b)| format!("{}{b}", loc.dec()))
                .unwrap_or_default()
        })
        .style(|s| s.font_family(MONO.to_string()).font_size(13.0).width(26.0)),
    ))
    .style(move |s| {
        s.width(W[5])
            .padding_horiz(4.0)
            .items_center()
            .apply_if(row.amount().is_sign_negative(), |s| s.color(NEG))
    });

    let check = Checkbox::new_rw(row.reimb)
        .into_view()
        .style(move |s| s.keyboard_navigable())
        .on_event_cont(listener::FocusGained, move |_, _| focused(ledger, i, 5))
        .on_event_stop(listener::KeyDown, move |_, e| nav_key(ledger, e))
        .container()
        .style(|s| s.width(W[6]).justify_center());
    register_cell(i, 5, check.id());

    Stack::horizontal((
        date.into_any(),
        desc.into_any(),
        cat.into_any(),
        qty.into_any(),
        price.into_any(),
        amount.into_any(),
        check.into_any(),
    ))
    .style(|s| s.gap(4.0).items_center().width_full())
}

/// One numeric cell: filtered typing, blur-normalise, arrow stepping, Esc.
fn num_cell(ledger: Ledger, i: usize, col: usize, field: &'static str, cell: Num) -> impl IntoView {
    // floem cannot refuse a keystroke, so filter after the fact.
    Effect::new(move |prev: Option<String>| {
        let text = cell.buf.get();
        let clean = sanitize(&text);
        if prev.is_some() && clean != text {
            cell.buf.set(clean.clone());
            println!("FILTER {text:?} -> {clean:?}");
        }
        clean
    });

    let input = TextInput::new(cell.buf).into_view();
    register_cell(i, col, input.id());
    input
        .style(move |s| {
            s.width(W[col])
                .padding(4.0)
                .font_family(MONO.to_string())
                .font_size(13.0)
                .text_align(Alignment::End)
                .apply_if(cell.err.get(), |s| s.border(1.5).border_color(ERR))
                .apply_if(cell.val.get().is_sign_negative(), |s| s.color(NEG))
        })
        .on_event_cont(listener::FocusGained, move |_, _| focused(ledger, i, col))
        .on_event_stop(listener::FocusLost, move |_, _| ledger.commit(i, field, cell))
        .on_event_stop(TextInputEnter::listener(), move |_, _| {
            ledger.commit(i, field, cell);
            ledger.move_focus(true);
        })
        .on_event_stop(listener::KeyDown, move |_, e| match &e.key {
            Key::Named(NamedKey::ArrowUp) if !e.modifiers.alt() => {
                ledger.step(i, field, cell, true, e.modifiers.shift())
            }
            Key::Named(NamedKey::ArrowDown) if !e.modifiers.alt() => {
                ledger.step(i, field, cell, false, e.modifiers.shift())
            }
            Key::Named(NamedKey::Escape) => ledger.revert(cell),
            _ => nav_key(ledger, e),
        })
}

/// Enter / ⇧Enter spreadsheet navigation, shared by the non-numeric cells.
fn nav_key(ledger: Ledger, e: &floem::prelude::KeyboardEvent) {
    if e.key == Key::Named(NamedKey::Enter) {
        ledger.move_focus(!e.modifiers.shift());
    }
}

fn focused(ledger: Ledger, row: usize, col: usize) {
    ledger.focus.set(Some((row, col)));
    // Focusable col 5 is the Reimb. checkbox; HEADERS[5] is the computed
    // Amount column, which is not focusable.
    println!("FOCUS row={} col={} ({})", row + 1, col, HEADERS[if col == 5 { 6 } else { col }]);
}

/// `YYYY-MM-DD` mask: keep digits, re-insert the dashes, cap at 10 chars.
fn mask_date(text: &str) -> String {
    let digits: String = text.chars().filter(|c| c.is_ascii_digit()).take(8).collect();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i == 4 || i == 6 {
            out.push('-');
        }
        out.push(c);
    }
    out
}

/// Verification hook for a shared research desktop (see FRICTION.md): touching
/// `$LEDGER_RAISE_FILE` makes this app take the front for one moment, so a
/// driver script's synthetic click is not eaten by a sibling app's window.
fn raise_watch() {
    let Some(path) = std::env::var_os("LEDGER_RAISE_FILE") else { return };
    let path = std::path::PathBuf::from(path);
    fn tick(path: std::path::PathBuf) {
        if path.exists() {
            let _ = std::fs::remove_file(&path);
            focus_window();
        }
        exec_after(Duration::from_millis(120), move |_| tick(path));
    }
    tick(path);
}

// ---------------------------------------------------------------------------
// Scripted self-test (LEDGER_SELFTEST=1)
// ---------------------------------------------------------------------------

static PASS: AtomicUsize = AtomicUsize::new(0);
static FAIL: AtomicUsize = AtomicUsize::new(0);

fn check(name: &str, ok: bool) {
    if ok {
        PASS.fetch_add(1, Ordering::Relaxed);
    } else {
        FAIL.fetch_add(1, Ordering::Relaxed);
    }
    println!("CHECK {} {name}", if ok { "pass" } else { "FAIL" });
}

fn dec(s: &str) -> Decimal {
    Decimal::from_str_exact(s).unwrap()
}

fn selftest(ledger: Ledger) {
    exec_after(Duration::from_millis(900), move |_| {
        let row = ledger.row(0);

        // --- parsing ------------------------------------------------------
        check("parse 1234.5 (en-US)", parse("1234.5", Loc::EnUs) == Some(dec("1234.5")));
        check("parse $1,234.56", parse("$1,234.56", Loc::EnUs) == Some(dec("1234.56")));
        check("parse 1.234,56 €", parse("1.234,56 €", Loc::EnUs) == Some(dec("1234.56")));
        check("parse (12.50) accounting neg", parse("(12.50)", Loc::EnUs) == Some(dec("-12.50")));
        check("parse 1 234,56 (fr-FR)", parse("1 234,56", Loc::FrFr) == Some(dec("1234.56")));
        check("parse rejects letters", parse("12a", Loc::EnUs).is_none());
        check("filter drops letters", sanitize("1a2b.5") == "12.5");

        // --- formatting ---------------------------------------------------
        check("format en-US 1,234.50", fmt(dec("1234.5"), 2, Loc::EnUs) == "1,234.50");
        check("format fr-FR 1 234,50", fmt(dec("1234.5"), 2, Loc::FrFr) == "1 234,50");
        check("format negative", fmt(dec("-189.99"), 2, Loc::EnUs) == "-189.99");

        // --- typing into the first row's Unit price -----------------------
        row.price.buf.set("1234.5".into());
        ledger.commit(0, "Unit price", row.price);
        check("blur normalises to 1,234.50", row.price.buf.get_untracked() == "1,234.50");
        println!("SHOT-READY typed 1234.5 -> {}", row.price.buf.get_untracked());

        ledger.set_locale(Loc::FrFr);
        check(
            "locale toggle re-renders as 1 234,50",
            row.price.buf.get_untracked() == "1 234,50",
        );
        check("qty re-rendered too", ledger.row(4).qty.buf.get_untracked() == "12");
        ledger.set_locale(Loc::EnUs);

        // --- arrow stepping ------------------------------------------------
        ledger.step(0, "Unit price", row.price, true, false);
        check("ArrowUp steps price by 0.01", row.price.val.get_untracked() == dec("1234.51"));
        ledger.step(0, "Unit price", row.price, true, true);
        check("Shift+ArrowUp steps by 0.10", row.price.val.get_untracked() == dec("1234.61"));
        ledger.step(0, "Unit price", row.price, false, false);
        check("ArrowDown steps back", row.price.val.get_untracked() == dec("1234.60"));
        let q = ledger.row(1).qty;
        ledger.step(1, "Qty", q, true, false);
        check("ArrowUp steps qty by 1", q.val.get_untracked() == dec("7"));

        // --- invalid input keeps the previous committed value --------------
        let keep = row.price.val.get_untracked();
        row.price.buf.set("..".into());
        ledger.commit(0, "Unit price", row.price);
        check("invalid input flags the field", row.price.err.get_untracked());
        check("invalid input keeps the old value", row.price.val.get_untracked() == keep);
        ledger.revert(row.price);
        check("Esc reverts to the committed value", !row.price.err.get_untracked());

        // --- computed columns and totals -----------------------------------
        let expected: Decimal = (0..N_ROWS).map(|i| ledger.row(i).amount_untracked()).sum();
        check("subtotal equals the sum of amounts", ledger.subtotal() == expected);
        let vat = (expected * dec("20") / dec("100")).round_dp(2);
        check("VAT is 20% of subtotal", ledger.vat_amount() == vat);
        check("total = subtotal + VAT", ledger.total() == expected + vat);

        // --- slider <-> field linkage --------------------------------------
        ledger.vat_buf.set("7.5".into());
        commit_vat(ledger);
        check("field writes the VAT value", ledger.vat.get_untracked() == dec("7.5"));
        ledger.vat.set(dec("12"));
        ledger.vat_buf.set(fmt(dec("12"), 1, Loc::EnUs));
        check("slider writes the field", ledger.vat_buf.get_untracked() == "12.0");

        // --- validation ----------------------------------------------------
        check("no errors on seed data", ledger.all_errors().is_empty());
        ledger.row(2).desc.set(String::new());
        check("empty description is an error", ledger.all_errors().len() == 1);
        ledger.row(3).qty.buf.set("1200".into());
        ledger.commit(3, "Qty", ledger.row(3).qty);
        check("qty 1200 is out of range", ledger.all_errors().len() == 2);
        ledger.row(2).desc.set("Restored".into());
        ledger.row(3).qty.buf.set("4".into());
        ledger.commit(3, "Qty", ledger.row(3).qty);
        check("errors clear again", ledger.all_errors().is_empty());

        // --- form-level undo/redo -------------------------------------------
        let before = ledger.row(5).price.buf.get_untracked();
        ledger.row(5).price.buf.set("42".into());
        ledger.commit(5, "Unit price", ledger.row(5).price);
        check("edit applied", ledger.row(5).price.buf.get_untracked() == "42.00");
        ledger.undo_one();
        check("undo restores the previous value", ledger.row(5).price.buf.get_untracked() == before);
        ledger.redo_one();
        check("redo re-applies it", ledger.row(5).price.buf.get_untracked() == "42.00");
        ledger.undo_one();

        // --- clipboard TSV round trip ----------------------------------------
        ledger.focus.set(Some((0, 3)));
        ledger.copy_row();
        let tsv = ledger.row_tsv(0);
        check("clipboard holds the TSV row", Clipboard::get_contents().ok().as_deref() == Some(tsv.as_str()));
        check("TSV has 7 fields", tsv.split('\t').count() == 7);
        ledger.focus.set(Some((11, 3)));
        ledger.paste_row();
        check(
            "paste fills the target row",
            ledger.row(11).desc.get_untracked() == ledger.row(0).desc.get_untracked()
                && ledger.row(11).price.val.get_untracked() == ledger.row(0).price.val.get_untracked(),
        );

        // --- Enter-moves-down -------------------------------------------------
        ledger.focus.set(Some((0, 4)));
        ledger.move_focus(true);
        exec_after(Duration::from_millis(250), move |_| {
            check("Enter moved focus one row down", ledger.focus.get_untracked() == Some((1, 4)));
            ledger.move_focus(false);
            exec_after(Duration::from_millis(250), move |_| {
                check(
                    "Shift+Enter moved focus one row up",
                    ledger.focus.get_untracked() == Some((0, 4)),
                );
                println!(
                    "SELFTEST DONE pass={} fail={}",
                    PASS.load(Ordering::Relaxed),
                    FAIL.load(Ordering::Relaxed)
                );
                use std::io::Write;
                let _ = std::io::stdout().flush();
                if std::env::var_os("LEDGER_HOLD").is_none() {
                    std::process::exit(if FAIL.load(Ordering::Relaxed) == 0 { 0 } else { 1 });
                }
            });
        });
    });
}
