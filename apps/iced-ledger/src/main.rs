//! "Ledger" — forms & numeric-input test (SPEC-10), iced 0.14.
//!
//! Research-relevant architecture notes:
//!
//! * **The number model.** `text_input` in iced is a *controlled* widget: the
//!   value it draws is whatever the application passes in, and `on_input`
//!   hands you the candidate string. Rejecting the candidate means the
//!   character never appears, so "filter while typing" is real filtering and
//!   not post-validation. The model therefore carries the **edited text** per
//!   cell (`Row::qty`, `Row::price`) and `Decimal` is produced on demand by
//!   `parse_number`; committing a cell rewrites the text with `format_number`.
//!
//! * **Focus.** Only `text_input` (and `text_editor`/`scrollable`) implement
//!   `Widget::operate` with `operation.focusable(..)` in iced 0.14 — `button`,
//!   `checkbox`, `slider` and `combo_box` do NOT, and `combo_box` has no
//!   `.id()` at all. So iced's own focus chain cannot reach a checkbox, a
//!   slider or a dropdown. This app keeps its own `focus: Focus` cursor over
//!   every control, mirrors it into iced with `operation::focus(id)` for the
//!   text cells, and draws its own focus ring for the ones iced cannot focus.
//!
//! * **Blur.** There is no `on_focus`/`on_blur` on `text_input`. Blur is
//!   reconstructed: a global mouse-press subscription asks
//!   `operation::is_focused(current cell id)` on the next update, and a
//!   `false` answer means the user clicked away — commit and reformat.
//!
//! * **Keys.** `text_input` consumes Enter (only if `on_submit` is set),
//!   Escape, Home/End and Left/Right, but ignores Tab and Up/Down. All
//!   navigation is handled from `event::listen_with`, which sees events
//!   regardless of capture status (the Escape trap from apps/iced-board).
//!
//! * **Fonts.** `iced::Font` is `{ family, weight, stretch, style }` — there
//!   is no OpenType feature list, so `tnum`/`lnum` cannot be requested at all.
//!   Tabular figures come from `Font::MONOSPACE`; decimal alignment comes from
//!   right-aligning strings that always carry exactly two fraction digits.

use std::str::FromStr;

use iced::alignment::Horizontal;
use iced::keyboard;
use iced::widget::{
    button, checkbox, column, combo_box, container, operation, row,
    scrollable, slider, space, text, text_input,
};
use iced::{
    Border, Color, Element, Event, Fill, Font, Length, Subscription, Task,
    Theme, event,
};
use rust_decimal::Decimal;

pub fn main() -> iced::Result {
    iced::application(Ledger::new, Ledger::update, Ledger::view)
        .title(|_: &Ledger| String::from("Ledger (iced)"))
        .subscription(Ledger::subscription)
        .window_size((820.0, 560.0))
        .run()
}

// ---------------------------------------------------------------------------
// Locale-aware number parsing and formatting
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
            // U+00A0 NO-BREAK SPACE, the fr-FR grouping separator.
            Locale::FrFr => '\u{00a0}',
        }
    }

    fn label(self) -> &'static str {
        match self {
            Locale::EnUs => "en-US",
            Locale::FrFr => "fr-FR",
        }
    }

    fn other(self) -> Locale {
        match self {
            Locale::EnUs => Locale::FrFr,
            Locale::FrFr => Locale::EnUs,
        }
    }
}

/// Tolerant parse: strips currency symbols and any spacing, understands the
/// accounting negative `(12.50)`, and works out which of `.`/`,` is the
/// decimal separator instead of trusting the current locale — so a pasted
/// `$1,234.56`, `1.234,56 €` and `(12.50)` all parse in either locale.
fn parse_number(input: &str, locale: Locale) -> Option<Decimal> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut negative = false;
    let mut body = trimmed;

    if body.starts_with('(') && body.ends_with(')') {
        negative = true;
        body = &body[1..body.len() - 1];
    }

    // Keep only characters that can be part of a number.
    let mut cleaned: String = body
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.' || *c == ',' || *c == '-')
        .collect();

    if let Some(rest) = cleaned.strip_prefix('-') {
        negative = !negative;
        cleaned = rest.to_string();
    }
    if cleaned.contains('-') {
        return None;
    }

    let last_dot = cleaned.rfind('.');
    let last_comma = cleaned.rfind(',');

    let decimal_at = match (last_dot, last_comma) {
        // Both present: the right-most one is the decimal separator.
        (Some(dot), Some(comma)) => Some(dot.max(comma)),
        (Some(dot), None) => {
            let after = cleaned.len() - dot - 1;
            let count = cleaned.matches('.').count();
            // "1.234" with one separator and exactly three trailing digits is
            // grouping unless '.' is this locale's decimal separator.
            if count > 1 || (after == 3 && locale.decimal() != '.') {
                None
            } else {
                Some(dot)
            }
        }
        (None, Some(comma)) => {
            let after = cleaned.len() - comma - 1;
            let count = cleaned.matches(',').count();
            if count > 1 || (after == 3 && locale.decimal() != ',') {
                None
            } else {
                Some(comma)
            }
        }
        (None, None) => None,
    };

    let normalized: String = match decimal_at {
        Some(index) => {
            let (head, tail) = cleaned.split_at(index);
            let head: String =
                head.chars().filter(char::is_ascii_digit).collect();
            let tail: String =
                tail[1..].chars().filter(char::is_ascii_digit).collect();
            if tail.is_empty() {
                head
            } else {
                format!("{head}.{tail}")
            }
        }
        None => cleaned.chars().filter(char::is_ascii_digit).collect(),
    };

    if normalized.is_empty() || normalized == "." {
        return None;
    }

    let value = Decimal::from_str(&normalized).ok()?;
    Some(if negative { -value } else { value })
}

/// `1234.5 -> "1,234.50"` (en-US) / `"1 234,50"` (fr-FR).
fn format_number(value: Decimal, places: u32, locale: Locale) -> String {
    let negative = value.is_sign_negative();
    let plain = format!("{:.*}", places as usize, value.abs().round_dp(places));
    let (int_part, frac_part) = match plain.split_once('.') {
        Some((head, tail)) => (head.to_string(), Some(tail.to_string())),
        None => (plain, None),
    };

    let mut grouped = String::new();
    for (index, ch) in int_part.chars().enumerate() {
        if index > 0 && (int_part.len() - index) % 3 == 0 {
            grouped.push(locale.group());
        }
        grouped.push(ch);
    }

    let mut out = String::new();
    if negative {
        out.push('-');
    }
    out.push_str(&grouped);
    if let Some(frac) = frac_part {
        out.push(locale.decimal());
        out.push_str(&frac);
    }
    out
}

/// Can `candidate` still grow into a valid number? This is what makes typing
/// filtered rather than merely validated.
fn partial_number(candidate: &str, locale: Locale, integer: bool) -> bool {
    let mut seen_decimal = false;
    for (index, ch) in candidate.chars().enumerate() {
        if ch == '-' {
            if index != 0 || integer {
                return false;
            }
        } else if ch == locale.decimal() {
            if integer || seen_decimal {
                return false;
            }
            seen_decimal = true;
        } else if ch == locale.group() {
            continue;
        } else if !ch.is_ascii_digit() {
            return false;
        }
    }
    true
}

/// `YYYY-MM-DD` mask: only digits in the eight digit slots and `-` in the two
/// separator slots, never longer than ten characters.
fn partial_date(candidate: &str) -> bool {
    if candidate.chars().count() > 10 {
        return false;
    }
    candidate.chars().enumerate().all(|(index, ch)| match index {
        4 | 7 => ch == '-',
        _ => ch.is_ascii_digit(),
    })
}

fn valid_date(value: &str) -> bool {
    if value.len() != 10 || !partial_date(value) {
        return false;
    }
    let year: u32 = value[0..4].parse().unwrap_or(0);
    let month: u32 = value[5..7].parse().unwrap_or(0);
    let day: u32 = value[8..10].parse().unwrap_or(0);
    (1900..=2999).contains(&year)
        && (1..=12).contains(&month)
        && (1..=31).contains(&day)
}

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Category {
    Travel,
    Lodging,
    Meals,
    Training,
    Hardware,
    Software,
    Office,
}

const CATEGORIES: [Category; 7] = [
    Category::Travel,
    Category::Lodging,
    Category::Meals,
    Category::Training,
    Category::Hardware,
    Category::Software,
    Category::Office,
];

impl std::fmt::Display for Category {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Category::Travel => "Travel",
            Category::Lodging => "Lodging",
            Category::Meals => "Meals",
            Category::Training => "Training",
            Category::Hardware => "Hardware",
            Category::Software => "Software",
            Category::Office => "Office",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Date,
    Desc,
    Cat,
    Qty,
    Price,
    Reimb,
}

impl Field {
    fn key(self) -> &'static str {
        match self {
            Field::Date => "date",
            Field::Desc => "desc",
            Field::Cat => "cat",
            Field::Qty => "qty",
            Field::Price => "price",
            Field::Reimb => "reimb",
        }
    }

    fn is_text(self) -> bool {
        matches!(self, Field::Date | Field::Desc | Field::Qty | Field::Price)
    }
}

const FIELDS: [Field; 6] = [
    Field::Date,
    Field::Desc,
    Field::Cat,
    Field::Qty,
    Field::Price,
    Field::Reimb,
];

/// The application's own focus cursor. iced's focus chain cannot express the
/// checkbox / slider / dropdown entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Cell(usize, Field),
    Vat,
    Slider,
    Save,
}

#[derive(Debug, Clone)]
struct Row {
    date: String,
    desc: String,
    cat: Category,
    qty: String,
    price: String,
    reimb: bool,
}

impl Row {
    fn text(&self, field: Field) -> String {
        match field {
            Field::Date => self.date.clone(),
            Field::Desc => self.desc.clone(),
            Field::Qty => self.qty.clone(),
            Field::Price => self.price.clone(),
            _ => String::new(),
        }
    }

    fn set_text(&mut self, field: Field, value: String) {
        match field {
            Field::Date => self.date = value,
            Field::Desc => self.desc = value,
            Field::Qty => self.qty = value,
            Field::Price => self.price = value,
            _ => {}
        }
    }

    fn qty_value(&self, locale: Locale) -> Option<Decimal> {
        parse_number(&self.qty, locale)
    }

    fn price_value(&self, locale: Locale) -> Option<Decimal> {
        parse_number(&self.price, locale)
    }

    fn amount(&self, locale: Locale) -> Option<Decimal> {
        Some(self.qty_value(locale)? * self.price_value(locale)?)
    }
}

struct Ledger {
    rows: Vec<Row>,
    categories: Vec<combo_box::State<Category>>,
    locale: Locale,
    vat: String,
    focus: Focus,
    /// Value of the focused text cell when it gained focus (Esc reverts to it).
    backup: Option<String>,
    undo: Vec<(usize, Field, String)>,
    redo: Vec<(usize, Field, String)>,
    status: String,
    saved: bool,
}

#[derive(Debug, Clone)]
enum Message {
    CellInput(usize, Field, String),
    CellPaste(usize, Field, String),
    /// Which widget iced itself thinks is focused, harvested after a click.
    FocusedIs(iced::advanced::widget::Id),
    CategoryPicked(usize, Category),
    ReimbToggled(usize, bool),
    VatInput(String),
    VatSlider(u8),
    ToggleLocale,
    Save,
    Key(keyboard::Key, keyboard::Modifiers),
    MousePressed,
    FocusAnswer(bool),
    SelfTest,
}

const W_DATE: f32 = 104.0;
const W_DESC: f32 = 186.0;
const W_CAT: f32 = 116.0;
const W_QTY: f32 = 58.0;
const W_PRICE: f32 = 96.0;
const W_AMOUNT: f32 = 96.0;
const W_REIMB: f32 = 60.0;

fn cell_id(row: usize, field: Field) -> String {
    format!("c-{row}-{}", field.key())
}

impl Ledger {
    fn new() -> (Self, Task<Message>) {
        let seed: [(&str, &str, Category, &str, &str, bool); 12] = [
            ("2026-01-08", "Flight LHR-JFK", Category::Travel, "1", "482.50", true),
            ("2026-01-09", "Airport transfer", Category::Travel, "2", "34.75", true),
            ("2026-01-09", "Hotel, 3 nights", Category::Lodging, "3", "219.00", true),
            ("2026-01-10", "Team dinner", Category::Meals, "6", "41.80", true),
            ("2026-01-11", "Conference pass", Category::Training, "1", "1295.00", true),
            ("2026-01-11", "Duplicate charge refund", Category::Travel, "1", "-34.75", false),
            ("2026-01-12", "USB-C dock", Category::Hardware, "1", "189.99", true),
            ("2026-01-13", "IDE licence", Category::Software, "4", "99.00", true),
            ("2026-01-14", "Printer paper", Category::Office, "12", "5.99", false),
            ("2026-01-15", "Taxi to venue", Category::Travel, "2", "18.40", true),
            ("2026-01-16", "Breakfast", Category::Meals, "5", "12.25", false),
            ("2026-01-17", "Monitor stand", Category::Hardware, "1", "76.00", true),
        ];

        let rows: Vec<Row> = seed
            .iter()
            .map(|(date, desc, cat, qty, price, reimb)| Row {
                date: (*date).into(),
                desc: (*desc).into(),
                cat: *cat,
                qty: (*qty).into(),
                price: format_number(
                    parse_number(price, Locale::EnUs).unwrap_or_default(),
                    2,
                    Locale::EnUs,
                ),
                reimb: *reimb,
            })
            .collect();

        let categories = rows
            .iter()
            .map(|row| {
                combo_box::State::with_selection(
                    CATEGORIES.to_vec(),
                    Some(&row.cat),
                )
            })
            .collect();

        let state = Self {
            rows,
            categories,
            locale: Locale::EnUs,
            vat: String::from("20"),
            focus: Focus::Cell(0, Field::Date),
            backup: None,
            undo: Vec::new(),
            redo: Vec::new(),
            status: String::from("ready"),
            saved: false,
        };

        let boot = if std::env::var_os("LEDGER_SELFTEST").is_some() {
            Task::done(Message::SelfTest)
        } else {
            operation::focus(cell_id(0, Field::Date))
        };

        (state, boot)
    }

    // -- derived values ---------------------------------------------------

    fn vat_percent(&self) -> Decimal {
        parse_number(&self.vat, self.locale).unwrap_or_default()
    }

    fn subtotal(&self) -> Decimal {
        self.rows
            .iter()
            .filter_map(|row| row.amount(self.locale))
            .sum()
    }

    fn vat_amount(&self) -> Decimal {
        (self.subtotal() * self.vat_percent() / Decimal::from(100)).round_dp(2)
    }

    fn total(&self) -> Decimal {
        self.subtotal().round_dp(2) + self.vat_amount()
    }

    fn errors(&self) -> Vec<(usize, Field, String)> {
        let mut out = Vec::new();
        for (index, row) in self.rows.iter().enumerate() {
            if !valid_date(&row.date) {
                out.push((index, Field::Date, "date must be YYYY-MM-DD".into()));
            }
            let length = row.desc.chars().count();
            if !(1..=60).contains(&length) {
                out.push((
                    index,
                    Field::Desc,
                    "description must be 1-60 characters".into(),
                ));
            }
            match row.qty_value(self.locale) {
                Some(qty)
                    if qty.fract().is_zero()
                        && qty >= Decimal::ONE
                        && qty <= Decimal::from(999) => {}
                _ => out.push((index, Field::Qty, "qty must be 1-999".into())),
            }
            match row.price_value(self.locale) {
                Some(price)
                    if price.abs() <= Decimal::from_str("99999.99").unwrap() => {}
                _ => out.push((
                    index,
                    Field::Price,
                    "unit price must be -99 999.99 … 99 999.99".into(),
                )),
            }
        }
        out
    }

    fn tab_order(&self) -> Vec<Focus> {
        let mut order: Vec<Focus> = Vec::with_capacity(self.rows.len() * 6 + 3);
        for index in 0..self.rows.len() {
            for field in FIELDS {
                order.push(Focus::Cell(index, field));
            }
        }
        order.push(Focus::Vat);
        order.push(Focus::Slider);
        order.push(Focus::Save);
        order
    }

    // -- editing ----------------------------------------------------------

    /// Commit whatever is in a numeric/date cell: parse, then rewrite the text
    /// in normalised, locale-formatted form. This is the "on blur" behaviour;
    /// there is no blur callback in iced, so every path that leaves a cell
    /// calls it.
    fn commit(&mut self, index: usize, field: Field) {
        let locale = self.locale;
        let row = &mut self.rows[index];
        let normalised = match field {
            Field::Qty => parse_number(&row.qty, locale)
                .map(|value| format_number(value.round(), 0, locale)),
            Field::Price => parse_number(&row.price, locale)
                .map(|value| format_number(value, 2, locale)),
            _ => None,
        };

        if let Some(text) = normalised {
            row.set_text(field, text);
        }

        if let Some(previous) = self.backup.take()
            && previous != self.rows[index].text(field)
        {
            self.undo.push((index, field, previous));
            self.redo.clear();
        }
    }

    fn focus_task(&mut self, target: Focus) -> Task<Message> {
        // Leaving a text cell commits it (this is the blur hook iced lacks).
        if let Focus::Cell(index, field) = self.focus
            && field.is_text()
            && target != self.focus
        {
            self.commit(index, field);
        }

        self.focus = target;
        eprintln!("focus: {target:?}");
        self.backup = match target {
            Focus::Cell(index, field) if field.is_text() => {
                Some(self.rows[index].text(field))
            }
            _ => None,
        };

        match target {
            Focus::Cell(index, field) if field.is_text() => {
                operation::focus(cell_id(index, field))
            }
            // Nothing else in iced 0.14 is focusable, so drop iced's own focus
            // entirely and rely on our focus ring. `operation::focus` on a
            // non-existent id is NOT enough: it silently no-ops and leaves the
            // previous text_input focused, which then snaps our cursor back on
            // the next click. `focusable::unfocus()` (only reachable through
            // the `advanced` feature) is the one that works.
            _ => iced::advanced::widget::operate(
                iced::advanced::widget::operation::focusable::unfocus(),
            ),
        }
    }

    fn step(&mut self, amount: Decimal) -> Task<Message> {
        let Focus::Cell(index, field) = self.focus else {
            return Task::none();
        };
        let locale = self.locale;
        match field {
            Field::Qty => {
                let current = self.rows[index]
                    .qty_value(locale)
                    .unwrap_or_default();
                let next = (current + amount).max(Decimal::ZERO);
                self.rows[index].qty = format_number(next.round(), 0, locale);
            }
            Field::Price => {
                let current = self.rows[index]
                    .price_value(locale)
                    .unwrap_or_default();
                self.rows[index].price =
                    format_number(current + amount, 2, locale);
            }
            _ => return Task::none(),
        }
        Task::none()
    }

    fn row_tsv(&self, index: usize) -> String {
        let row = &self.rows[index];
        format!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            row.date,
            row.desc,
            row.cat,
            row.qty,
            row.price,
            if row.reimb { "yes" } else { "no" }
        )
    }

    fn apply_tsv(&mut self, index: usize, tsv: &str) -> bool {
        let parts: Vec<&str> = tsv.trim_end_matches('\n').split('\t').collect();
        if parts.len() < 5 {
            return false;
        }
        let locale = self.locale;
        let row = &mut self.rows[index];
        row.date = parts[0].trim().to_string();
        row.desc = parts[1].trim().to_string();
        row.cat = CATEGORIES
            .iter()
            .copied()
            .find(|c| c.to_string() == parts[2].trim())
            .unwrap_or(row.cat);
        if let Some(qty) = parse_number(parts[3], locale) {
            row.qty = format_number(qty.round(), 0, locale);
        }
        if let Some(price) = parse_number(parts[4], locale) {
            row.price = format_number(price, 2, locale);
        }
        if let Some(flag) = parts.get(5) {
            row.reimb = matches!(flag.trim(), "yes" | "true" | "1");
        }
        self.categories[index] =
            combo_box::State::with_selection(CATEGORIES.to_vec(), Some(&self.rows[index].cat));
        true
    }

    // -- update -----------------------------------------------------------

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::CellInput(index, field, value) => {
                let locale = self.locale;
                let accepted = match field {
                    Field::Date => partial_date(&value),
                    Field::Desc => value.chars().count() <= 60,
                    Field::Qty => partial_number(&value, locale, true),
                    Field::Price => partial_number(&value, locale, false),
                    _ => false,
                };
                if accepted {
                    // The candidate string only becomes the widget's value if
                    // we store it: this is the filter.
                    self.rows[index].set_text(field, value);
                    self.saved = false;
                    if self.focus != Focus::Cell(index, field) {
                        self.focus = Focus::Cell(index, field);
                        self.backup = Some(self.rows[index].text(field));
                    }
                } else {
                    self.status =
                        format!("rejected keystroke in row {} {}", index + 1, field.key());
                }
                Task::none()
            }
            Message::CellPaste(index, field, value) => {
                let locale = self.locale;
                let handled = match field {
                    Field::Qty => parse_number(&value, locale)
                        .map(|v| format_number(v.round(), 0, locale)),
                    Field::Price => parse_number(&value, locale)
                        .map(|v| format_number(v, 2, locale)),
                    Field::Date if partial_date(&value) => Some(value.clone()),
                    Field::Desc => Some(value.chars().take(60).collect()),
                    _ => None,
                };
                match handled {
                    Some(text) => {
                        self.rows[index].set_text(field, text);
                        self.status = format!("pasted into row {}", index + 1);
                    }
                    None => {
                        self.status =
                            format!("paste rejected: {value:?} is not a number");
                    }
                }
                Task::none()
            }
            Message::FocusedIs(id) => {
                // Click-to-focus: iced gives no `on_focus`, so after every
                // mouse press we ask which widget ended up focused and map the
                // answer back onto our own focus cursor.
                for index in 0..self.rows.len() {
                    for field in FIELDS {
                        if field.is_text()
                            && id == iced::advanced::widget::Id::from(cell_id(index, field))
                            && self.focus != Focus::Cell(index, field)
                        {
                            return self.focus_task(Focus::Cell(index, field));
                        }
                    }
                }
                Task::none()
            }
            Message::CategoryPicked(index, category) => {
                self.rows[index].cat = category;
                self.categories[index] = combo_box::State::with_selection(
                    CATEGORIES.to_vec(),
                    Some(&category),
                );
                self.saved = false;
                self.focus = Focus::Cell(index, Field::Cat);
                Task::none()
            }
            Message::ReimbToggled(index, value) => {
                self.rows[index].reimb = value;
                self.saved = false;
                Task::none()
            }
            Message::VatInput(value) => {
                if partial_number(&value, self.locale, false) {
                    self.vat = value;
                }
                Task::none()
            }
            Message::VatSlider(value) => {
                self.vat = value.to_string();
                Task::none()
            }
            Message::ToggleLocale => {
                // Re-express every stored string in the new locale.
                let from = self.locale;
                let to = from.other();
                for row in &mut self.rows {
                    if let Some(qty) = parse_number(&row.qty, from) {
                        row.qty = format_number(qty, 0, to);
                    }
                    if let Some(price) = parse_number(&row.price, from) {
                        row.price = format_number(price, 2, to);
                    }
                }
                if let Some(vat) = parse_number(&self.vat, from) {
                    self.vat = format_number(vat, 0, to);
                }
                self.locale = to;
                self.status = format!("locale {}", to.label());
                Task::none()
            }
            Message::Save => {
                self.saved = true;
                self.status = format!("saved {} rows", self.rows.len());
                Task::none()
            }
            Message::MousePressed => {
                // Blur detection: ask iced whether the cell we think is focused
                // still is. There is no on_blur in 0.14.
                let blur = match self.focus {
                    Focus::Cell(index, field) if field.is_text() => {
                        operation::is_focused(cell_id(index, field))
                            .map(Message::FocusAnswer)
                    }
                    _ => Task::none(),
                };
                Task::batch([
                    blur,
                    iced::advanced::widget::operate(
                        iced::advanced::widget::operation::focusable::find_focused(),
                    )
                    .map(Message::FocusedIs),
                ])
            }
            Message::FocusAnswer(still_focused) => {
                if !still_focused
                    && let Focus::Cell(index, field) = self.focus
                    && field.is_text()
                {
                    self.commit(index, field);
                }
                Task::none()
            }
            Message::Key(key, modifiers) => self.on_key(key, modifiers),
            Message::SelfTest => self.self_test(),
        }
    }

    fn on_key(
        &mut self,
        key: keyboard::Key,
        modifiers: keyboard::Modifiers,
    ) -> Task<Message> {
        use keyboard::key::Named;

        let character = match &key {
            keyboard::Key::Character(c) => c.to_lowercase(),
            _ => String::new(),
        };

        // Row TSV clipboard. Cmd+C / Cmd+V are already claimed by text_input
        // for its own selection, so the row-level ones are Cmd+Shift+C/V.
        if modifiers.command() && modifiers.shift() && character == "c" {
            if let Focus::Cell(index, _) = self.focus {
                let tsv = self.row_tsv(index);
                self.status = format!("copied row {} as TSV", index + 1);
                return iced::clipboard::write(tsv);
            }
            return Task::none();
        }
        if modifiers.command() && modifiers.shift() && character == "v" {
            if let Focus::Cell(index, _) = self.focus {
                return iced::clipboard::read().map(move |content| {
                    Message::CellPaste(
                        index,
                        Field::Desc,
                        content.unwrap_or_default(),
                    )
                });
            }
            return Task::none();
        }

        // Form-level undo/redo of committed cell edits. text_input has no
        // undo stack of its own in 0.14, so Cmd+Z is free.
        if modifiers.command() && character == "z" {
            let stack = if modifiers.shift() {
                &mut self.redo
            } else {
                &mut self.undo
            };
            if let Some((index, field, value)) = stack.pop() {
                let current = self.rows[index].text(field);
                self.rows[index].set_text(field, value);
                if modifiers.shift() {
                    self.undo.push((index, field, current));
                } else {
                    self.redo.push((index, field, current));
                }
                self.status = String::from(if modifiers.shift() {
                    "redo"
                } else {
                    "undo"
                });
            }
            return Task::none();
        }

        match key {
            keyboard::Key::Named(Named::Tab) => {
                let order = self.tab_order();
                let position = order
                    .iter()
                    .position(|entry| *entry == self.focus)
                    .unwrap_or(0);
                let next = if modifiers.shift() {
                    (position + order.len() - 1) % order.len()
                } else {
                    (position + 1) % order.len()
                };
                self.focus_task(order[next])
            }
            keyboard::Key::Named(Named::Enter) => {
                // Spreadsheet convention: Enter moves DOWN the column.
                if let Focus::Cell(index, field) = self.focus {
                    let next = if modifiers.shift() {
                        index.checked_sub(1)
                    } else {
                        (index + 1 < self.rows.len()).then_some(index + 1)
                    };
                    if let Some(next) = next {
                        return self.focus_task(Focus::Cell(next, field));
                    }
                }
                Task::none()
            }
            keyboard::Key::Named(Named::Escape) => {
                if let (Focus::Cell(index, field), Some(previous)) =
                    (self.focus, self.backup.clone())
                    && field.is_text()
                {
                    self.rows[index].set_text(field, previous);
                    self.status = String::from("reverted");
                    return operation::focus(cell_id(index, field));
                }
                Task::none()
            }
            keyboard::Key::Named(Named::ArrowUp)
            | keyboard::Key::Named(Named::ArrowDown) => {
                let up = matches!(key, keyboard::Key::Named(Named::ArrowUp));
                let Focus::Cell(_, field) = self.focus else {
                    return Task::none();
                };
                let base = match field {
                    Field::Qty => Decimal::ONE,
                    Field::Price => Decimal::from_str("0.01").unwrap(),
                    _ => return Task::none(),
                };
                let magnitude = if modifiers.shift() {
                    base * Decimal::TEN
                } else {
                    base
                };
                self.step(if up { magnitude } else { -magnitude })
            }
            keyboard::Key::Named(Named::ArrowLeft)
            | keyboard::Key::Named(Named::ArrowRight) => {
                if self.focus == Focus::Slider {
                    let delta = if matches!(key, keyboard::Key::Named(Named::ArrowRight))
                    {
                        1
                    } else {
                        -1
                    };
                    let current =
                        self.vat_percent().round().try_into().unwrap_or(0i64);
                    let next = (current + delta).clamp(0, 25) as u8;
                    return Task::done(Message::VatSlider(next));
                }
                Task::none()
            }
            keyboard::Key::Named(Named::Space) => match self.focus {
                Focus::Cell(index, Field::Reimb) => {
                    self.rows[index].reimb = !self.rows[index].reimb;
                    Task::none()
                }
                Focus::Save => Task::done(Message::Save),
                _ => Task::none(),
            },
            _ => Task::none(),
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        // `listen_with` sees events regardless of whether a widget captured
        // them — the only way to react to Escape (text_input eats it) and the
        // only way to get Tab at all.
        event::listen_with(|event, _status, _window| match event {
            Event::Keyboard(keyboard::Event::KeyPressed {
                key, modifiers, ..
            }) => Some(Message::Key(key, modifiers)),
            Event::Mouse(iced::mouse::Event::ButtonPressed(_)) => {
                Some(Message::MousePressed)
            }
            _ => None,
        })
    }

    // -- view -------------------------------------------------------------

    fn view(&self) -> Element<'_, Message> {
        let errors = self.errors();

        let vat_value: u8 = self.vat_percent().round().try_into().unwrap_or(0);

        let toolbar = row![
            button(text(format!("Locale: {}", self.locale.label())).size(13))
                .on_press(Message::ToggleLocale),
            text("VAT").size(13),
            slider(0..=25u8, vat_value, Message::VatSlider).width(160),
            self.ring(
                Focus::Vat,
                text_input("", &self.vat)
                    .id(cell_id(usize::MAX, Field::Qty))
                    .on_input(Message::VatInput)
                    .align_x(Horizontal::Right)
                    .font(Font::MONOSPACE)
                    .width(52)
                    .into(),
            ),
            text("%").size(13),
            space::horizontal(),
            self.ring(
                Focus::Save,
                button(
                    text(if errors.is_empty() {
                        String::from("Save")
                    } else {
                        format!("Save ({} errors)", errors.len())
                    })
                    .size(13)
                )
                .on_press_maybe(errors.is_empty().then_some(Message::Save))
                .into(),
            ),
        ]
        .spacing(10)
        .align_y(iced::Center);

        let header = row![
            head("Date", W_DATE, false),
            head("Description", W_DESC, false),
            head("Category", W_CAT, false),
            head("Qty", W_QTY, true),
            head("Unit price", W_PRICE, true),
            head("Amount", W_AMOUNT, true),
            head("Reimb", W_REIMB, false),
        ]
        .spacing(4);

        let mut body = column![].spacing(3);
        for (index, r) in self.rows.iter().enumerate() {
            body = body.push(self.view_row(index, r, &errors));
        }

        let footer = column![
            total_line("Subtotal", format_number(self.subtotal().round_dp(2), 2, self.locale)),
            total_line(
                &format!("VAT {vat_value}%"),
                format_number(self.vat_amount(), 2, self.locale)
            ),
            total_line("Total", format_number(self.total(), 2, self.locale)),
        ]
        .spacing(2);

        let summary: Element<'_, Message> = if errors.is_empty() {
            text(if self.saved {
                String::from("all rows valid · saved")
            } else {
                String::from("all rows valid")
            })
            .size(12)
            .into()
        } else {
            let mut list = column![].spacing(1);
            for (index, field, message) in errors.iter().take(3) {
                list = list.push(
                    text(format!("row {} / {}: {message}", index + 1, field.key()))
                        .size(11)
                        .color(Color::from_rgb(0.85, 0.30, 0.30)),
                );
            }
            if errors.len() > 3 {
                list = list.push(
                    text(format!("… {} more", errors.len() - 3)).size(11),
                );
            }
            list.into()
        };

        column![
            toolbar,
            header,
            scrollable(body).height(Fill),
            container(footer).align_right(Fill).height(Length::Shrink),
            summary,
            text(format!(
                "Tab/Shift+Tab: next cell · Enter/Shift+Enter: down/up · \
                 Up/Down: step (Shift = x10) · Esc: revert · Cmd+Z / Cmd+Shift+Z: \
                 undo/redo · Cmd+Shift+C / Cmd+Shift+V: row TSV · {}",
                self.status
            ))
            .size(10),
        ]
        .spacing(8)
        .padding(12)
        .into()
    }

    fn view_row<'a>(
        &'a self,
        index: usize,
        r: &'a Row,
        errors: &[(usize, Field, String)],
    ) -> Element<'a, Message> {
        let bad = |field: Field| {
            errors.iter().any(|(i, f, _)| *i == index && *f == field)
        };

        let cell = |field: Field, value: &'a str, width: f32, right: bool| {
            let input = text_input("", value)
                .id(cell_id(index, field))
                .on_input(move |v| Message::CellInput(index, field, v))
                .on_paste(move |v| Message::CellPaste(index, field, v))
                .size(13)
                .padding(4)
                .width(width);
            let input = if right {
                input.align_x(Horizontal::Right).font(Font::MONOSPACE)
            } else {
                input
            };
            let invalid = bad(field);
            container(input.style(move |theme: &Theme, status| {
                let mut style = text_input::default(theme, status);
                if invalid {
                    style.border = style
                        .border
                        .color(Color::from_rgb(0.85, 0.30, 0.30))
                        .width(2.0);
                }
                style
            }))
        };

        let amount = r.amount(self.locale);
        let amount_text = match amount {
            Some(value) if value.is_sign_negative() => text(format!(
                "({})",
                format_number(value.abs(), 2, self.locale)
            ))
            .color(Color::from_rgb(0.85, 0.30, 0.30)),
            Some(value) => text(format_number(value, 2, self.locale)),
            None => text("—"),
        };

        let reimb = checkbox(r.reimb)
            .on_toggle(move |v| Message::ReimbToggled(index, v));

        row![
            cell(Field::Date, &r.date, W_DATE, false),
            cell(Field::Desc, &r.desc, W_DESC, false),
            container(
                combo_box(
                    &self.categories[index],
                    "Category",
                    Some(&r.cat),
                    move |c| Message::CategoryPicked(index, c),
                )
                .size(13)
                .width(W_CAT)
            ),
            cell(Field::Qty, &r.qty, W_QTY, true),
            cell(Field::Price, &r.price, W_PRICE, true),
            container(
                amount_text
                    .size(13)
                    .font(Font::MONOSPACE)
                    .width(W_AMOUNT)
                    .align_x(Horizontal::Right)
            )
            .padding(4),
            self.ring(
                Focus::Cell(index, Field::Reimb),
                container(reimb).width(W_REIMB).center_x(W_REIMB).into()
            ),
        ]
        .spacing(4)
        .align_y(iced::Center)
        .into()
    }

    /// The focus ring iced cannot draw: checkbox, slider and button are not
    /// focusable widgets in 0.14, so "focused" is our own state.
    fn ring<'a>(
        &self,
        target: Focus,
        content: Element<'a, Message>,
    ) -> Element<'a, Message> {
        let focused = self.focus == target;
        container(content)
            .padding(2)
            .style(move |theme: &Theme| {
                let palette = theme.extended_palette();
                container::Style {
                    border: Border {
                        color: if focused {
                            palette.primary.base.color
                        } else {
                            Color::TRANSPARENT
                        },
                        width: 2.0,
                        radius: 4.0.into(),
                    },
                    background: None,
                    ..container::Style::default()
                }
            })
            .into()
    }

    // -- scripted self-test (LEDGER_SELFTEST=1) ---------------------------

    fn self_test(&mut self) -> Task<Message> {
        let mut pass = 0u32;
        let mut fail = 0u32;
        let mut check = |name: &str, ok: bool| {
            if ok {
                pass += 1;
            } else {
                fail += 1;
            }
            println!("selftest {}: {name}", if ok { "PASS" } else { "FAIL" });
        };

        // 1. filtered typing: every prefix of "1234.5" must be accepted and a
        //    letter must be rejected.
        let mut typed_ok = true;
        for end in 1..="1234.5".len() {
            let prefix = &"1234.5"[..end];
            let _ = self.update(Message::CellInput(
                0,
                Field::Price,
                prefix.to_string(),
            ));
            typed_ok &= self.rows[0].price == prefix;
        }
        check("typing 1234.5 into a price cell is accepted", typed_ok);

        let _ = self.update(Message::CellInput(0, Field::Price, "1234.5a".into()));
        check("a letter is rejected while typing", self.rows[0].price == "1234.5");
        let _ = self.update(Message::CellInput(0, Field::Price, "1234.5.6".into()));
        check("a second decimal separator is rejected", self.rows[0].price == "1234.5");
        let _ = self.update(Message::CellInput(0, Field::Qty, "1.5".into()));
        check("a decimal separator is rejected in the integer Qty cell", self.rows[0].qty == "1");

        // 2. commit / blur formatting, then the locale toggle.
        self.focus = Focus::Cell(0, Field::Price);
        let _ = self.focus_task(Focus::Cell(0, Field::Desc));
        check(
            "blur formats 1234.5 as 1,234.50 (en-US)",
            self.rows[0].price == "1,234.50",
        );
        let _ = self.update(Message::ToggleLocale);
        check(
            "locale toggle re-renders it as 1\u{00a0}234,50 (fr-FR)",
            self.rows[0].price == "1\u{00a0}234,50",
        );
        let _ = self.update(Message::ToggleLocale);
        check("toggling back restores 1,234.50", self.rows[0].price == "1,234.50");

        // 3. paste of currency / accounting formats.
        let _ = self.update(Message::CellPaste(1, Field::Price, "$1,234.56".into()));
        check("paste $1,234.56", self.rows[1].price == "1,234.56");
        let _ = self.update(Message::CellPaste(1, Field::Price, "1.234,56 €".into()));
        check("paste 1.234,56 €", self.rows[1].price == "1,234.56");
        let _ = self.update(Message::CellPaste(1, Field::Price, "(12.50)".into()));
        check("paste (12.50) is negative", self.rows[1].price == "-12.50");
        let _ = self.update(Message::CellPaste(1, Field::Price, "abc".into()));
        check("paste of garbage keeps the previous value", self.rows[1].price == "-12.50");

        // 4. arrow stepping.
        self.focus = Focus::Cell(2, Field::Price);
        let before = self.rows[2].price.clone();
        let _ = self.on_key(
            keyboard::Key::Named(keyboard::key::Named::ArrowUp),
            keyboard::Modifiers::default(),
        );
        check("Up steps the price by 0.01", self.rows[2].price == "219.01");
        let _ = self.on_key(
            keyboard::Key::Named(keyboard::key::Named::ArrowUp),
            keyboard::Modifiers::SHIFT,
        );
        check("Shift+Up steps by 0.10", self.rows[2].price == "219.11");
        let _ = self.update(Message::CellPaste(2, Field::Price, before));
        self.focus = Focus::Cell(2, Field::Qty);
        let _ = self.on_key(
            keyboard::Key::Named(keyboard::key::Named::ArrowDown),
            keyboard::Modifiers::default(),
        );
        check("Down steps Qty by 1", self.rows[2].qty == "2");

        // 5. computed columns and totals.
        let amount = self.rows[2].amount(self.locale).unwrap_or_default();
        check(
            "Amount = Qty x Unit price",
            amount == Decimal::from_str("438.00").unwrap(),
        );
        let subtotal = self.subtotal().round_dp(2);
        let vat = self.vat_amount();
        check(
            "Total = Subtotal + VAT",
            self.total() == subtotal + vat && vat > Decimal::ZERO,
        );

        // 6. validation, Save gating and the error summary.
        let clean = self.errors().is_empty();
        check("seed rows are all valid", clean);
        let _ = self.update(Message::CellInput(3, Field::Desc, String::new()));
        let errors = self.errors();
        check(
            "empty description is reported for the right row/field",
            errors.iter().any(|(i, f, _)| *i == 3 && *f == Field::Desc),
        );
        check("Save is gated on the error count", !errors.is_empty());
        let _ = self.update(Message::CellInput(3, Field::Desc, "Team dinner".into()));

        // 7. undo / redo of a committed cell edit.
        self.focus = Focus::Cell(4, Field::Price);
        self.backup = Some(self.rows[4].price.clone());
        let original = self.rows[4].price.clone();
        let _ = self.update(Message::CellInput(4, Field::Price, "5".into()));
        self.commit(4, Field::Price);
        check("edit committed as 5.00", self.rows[4].price == "5.00");
        let _ = self.on_key(
            keyboard::Key::Character("z".into()),
            keyboard::Modifiers::COMMAND,
        );
        check("Cmd+Z restores the previous value", self.rows[4].price == original);
        let _ = self.on_key(
            keyboard::Key::Character("z".into()),
            keyboard::Modifiers::COMMAND | keyboard::Modifiers::SHIFT,
        );
        check("Cmd+Shift+Z redoes it", self.rows[4].price == "5.00");

        // 8. TSV round-trip.
        let tsv = self.row_tsv(0);
        check("row copies as 6 tab-separated fields", tsv.split('\t').count() == 6);
        let applied = self.apply_tsv(
            11,
            "2026-02-01\tPasted row\tOffice\t3\t7.25\tyes",
        );
        check(
            "TSV pastes back into a row",
            applied
                && self.rows[11].desc == "Pasted row"
                && self.rows[11].price == "7.25"
                && self.rows[11].cat == Category::Office,
        );

        // 9. navigation model.
        let order = self.tab_order();
        check(
            "tab order covers every cell plus VAT/slider/Save",
            order.len() == self.rows.len() * 6 + 3
                && order[2] == Focus::Cell(0, Field::Cat)
                && order[5] == Focus::Cell(0, Field::Reimb),
        );
        self.focus = Focus::Cell(0, Field::Qty);
        let _ = self.on_key(
            keyboard::Key::Named(keyboard::key::Named::Enter),
            keyboard::Modifiers::default(),
        );
        check(
            "Enter moves down the same column",
            self.focus == Focus::Cell(1, Field::Qty),
        );
        let _ = self.on_key(
            keyboard::Key::Named(keyboard::key::Named::Enter),
            keyboard::Modifiers::SHIFT,
        );
        check(
            "Shift+Enter moves up the same column",
            self.focus == Focus::Cell(0, Field::Qty),
        );
        self.focus = Focus::Cell(0, Field::Price);
        self.backup = Some(self.rows[0].price.clone());
        let kept = self.rows[0].price.clone();
        let _ = self.update(Message::CellInput(0, Field::Price, "9".into()));
        let _ = self.on_key(
            keyboard::Key::Named(keyboard::key::Named::Escape),
            keyboard::Modifiers::default(),
        );
        check("Esc reverts the uncommitted edit", self.rows[0].price == kept);

        // 10. decimal alignment invariant.
        let aligned = self.rows.iter().all(|row| {
            row.price
                .rsplit_once(self.locale.decimal())
                .is_some_and(|(_, frac)| frac.len() == 2)
        });
        check("every price carries exactly 2 fraction digits", aligned);

        println!("SELFTEST DONE pass={pass} fail={fail}");
        iced::exit()
    }
}

fn head(label: &str, width: f32, right: bool) -> Element<'_, Message> {
    let content = text(label).size(11).width(width);
    container(if right {
        content.align_x(Horizontal::Right)
    } else {
        content
    })
    .padding(4)
    .into()
}

fn total_line<'a>(label: &str, value: String) -> Element<'a, Message> {
    row![
        text(label.to_string()).size(13).width(120).align_x(Horizontal::Right),
        text(value)
            .size(13)
            .font(Font::MONOSPACE)
            .width(110)
            .align_x(Horizontal::Right),
    ]
    .spacing(8)
    .into()
}
