//! SPEC-10 "Ledger" — forms & numeric-input probe, vizia 0.4.
//!
//! Number model (the short version; FRICTION.md has the long one):
//! the widget layer is `String` and only `String`. `Textbox::new(cx, sig)`
//! takes any `T: FromStr + ToString`, but its edit path round-trips through
//! text anyway, and `validate(|v| ..)` is *post*-validation — it toggles the
//! `:invalid` pseudo-class and suppresses `on_submit`, it never refuses a
//! keystroke. So every numeric cell here is a `Signal<String>` draft, the
//! filtering/parsing/formatting is application code, and the committed value
//! is a `rust_decimal::Decimal` living in `Signal<Vec<Row>>`.
//!
//! With LEDGER_SELFTEST=1 the app runs the scripted checks, prints evidence
//! lines, ends with `SELFTEST DONE pass=N fail=M` and exits 0.
//! With LEDGER_POSE=<a,b,c> it drives itself into a known state for
//! screenshots (see the `Msg::Pose` arm).

use std::collections::HashMap;
use std::io::Write;
use std::str::FromStr;

use chrono::NaiveDate;
use rust_decimal::Decimal;
use vizia::prelude::*;

// Column widths are applied as INLINE modifiers rather than through the
// stylesheet. TRAP: `Handle::class(name)` does `class_list.insert(name)` — it
// does NOT split on whitespace, so `.class("c-qty num")` creates one class
// literally called "c-qty num" that matches neither `.c-qty` nor `.num`, and
// the cells silently kept the built-in `textbox { width: auto }`. Inline
// widths sidestep the whole question and keep the header, the rows and the
// footer provably on the same grid.
const W_DATE: f32 = 104.0;
const W_CAT: f32 = 118.0;
const W_QTY: f32 = 52.0;
const W_UNIT: f32 = 96.0;
const W_AMOUNT: f32 = 96.0;
const W_REIMB: f32 = 44.0;

const CATEGORIES: [&str; 6] =
    ["Travel", "Meals", "Software", "Hardware", "Training", "Other"];

fn say(line: impl AsRef<str>) {
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{}", line.as_ref());
    let _ = out.flush();
}

// ---------------------------------------------------------------------------
// Locale-aware number model
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
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
            Loc::FrFr => '\u{202f}', // narrow no-break space, as fr-FR uses
        }
    }
    fn name(self) -> &'static str {
        match self {
            Loc::EnUs => "en-US",
            Loc::FrFr => "fr-FR",
        }
    }
}

/// True while `text` can still grow into a valid number in `loc`.
/// This is the "filter while typing" predicate; vizia has no such hook, so it
/// is applied by hand in `on_edit` (see `Msg::EditUnit`).
fn typable(text: &str, loc: Loc) -> bool {
    let mut seen_dec = false;
    let mut seen_digit = false;
    for (i, c) in text.chars().enumerate() {
        match c {
            '-' if i == 0 => {}
            '(' if i == 0 => {}
            ')' => {}
            c if c.is_ascii_digit() => seen_digit = true,
            c if c == loc.dec() => {
                if seen_dec {
                    return false;
                }
                seen_dec = true;
            }
            c if c == loc.group() || c == ' ' || c == '\u{a0}' => {}
            '$' | '€' | '£' => {}
            _ => return false,
        }
    }
    let _ = seen_digit;
    true
}

fn typable_int(text: &str) -> bool {
    text.chars().all(|c| c.is_ascii_digit()) && text.len() <= 3
}

/// Locale-tolerant money parser. Handles `$1,234.56`, `1.234,56 €`,
/// `(12.50)` (accounting negative) and bare `1234.5`. When both `,` and `.`
/// appear the LAST one is the decimal separator; when only one appears it is
/// a decimal separator if 1-2 digits follow it, otherwise grouping.
fn parse_money(raw: &str, loc: Loc) -> Option<Decimal> {
    let mut s: String = raw.trim().to_owned();
    if s.is_empty() {
        return None;
    }
    let mut neg = false;
    if s.starts_with('(') && s.ends_with(')') {
        neg = true;
        s = s[1..s.len() - 1].to_owned();
    }
    s.retain(|c| !matches!(c, '$' | '€' | '£' | ' ' | '\u{a0}' | '\u{202f}' | '\''));
    if let Some(rest) = s.strip_prefix('-') {
        neg = !neg;
        s = rest.to_owned();
    }
    if s.is_empty() || !s.chars().all(|c| c.is_ascii_digit() || c == ',' || c == '.') {
        return None;
    }
    let last_comma = s.rfind(',');
    let last_dot = s.rfind('.');
    let dec_pos = match (last_comma, last_dot) {
        (Some(c), Some(d)) => Some(c.max(d)),
        (Some(c), None) => (s.len() - c - 1 <= 2 && s.matches(',').count() == 1).then_some(c),
        (None, Some(d)) => (s.len() - d - 1 <= 2 && s.matches('.').count() == 1).then_some(d),
        (None, None) => None,
    };
    let (int_part, frac_part) = match dec_pos {
        Some(p) => (s[..p].to_owned(), s[p + 1..].to_owned()),
        None => (s.clone(), String::new()),
    };
    // Reject malformed grouping ("12,,5", ",5", "1,234," ...).
    let mut prev_sep = true;
    for c in int_part.chars() {
        let is_sep = c == ',' || c == '.';
        if is_sep && prev_sep {
            return None;
        }
        prev_sep = is_sep;
    }
    if prev_sep && !int_part.is_empty() {
        return None;
    }
    let int_digits: String = int_part.chars().filter(|c| c.is_ascii_digit()).collect();
    if !frac_part.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let joined = if frac_part.is_empty() {
        int_digits.clone()
    } else {
        format!("{int_digits}.{frac_part}")
    };
    if joined.is_empty() || joined == "." {
        return None;
    }
    let d = Decimal::from_str(&joined).ok()?;
    let _ = loc;
    Some(if neg { -d } else { d })
}

/// 2-decimal, grouped, locale-aware rendering. Always emits exactly two
/// fraction digits so that a right-aligned monospaced-digit column lines up
/// on the separator.
fn fmt_money(value: Decimal, loc: Loc) -> String {
    let v = value.round_dp(2);
    let neg = v.is_sign_negative();
    let text = v.abs().to_string();
    let (int_part, frac_part) = match text.split_once('.') {
        Some((i, f)) => (i.to_owned(), format!("{f:0<2}")[..2].to_owned()),
        None => (text, "00".to_owned()),
    };
    let mut grouped = String::new();
    for (i, c) in int_part.chars().enumerate() {
        if i > 0 && (int_part.len() - i) % 3 == 0 {
            grouped.push(loc.group());
        }
        grouped.push(c);
    }
    format!("{}{}{}{}", if neg { "-" } else { "" }, grouped, loc.dec(), frac_part)
}

// ---------------------------------------------------------------------------
// Data
// ---------------------------------------------------------------------------

#[derive(Clone, PartialEq)]
struct Row {
    date: String,
    desc: String,
    cat: usize,
    qty: i64,
    unit: Decimal,
    reimb: bool,
}

fn dec(s: &str) -> Decimal {
    Decimal::from_str(s).unwrap()
}

fn seed() -> Vec<Row> {
    [
        ("2026-01-07", "Taxi to the airport", 0, 1, "48.20", true),
        ("2026-01-08", "Conference ticket", 4, 1, "1250.00", true),
        ("2026-01-08", "Team dinner", 1, 6, "27.45", true),
        ("2026-01-12", "JetBrains licence", 2, 3, "199.00", false),
        ("2026-01-14", "USB-C dock", 3, 2, "129.99", true),
        ("2026-01-15", "Refund: cancelled hotel", 0, 1, "-312.50", true),
        ("2026-01-19", "Printer paper", 5, 12, "4.75", false),
        ("2026-01-21", "Train to Leeds", 0, 2, "88.30", true),
        ("2026-01-23", "Coffee with candidate", 1, 1, "9.60", false),
        ("2026-02-02", "Monitor arm", 3, 1, "64.00", true),
        ("2026-02-04", "Rust workshop", 4, 4, "450.00", true),
        ("2026-02-09", "Domain renewal", 2, 1, "12.99", false),
    ]
    .into_iter()
    .map(|(date, desc, cat, qty, unit, reimb)| Row {
        date: date.to_owned(),
        desc: desc.to_owned(),
        cat,
        qty,
        unit: dec(unit),
        reimb,
    })
    .collect()
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Field {
    Date,
    Desc,
    Cat,
    Qty,
    Unit,
    Reimb,
}

/// One `Signal<String>` per editable text cell. These are the *drafts*: the
/// text the user is typing. Committed values live in `rows`.
#[derive(Clone, Copy)]
struct Cells {
    date: Signal<String>,
    desc: Signal<String>,
    qty: Signal<String>,
    unit: Signal<String>,
}

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

enum Msg {
    Register(Entity, usize, Field),
    EditDate(usize, String),
    EditDesc(usize, String),
    EditQty(usize, String),
    EditUnit(usize, String),
    CommitRow(usize),
    RevertRow(usize),
    PickDate(usize, NaiveDate),
    SetCat(usize, usize),
    ToggleReimb(usize),
    SetLocale(Loc),
    SetVat(f32),
    EditVatText(String),
    Step(f64),
    MoveDown(bool),
    CopyRow,
    PasteRow,
    Undo,
    Redo,
    Save,
    Pose(String),
    Tick,
}

struct Ledger {
    rows: Signal<Vec<Row>>,
    cells: Vec<Cells>,
    loc: Signal<Loc>,
    vat: Signal<f32>,
    vat_text: Signal<String>,
    status: Signal<String>,
    focus_map: HashMap<Entity, (usize, Field)>,
    trace_focus: bool,
    focus_hits: usize,
    entity_of: HashMap<(usize, Field), Entity>,
    undo: Vec<Vec<Row>>,
    redo: Vec<Vec<Row>>,
    selftest: Option<SelfTest>,
}

impl Ledger {
    fn loc(&self) -> Loc {
        self.loc.get()
    }

    /// Re-render every draft from the committed model (used on commit,
    /// locale switch and undo).
    fn refresh_drafts(&self) {
        let loc = self.loc();
        for (row, cell) in self.rows.get().iter().zip(self.cells.iter()) {
            cell.date.set(row.date.clone());
            cell.desc.set(row.desc.clone());
            cell.qty.set(row.qty.to_string());
            cell.unit.set(fmt_money(row.unit, loc));
        }
    }

    fn push_undo(&mut self) {
        self.undo.push(self.rows.get());
        self.redo.clear();
        if self.undo.len() > 64 {
            self.undo.remove(0);
        }
    }

    fn commit(&mut self, index: usize) {
        let loc = self.loc();
        let Some(cell) = self.cells.get(index).copied() else { return };
        let before = self.rows.get();
        let Some(old) = before.get(index).cloned() else { return };
        let mut next = old.clone();
        next.desc = cell.desc.get();
        if let Ok(q) = cell.qty.get().trim().parse::<i64>() {
            next.qty = q;
        }
        if let Some(u) = parse_money(&cell.unit.get(), loc) {
            next.unit = u;
        }
        let d = cell.date.get();
        if NaiveDate::parse_from_str(&d, "%Y-%m-%d").is_ok() {
            next.date = d;
        }
        if next != old {
            self.push_undo();
            self.rows.update(|rows| rows[index] = next);
        }
        // Normalise the drafts back from the committed values.
        let rows = self.rows.get();
        let row = &rows[index];
        cell.qty.set(row.qty.to_string());
        cell.unit.set(fmt_money(row.unit, loc));
        cell.date.set(row.date.clone());
        cell.desc.set(row.desc.clone());
    }

    fn focused_cell(&self, cx: &EventContext) -> Option<(usize, Field)> {
        self.focus_map.get(&cx.focused()).copied()
    }

    fn tsv(&self, index: usize) -> String {
        let loc = self.loc();
        let rows = self.rows.get();
        let r = &rows[index];
        format!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            r.date,
            r.desc,
            CATEGORIES[r.cat.min(CATEGORIES.len() - 1)],
            r.qty,
            fmt_money(r.unit, loc),
            if r.reimb { "yes" } else { "no" }
        )
    }

    fn apply_tsv(&mut self, index: usize, text: &str) -> bool {
        let loc = self.loc();
        let f: Vec<&str> = text.trim_end_matches(['\n', '\r']).split('\t').collect();
        if f.len() < 5 {
            return false;
        }
        let Some(unit) = parse_money(f[4], loc) else { return false };
        let Ok(qty) = f[3].trim().parse::<i64>() else { return false };
        if NaiveDate::parse_from_str(f[0].trim(), "%Y-%m-%d").is_err() {
            return false;
        }
        let cat = CATEGORIES.iter().position(|c| *c == f[2].trim()).unwrap_or(5);
        self.push_undo();
        let row = Row {
            date: f[0].trim().to_owned(),
            desc: f[1].to_owned(),
            cat,
            qty,
            unit,
            reimb: f.get(5).map(|v| v.trim() == "yes").unwrap_or(false),
        };
        self.rows.update(|rows| rows[index] = row);
        self.refresh_drafts();
        true
    }
}

impl Model for Ledger {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.take(|msg, _| match msg {
            Msg::Register(entity, index, field) => {
                self.focus_map.insert(entity, (index, field));
                self.entity_of.insert((index, field), entity);
            }

            // --- typed input -------------------------------------------------
            // vizia cannot refuse a keystroke, so the filter is applied here:
            // an unacceptable draft is written straight back to the signal the
            // Textbox is bound to.
            Msg::EditUnit(index, text) => {
                let loc = self.loc();
                let cell = self.cells[index].unit;
                if typable(&text, loc) {
                    cell.set(text);
                } else {
                    // The only way to "refuse" a keystroke: write the previous
                    // accepted text back to the signal the Textbox is bound to.
                    cell.set(cell.get());
                    self.status.set(format!("rejected keystroke in row {} unit price", index + 1));
                }
            }
            Msg::EditQty(index, text) => {
                let cell = self.cells[index].qty;
                if typable_int(&text) {
                    cell.set(text);
                } else {
                    cell.set(cell.get());
                    self.status.set(format!("rejected keystroke in row {} qty", index + 1));
                }
            }
            Msg::EditDesc(index, text) => self.cells[index].desc.set(text),
            // Date mask: digits only, dashes inserted after YYYY and MM.
            Msg::EditDate(index, text) => {
                let digits: String =
                    text.chars().filter(|c| c.is_ascii_digit()).take(8).collect();
                let mut masked = String::new();
                for (i, c) in digits.chars().enumerate() {
                    if i == 4 || i == 6 {
                        masked.push('-');
                    }
                    masked.push(c);
                }
                self.cells[index].date.set(masked);
            }

            Msg::CommitRow(index) => self.commit(index),
            Msg::RevertRow(index) => {
                let loc = self.loc();
                let rows = self.rows.get();
                let cell = self.cells[index];
                let row = &rows[index];
                cell.date.set(row.date.clone());
                cell.desc.set(row.desc.clone());
                cell.qty.set(row.qty.to_string());
                cell.unit.set(fmt_money(row.unit, loc));
                self.status.set(format!("row {} edit reverted (Esc)", index + 1));
            }
            Msg::PickDate(index, date) => {
                self.push_undo();
                let text = date.format("%Y-%m-%d").to_string();
                self.rows.update(|rows| rows[index].date = text.clone());
                self.cells[index].date.set(text);
            }
            Msg::SetCat(index, cat) => {
                self.push_undo();
                self.rows.update(|rows| rows[index].cat = cat);
            }
            Msg::ToggleReimb(index) => {
                self.push_undo();
                self.rows.update(|rows| rows[index].reimb = !rows[index].reimb);
            }

            Msg::SetLocale(loc) => {
                self.loc.set(loc);
                self.refresh_drafts();
                self.vat_text.set(fmt_money(
                    Decimal::from_f32_retain(self.vat.get()).unwrap_or_default(),
                    loc,
                ));
                self.status.set(format!("locale {}", loc.name()));
            }

            Msg::SetVat(v) => {
                let v = v.clamp(0.0, 25.0);
                self.vat.set(v);
                let loc = self.loc();
                self.vat_text
                    .set(fmt_money(Decimal::from_f32_retain(v).unwrap_or_default(), loc));
            }
            Msg::EditVatText(text) => {
                let loc = self.loc();
                self.vat_text.set(text.clone());
                if let Some(d) = parse_money(&text, loc) {
                    let v = d.try_into().unwrap_or(0.0f64) as f32;
                    self.vat.set(v.clamp(0.0, 25.0));
                }
            }

            // --- arrow stepping ---------------------------------------------
            Msg::Step(delta) => {
                let Some((index, field)) = self.focused_cell(cx) else { return };
                let loc = self.loc();
                match field {
                    Field::Qty => {
                        let cur = self.cells[index].qty.get().parse::<i64>().unwrap_or(0);
                        let next = (cur + delta as i64).clamp(0, 999);
                        self.cells[index].qty.set(next.to_string());
                        self.commit(index);
                    }
                    Field::Unit => {
                        let cur =
                            parse_money(&self.cells[index].unit.get(), loc).unwrap_or_default();
                        let step = Decimal::from_f64_retain(delta / 100.0).unwrap_or_default();
                        self.cells[index].unit.set(fmt_money(cur + step, loc));
                        self.commit(index);
                    }
                    _ => {}
                }
            }

            // --- spreadsheet navigation --------------------------------------
            Msg::MoveDown(down) => {
                let Some((index, field)) = self.focused_cell(cx) else { return };
                self.commit(index);
                let rows = self.rows.get().len();
                let next = if down {
                    (index + 1) % rows
                } else {
                    (index + rows - 1) % rows
                };
                if let Some(entity) = self.entity_of.get(&(next, field)).copied() {
                    cx.with_current(entity, |cx| cx.focus());
                    self.status.set(format!("focus -> row {} {:?}", next + 1, field));
                }
            }

            // --- clipboard ----------------------------------------------------
            Msg::CopyRow => {
                let Some((index, _)) = self.focused_cell(cx) else { return };
                let tsv = self.tsv(index);
                let _ = cx.set_clipboard(tsv.clone());
                self.status.set(format!("copied row {} as TSV", index + 1));
                say(format!("CLIPBOARD copy {tsv:?}"));
            }
            Msg::PasteRow => {
                let Some((index, _)) = self.focused_cell(cx) else { return };
                let Ok(text) = cx.get_clipboard() else { return };
                if self.apply_tsv(index, &text) {
                    self.status.set(format!("pasted TSV into row {}", index + 1));
                } else {
                    self.status.set(String::from("clipboard is not a ledger row"));
                }
            }

            Msg::Undo => {
                if let Some(prev) = self.undo.pop() {
                    self.redo.push(self.rows.get());
                    self.rows.set(prev);
                    self.refresh_drafts();
                    self.status.set(String::from("undo"));
                }
            }
            Msg::Redo => {
                if let Some(next) = self.redo.pop() {
                    self.undo.push(self.rows.get());
                    self.rows.set(next);
                    self.refresh_drafts();
                    self.status.set(String::from("redo"));
                }
            }

            Msg::Save => self.status.set(String::from("saved 12 rows")),

            Msg::Pose(step) => match step.as_str() {
                "fr" => cx.emit(Msg::SetLocale(Loc::FrFr)),
                "us" => cx.emit(Msg::SetLocale(Loc::EnUs)),
                "bigunit" => {
                    self.cells[0].unit.set(String::from("1234.5"));
                    cx.emit(Msg::CommitRow(0));
                }
                "invalid" => {
                    self.cells[2].desc.set(String::new());
                    self.cells[3].qty.set(String::from("0"));
                    cx.emit(Msg::CommitRow(2));
                    cx.emit(Msg::CommitRow(3));
                }
                other => say(format!("POSE unknown {other:?}")),
            },

            Msg::Tick => {
                if let Some(mut script) = self.selftest.take() {
                    script.step(self, cx);
                    self.selftest = Some(script);
                }
            }
        });

        event.map(|window_event, meta| {
            // TRAP: `Textbox`'s `WindowEvent::FocusOut` arm emits only
            // `TextEvent::EndEdit`, never `TextEvent::Submit`, so `on_submit`
            // (and `on_blur`, which is driven by the never-emitted
            // `TextEvent::Blur`) do NOT fire when the user Tabs out of a cell.
            // SPEC-10's "normalise and format on blur" therefore has to be
            // driven from the raw focus event here.
            if let WindowEvent::FocusOut = window_event {
                if let Some((index, _)) = self.focus_map.get(&meta.target).copied() {
                    cx.emit(Msg::CommitRow(index));
                }
            }
            // Tab-order walk, logged from vizia's own FocusIn event.
            if let WindowEvent::FocusIn = window_event {
                if self.trace_focus {
                    self.focus_hits += 1;
                    let what = match self.focus_map.get(&meta.target) {
                        Some((r, f)) => format!("row {} {:?}", r + 1, f),
                        None => String::from("(unregistered: a ComboBox's internal Textbox, or a toolbar control)"),
                    };
                    say(format!("FOCUS {:>3} {}", self.focus_hits, what));
                }
            }
            if let WindowEvent::KeyDown(code, _) = window_event {
                let m = cx.modifiers();
                let cmd = m.logo() || m.ctrl();
                match code {
                    Code::ArrowUp if !cmd => {
                        cx.emit(Msg::Step(if m.shift() { 10.0 } else { 1.0 }))
                    }
                    Code::ArrowDown if !cmd => {
                        cx.emit(Msg::Step(if m.shift() { -10.0 } else { -1.0 }))
                    }
                    Code::Enter | Code::NumpadEnter => cx.emit(Msg::MoveDown(!m.shift())),
                    // Cmd+L toggles the locale (the same action as the toolbar
                    // buttons; a shortcut makes it scriptable).
                    Code::KeyL if cmd => {
                        let next = if self.loc() == Loc::EnUs { Loc::FrFr } else { Loc::EnUs };
                        cx.emit(Msg::SetLocale(next));
                        meta.consume();
                    }
                    Code::KeyZ if cmd => {
                        cx.emit(if m.shift() { Msg::Redo } else { Msg::Undo });
                        meta.consume();
                    }
                    // The focused Textbox has already QUEUED its own
                    // TextEvent::Copy/Paste by the time this runs, so the row
                    // level action is scheduled one beat later to win.
                    Code::KeyC if cmd => {
                        cx.schedule_emit(Msg::CopyRow, Instant::now() + Duration::from_millis(30));
                    }
                    Code::KeyV if cmd => {
                        cx.schedule_emit(Msg::PasteRow, Instant::now() + Duration::from_millis(30));
                    }
                    _ => {}
                }
            }
        });
    }
}

// ---------------------------------------------------------------------------
// View
// ---------------------------------------------------------------------------

fn main() -> Result<(), ApplicationError> {
    let selftest = std::env::var_os("LEDGER_SELFTEST").is_some();
    let pose: Vec<String> = std::env::var("LEDGER_POSE")
        .ok()
        .map(|v| v.split(',').filter(|s| !s.is_empty()).map(str::to_owned).collect())
        .unwrap_or_default();
    let x: i32 = std::env::var("LEDGER_X").ok().and_then(|v| v.parse().ok()).unwrap_or(60);
    let y: i32 = std::env::var("LEDGER_Y").ok().and_then(|v| v.parse().ok()).unwrap_or(60);

    Application::new(move |cx| {
        cx.add_stylesheet(STYLE).expect("stylesheet");

        let rows = Signal::new(seed());
        let loc = Signal::new(Loc::EnUs);
        let vat = Signal::new(20.0f32);
        let vat_text = Signal::new(String::from("20.00"));
        let status = Signal::new(String::from("ready"));
        let cells: Vec<Cells> = seed()
            .iter()
            .map(|r| Cells {
                date: Signal::new(r.date.clone()),
                desc: Signal::new(r.desc.clone()),
                qty: Signal::new(r.qty.to_string()),
                unit: Signal::new(fmt_money(r.unit, Loc::EnUs)),
            })
            .collect();
        let cells_for_view = cells.clone();

        let timer = cx.add_timer(Duration::from_millis(120), None, |cx, action| {
            if let TimerAction::Tick(_) = action {
                cx.emit(Msg::Tick);
            }
        });

        Ledger {
            rows,
            cells,
            loc,
            vat,
            vat_text,
            status,
            focus_map: HashMap::new(),
            trace_focus: std::env::var_os("LEDGER_TRACE").is_some(),
            focus_hits: 0,
            entity_of: HashMap::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            selftest: selftest.then(SelfTest::default),
        }
        .build(cx);

        if selftest {
            cx.start_timer(timer);
        } else if !pose.is_empty() {
            let mut at = Instant::now() + Duration::from_millis(700);
            for step in pose.iter().cloned() {
                cx.schedule_emit(Msg::Pose(step), at);
                at += Duration::from_millis(400);
            }
        }

        // Derived money. `Memo` recomputes only when `rows`/`vat`/`loc` change.
        let subtotal = Memo::new(move |_| {
            rows.get().iter().map(|r| Decimal::from(r.qty) * r.unit).sum::<Decimal>()
        });
        let vat_amount = Memo::new(move |_| {
            let sub: Decimal =
                rows.get().iter().map(|r| Decimal::from(r.qty) * r.unit).sum::<Decimal>();
            (sub * Decimal::from_f32_retain(vat.get()).unwrap_or_default()
                / Decimal::from(100))
            .round_dp(2)
        });
        let errors = Memo::new(move |_| {
            let mut out = Vec::new();
            for (i, r) in rows.get().iter().enumerate() {
                if r.desc.trim().is_empty() || r.desc.chars().count() > 60 {
                    out.push(format!("row {}: description must be 1–60 characters", i + 1));
                }
                if !(1..=999).contains(&r.qty) {
                    out.push(format!("row {}: qty must be 1–999 (is {})", i + 1, r.qty));
                }
                if r.unit < dec("-99999.99") || r.unit > dec("99999.99") {
                    out.push(format!("row {}: unit price out of range", i + 1));
                }
            }
            out
        });

        VStack::new(cx, move |cx| {
            // ---- toolbar --------------------------------------------------
            HStack::new(cx, move |cx| {
                Label::new(cx, "Locale").class("dim").hoverable(false);
                Button::new(cx, |cx| Label::new(cx, "en-US"))
                    .toggle_class("on", loc.map(|l| *l == Loc::EnUs))
                    .on_press(|cx| cx.emit(Msg::SetLocale(Loc::EnUs)))
                    .name("locale en-US");
                Button::new(cx, |cx| Label::new(cx, "fr-FR"))
                    .toggle_class("on", loc.map(|l| *l == Loc::FrFr))
                    .on_press(|cx| cx.emit(Msg::SetLocale(Loc::FrFr)))
                    .name("locale fr-FR");

                Element::new(cx).width(Pixels(16.0)).hoverable(false);
                Label::new(cx, "VAT %").class("dim").hoverable(false);
                Slider::new(cx, vat)
                    .range(0.0..25.0)
                    .step(0.5f32)
                    .width(Pixels(150.0))
                    .name("VAT percent slider")
                    .on_change(|cx, v| cx.emit(Msg::SetVat(v)));
                Textbox::new(cx, vat_text)
                    .width(Pixels(70.0))
                    .class("num")
                    .name("VAT percent")
                    .role(Role::TextInput)
                    .on_edit(|cx, t| cx.emit(Msg::EditVatText(t)));

                Element::new(cx).width(Stretch(1.0)).hoverable(false);
                Button::new(cx, move |cx| {
                    Label::new(
                        cx,
                        errors.map(|e| {
                            if e.is_empty() {
                                String::from("Save")
                            } else {
                                format!("Save ({} error{})", e.len(), if e.len() == 1 { "" } else { "s" })
                            }
                        }),
                    )
                })
                .variant(ButtonVariant::Primary)
                .disabled(errors.map(|e| !e.is_empty()))
                .name("Save")
                .on_press(|cx| cx.emit(Msg::Save));
            })
            .class("toolbar");

            // ---- header ---------------------------------------------------
            HStack::new(cx, |cx| {
                Label::new(cx, "Date").width(Pixels(W_DATE)).hoverable(false);
                Label::new(cx, "Description").width(Stretch(1.0)).hoverable(false);
                Label::new(cx, "Category").width(Pixels(W_CAT)).hoverable(false);
                Label::new(cx, "Qty").width(Pixels(W_QTY)).class("num").hoverable(false);
                Label::new(cx, "Unit price").width(Pixels(W_UNIT)).class("num").hoverable(false);
                Label::new(cx, "Amount").width(Pixels(W_AMOUNT)).class("num").hoverable(false);
                Label::new(cx, "Reimb.").width(Pixels(W_REIMB)).hoverable(false);
            })
            .class("head");

            // ---- rows -----------------------------------------------------
            ScrollView::new(cx, move |cx| {
                VStack::new(cx, move |cx| {
                    for index in 0..12 {
                        let cell = cells_for_view[index];
                        HStack::new(cx, move |cx| {
                            // Date: masked Textbox + a real Calendar picker in a Dropdown.
                            Dropdown::new(
                                cx,
                                move |cx| {
                                    Textbox::new(cx, cell.date)
                                        .width(Stretch(1.0))
                                        .class("num")
                                        .name(format!("row {} date", index + 1))
                                        .role(Role::TextInput)
                                        .validate(|s: &String| {
                                            NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok()
                                        })
                                        .on_build(move |cx| {
                                            let e = cx.current();
                                            cx.emit(Msg::Register(e, index, Field::Date));
                                        })
                                        .on_edit(move |cx, t| cx.emit(Msg::EditDate(index, t)))
                                        .on_submit(move |cx, _, _| cx.emit(Msg::CommitRow(index)))
                                        .on_cancel(move |cx| cx.emit(Msg::RevertRow(index)));
                                },
                                move |cx| {
                                    Calendar::new(
                                        cx,
                                        cell.date.map(|s| {
                                            NaiveDate::parse_from_str(s, "%Y-%m-%d")
                                                .unwrap_or_else(|_| {
                                                    NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()
                                                })
                                        }),
                                    )
                                    .on_select(move |cx, d| cx.emit(Msg::PickDate(index, d)));
                                },
                            )
                            .width(Pixels(W_DATE));

                            Textbox::new(cx, cell.desc)
                                .width(Stretch(1.0))
                                .name(format!("row {} description", index + 1))
                                .role(Role::TextInput)
                                .validate(|s: &String| {
                                    !s.trim().is_empty() && s.chars().count() <= 60
                                })
                                .on_build(move |cx| {
                                    let e = cx.current();
                                    cx.emit(Msg::Register(e, index, Field::Desc));
                                })
                                .on_edit(move |cx, t| cx.emit(Msg::EditDesc(index, t)))
                                .on_submit(move |cx, _, _| cx.emit(Msg::CommitRow(index)))
                                .on_cancel(move |cx| cx.emit(Msg::RevertRow(index)));

                            ComboBox::new(
                                cx,
                                Signal::new(CATEGORIES.map(String::from).to_vec()),
                                rows.map(move |r| r[index].cat),
                            )
                            .width(Pixels(W_CAT))
                            .name(format!("row {} category", index + 1))
                            .on_build(move |cx| {
                                let e = cx.current();
                                cx.emit(Msg::Register(e, index, Field::Cat));
                            })
                            .on_select(move |cx, c| cx.emit(Msg::SetCat(index, c)));

                            Textbox::new(cx, cell.qty)
                                .width(Pixels(W_QTY))
                                .class("num")
                                .name(format!("row {} quantity", index + 1))
                                .role(Role::SpinButton)
                                .validate(|s: &String| {
                                    s.parse::<i64>().map(|q| (1..=999).contains(&q)).unwrap_or(false)
                                })
                                .on_build(move |cx| {
                                    let e = cx.current();
                                    cx.emit(Msg::Register(e, index, Field::Qty));
                                })
                                .on_edit(move |cx, t| cx.emit(Msg::EditQty(index, t)))
                                .on_submit(move |cx, _, _| cx.emit(Msg::CommitRow(index)))
                                .on_cancel(move |cx| cx.emit(Msg::RevertRow(index)));

                            Textbox::new(cx, cell.unit)
                                .width(Pixels(W_UNIT))
                                .class("num")
                                .name(format!("row {} unit price", index + 1))
                                .role(Role::SpinButton)
                                .validate(move |s: &String| parse_money(s, loc.get()).is_some())
                                .on_build(move |cx| {
                                    let e = cx.current();
                                    cx.emit(Msg::Register(e, index, Field::Unit));
                                })
                                .on_edit(move |cx, t| cx.emit(Msg::EditUnit(index, t)))
                                .on_submit(move |cx, _, _| cx.emit(Msg::CommitRow(index)))
                                .on_cancel(move |cx| cx.emit(Msg::RevertRow(index)));

                            Label::new(
                                cx,
                                Memo::new(move |_| {
                                    let r = rows.get();
                                    fmt_money(Decimal::from(r[index].qty) * r[index].unit, loc.get())
                                }),
                            )
                            .width(Pixels(W_AMOUNT))
                            .class("num")
                            .toggle_class(
                                "neg",
                                rows.map(move |r| (Decimal::from(r[index].qty) * r[index].unit).is_sign_negative()),
                            )
                            .hoverable(false);

                            HStack::new(cx, move |cx| {
                                Checkbox::new(cx, rows.map(move |r| r[index].reimb))
                                    .name(format!("row {} reimbursable", index + 1))
                                    .on_build(move |cx| {
                                        let e = cx.current();
                                        cx.emit(Msg::Register(e, index, Field::Reimb));
                                    })
                                    .on_toggle(move |cx| cx.emit(Msg::ToggleReimb(index)));
                            })
                            .width(Pixels(W_REIMB))
                            .class("reimb-cell");
                        })
                        .class("row");
                    }
                })
                .class("rows");
            })
            .height(Stretch(1.0))
            .class("body");

            // ---- footer ---------------------------------------------------
            HStack::new(cx, move |cx| {
                Label::new(cx, "Subtotal").class("dim").hoverable(false);
                Label::new(cx, Memo::new(move |_| fmt_money(subtotal.get(), loc.get())))
                    .width(Pixels(W_AMOUNT))
                    .class("num")
                    .hoverable(false);
                Label::new(cx, Memo::new(move |_| format!("VAT {:.1} %", vat.get())))
                    .class("dim")
                    .hoverable(false);
                Label::new(cx, Memo::new(move |_| fmt_money(vat_amount.get(), loc.get())))
                    .width(Pixels(W_AMOUNT))
                    .class("num")
                    .hoverable(false);
                Label::new(cx, "Total").class("dim").hoverable(false);
                Label::new(
                    cx,
                    Memo::new(move |_| fmt_money(subtotal.get() + vat_amount.get(), loc.get())),
                )
                .width(Pixels(W_AMOUNT))
                .class("num grand")
                .hoverable(false);
            })
            .class("footer");

            // ---- error summary --------------------------------------------
            Binding::new(cx, errors, move |cx| {
                let list = errors.get();
                if list.is_empty() {
                    Label::new(cx, status).class("status").hoverable(false);
                } else {
                    VStack::new(cx, move |cx| {
                        for e in errors.get() {
                            Label::new(cx, e).class("err-item").hoverable(false);
                        }
                    })
                    .class("errors");
                }
            });
        })
        .class("ledger");
    })
    .title("Ledger (vizia)")
    .inner_size((820, 560))
    .position((x, y))
    .run()
}

// ---------------------------------------------------------------------------
// Scripted self-test (LEDGER_SELFTEST=1)
// ---------------------------------------------------------------------------

#[derive(Default)]
struct SelfTest {
    step: usize,
    pass: u32,
    fail: u32,
}

impl SelfTest {
    fn check(&mut self, what: &str, ok: bool, detail: String) {
        if ok {
            self.pass += 1;
        } else {
            self.fail += 1;
        }
        say(format!("SELFTEST {} {} {}", if ok { "PASS" } else { "FAIL" }, what, detail));
    }

    fn step(&mut self, app: &mut Ledger, cx: &mut EventContext) {
        let step = self.step;
        self.step += 1;
        match step {
            0 => {
                for (raw, want) in [
                    ("$1,234.56", "1234.56"),
                    ("1.234,56 €", "1234.56"),
                    ("(12.50)", "-12.50"),
                    ("1234.5", "1234.5"),
                    ("-99999.99", "-99999.99"),
                ] {
                    let got = parse_money(raw, Loc::EnUs);
                    self.check(
                        "parse",
                        got == Some(dec(want)),
                        format!("{raw:?} -> {got:?} (want {want})"),
                    );
                }
                let bad = parse_money("12,,5", Loc::EnUs);
                self.check("parse_reject", bad.is_none(), format!("\"12,,5\" -> {bad:?}"));
            }
            1 => {
                let v = dec("1234.5");
                let us = fmt_money(v, Loc::EnUs);
                let fr = fmt_money(v, Loc::FrFr);
                self.check("format_en_us", us == "1,234.50", format!("{us:?}"));
                self.check(
                    "format_fr_fr",
                    fr == "1\u{202f}234,50",
                    format!("{fr:?} (U+202F narrow nbsp group separator)"),
                );
            }
            2 => {
                self.check("typable_accepts", typable("-1,2", Loc::EnUs), "\"-1,2\"".into());
                self.check(
                    "typable_rejects_letter",
                    !typable("1a", Loc::EnUs),
                    "\"1a\"".into(),
                );
                self.check(
                    "typable_rejects_two_separators",
                    !typable("1.2.3", Loc::EnUs),
                    "\"1.2.3\"".into(),
                );
                self.check("typable_int", !typable_int("1234"), "qty \"1234\" > 3 digits".into());
            }
            3 => {
                // Type 1234.5 into row 1's unit price and commit (the Tab /
                // blur path) — the draft must come back formatted.
                app.cells[0].unit.set(String::from("1234.5"));
                cx.emit(Msg::CommitRow(0));
            }
            4 => {
                let shown = app.cells[0].unit.get();
                self.check("blur_normalises", shown == "1,234.50", format!("{shown:?}"));
                let committed = app.rows.get()[0].unit;
                self.check(
                    "commit_decimal",
                    committed == dec("1234.5"),
                    format!("{committed}"),
                );
                cx.emit(Msg::SetLocale(Loc::FrFr));
            }
            5 => {
                let shown = app.cells[0].unit.get();
                self.check(
                    "locale_toggle_reformats",
                    shown == "1\u{202f}234,50",
                    format!("{shown:?}"),
                );
                cx.emit(Msg::SetLocale(Loc::EnUs));
            }
            6 => {
                let sub: Decimal =
                    app.rows.get().iter().map(|r| Decimal::from(r.qty) * r.unit).sum();
                self.check(
                    "live_totals",
                    sub == dec("1234.5") + subtotal_of_rest(&app.rows.get()),
                    format!("subtotal={}", fmt_money(sub, Loc::EnUs)),
                );
            }
            7 => {
                // Arrow stepping goes through the same path the key handler uses.
                let entity = app.entity_of[&(1, Field::Qty)];
                cx.with_current(entity, |cx| cx.focus());
            }
            8 => {
                let before = app.cells[1].qty.get();
                cx.emit(Msg::Step(1.0));
                say(format!("SELFTEST INFO qty_before={before}"));
            }
            9 => {
                let after = app.cells[1].qty.get();
                self.check("step_up_qty", after == "2", format!("qty={after}"));
                let entity = app.entity_of[&(1, Field::Unit)];
                cx.with_current(entity, |cx| cx.focus());
            }
            10 => cx.emit(Msg::Step(1.0)),
            11 => {
                let after = app.cells[1].unit.get();
                self.check("step_up_price_0.01", after == "1,250.01", format!("unit={after}"));
                cx.emit(Msg::Step(-10.0));
            }
            12 => {
                let after = app.cells[1].unit.get();
                self.check(
                    "shift_step_price_0.10",
                    after == "1,249.91",
                    format!("unit={after}"),
                );
            }
            13 => {
                // Enter-moves-down from row 2 unit price to row 3 unit price.
                cx.emit(Msg::MoveDown(true));
            }
            14 => {
                let want = app.entity_of[&(2, Field::Unit)];
                self.check(
                    "enter_moves_down",
                    cx.focused() == want,
                    format!("focused={:?} want={:?}", cx.focused(), want),
                );
                cx.emit(Msg::MoveDown(false));
            }
            15 => {
                let want = app.entity_of[&(1, Field::Unit)];
                self.check(
                    "shift_enter_moves_up",
                    cx.focused() == want,
                    format!("focused={:?}", cx.focused()),
                );
            }
            16 => cx.emit(Msg::CopyRow),
            17 => {
                let text = cx.get_clipboard().unwrap_or_default();
                self.check(
                    "copy_row_tsv",
                    text.matches('\t').count() == 5 && text.starts_with("2026-01-08"),
                    format!("{text:?}"),
                );
                let _ = cx.set_clipboard(String::from(
                    "2026-03-01\tPasted row\tMeals\t7\t$1,000.25\tyes",
                ));
                let entity = app.entity_of[&(11, Field::Unit)];
                cx.with_current(entity, |cx| cx.focus());
            }
            18 => cx.emit(Msg::PasteRow),
            19 => {
                let r = app.rows.get()[11].clone();
                self.check(
                    "paste_row_tsv",
                    r.desc == "Pasted row" && r.qty == 7 && r.unit == dec("1000.25") && r.reimb,
                    format!("{} {} {} {}", r.date, r.desc, r.qty, r.unit),
                );
                cx.emit(Msg::Undo);
            }
            20 => {
                let r = app.rows.get()[11].clone();
                self.check(
                    "form_undo",
                    r.desc == "Domain renewal",
                    format!("row 12 desc={:?}", r.desc),
                );
                cx.emit(Msg::Redo);
            }
            21 => {
                let r = app.rows.get()[11].clone();
                self.check("form_redo", r.desc == "Pasted row", format!("row 12 desc={:?}", r.desc));
            }
            22 => {
                // Validation: blank the description of row 3 and zero row 4's qty.
                app.cells[2].desc.set(String::new());
                app.cells[3].qty.set(String::from("0"));
                cx.emit(Msg::CommitRow(2));
                cx.emit(Msg::CommitRow(3));
            }
            23 => {
                let rows = app.rows.get();
                let bad_desc = rows[2].desc.trim().is_empty();
                let bad_qty = rows[3].qty == 0;
                self.check(
                    "validation_state",
                    bad_desc && bad_qty,
                    format!("desc={:?} qty={}", rows[2].desc, rows[3].qty),
                );
            }
            24 => {
                let d = NaiveDate::parse_from_str("2026-01-07", "%Y-%m-%d");
                self.check("date_parse", d.is_ok(), format!("{d:?}"));
                cx.emit(Msg::PickDate(0, NaiveDate::from_ymd_opt(2026, 6, 15).unwrap()));
            }
            25 => {
                let got = app.rows.get()[0].date.clone();
                self.check("date_picker_commit", got == "2026-06-15", format!("{got:?}"));
            }
            26 => {
                cx.emit(Msg::SetVat(7.5));
            }
            27 => {
                let t = app.vat_text.get();
                self.check("slider_to_field", t == "7.50", format!("vat_text={t:?}"));
                cx.emit(Msg::EditVatText(String::from("12,5")));
            }
            28 => {
                let v = app.vat.get();
                self.check("field_to_slider", (v - 12.5).abs() < 0.001, format!("vat={v}"));
            }
            29 => {
                say(format!("SELFTEST DONE pass={} fail={}", self.pass, self.fail));
                std::process::exit(0);
            }
            _ => {}
        }
    }
}

fn subtotal_of_rest(rows: &[Row]) -> Decimal {
    rows.iter().skip(1).map(|r| Decimal::from(r.qty) * r.unit).sum()
}

// ---------------------------------------------------------------------------
// Style
// ---------------------------------------------------------------------------

const STYLE: &str = r#"
.ledger { width: 1s; height: 1s; padding: 8px; vertical-gap: 6px; background-color: var(--background); }

.toolbar { height: auto; horizontal-gap: 6px; alignment: left; }
.toolbar .on { background-color: var(--primary); color: var(--primary-foreground); }
.dim { height: auto; font-size: 12px; color: var(--muted-foreground); }

/* SPEC-10 asks for tabular figures. vizia exposes font-family / font-weight /
   font-slant / font-width / font-variation-settings but NO OpenType feature
   API (there is no `font-feature-settings` property and no `tnum` anywhere in
   vizia_style), so equal-advance digits are obtained the only other way: a
   monospaced face for the numeric columns. */
.num { font-family: "SF Mono", "Menlo", "Courier New", monospace; text-align: right; }
.neg { color: #d33a2f; }

.head { height: 24px; horizontal-gap: 6px; padding-left: 4px; padding-right: 12px;
        font-size: 11px; color: var(--muted-foreground); border-bottom: 1px solid var(--border); }
.body { border: 1px solid var(--border); corner-radius: 6px; }
.rows { height: auto; width: 1s; vertical-gap: 2px; padding: 2px; }
.row  { height: 30px; horizontal-gap: 6px; alignment: center; }

.reimb-cell { alignment: center; }

.row textbox { height: 26px; padding-left: 6px; padding-right: 6px; min-width: auto; }
.row textbox:invalid { border-color: #d33a2f; border-width: 2px; }
.row combobox { height: 26px; }
.row dropdown { height: 26px; }

.footer { height: auto; horizontal-gap: 8px; alignment: right; padding-right: 12px; padding-top: 2px; }
.grand { font-weight: bold; }

.status { height: auto; font-size: 12px; color: var(--muted-foreground); }
.errors { height: auto; max-height: 74px; vertical-gap: 1px; }
.err-item { height: auto; font-size: 12px; color: #d33a2f; }
"#;
