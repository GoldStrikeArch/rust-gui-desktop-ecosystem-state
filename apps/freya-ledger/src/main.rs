//! "Ledger" — forms & numeric input on Freya 0.4 (SPEC-10).
//!
//! 12-row expense table with typed numeric cells, locale-aware parsing and
//! formatting, decimal-aligned money columns, inline validation, a type-ahead
//! category cell, a VAT slider linked to a numeric field, spreadsheet
//! Enter-moves-down navigation and live totals.
//!
//! Number model: the widget carries a `String` (Freya's `Input` is a text
//! editor and nothing else), the model carries `rust_decimal::Decimal`, and
//! parsing happens twice — once per keystroke in `Input::on_validate`, which is
//! a real *filter* (returning invalid makes the editor undo the keystroke), and
//! once on blur, where the committed value is re-formatted for the current
//! locale. See FRICTION.md.

use std::{
    cell::Cell,
    time::Duration,
};

use async_io::Timer;
use freya::prelude::*;
use rust_decimal::{
    Decimal,
    prelude::*,
};


/// Character widths the numeric columns are padded to. Freya's
/// `Input::text_align(Right)` is broken (see FRICTION.md), so right alignment
/// is achieved by left-padding the *text* inside a monospaced face — which is
/// also what makes the decimal points line up across rows.
const PAD_QTY: usize = 3;
const PAD_UNIT: usize = 10;

fn pad_left(text: &str, width: usize) -> String {
    let missing = width.saturating_sub(text.chars().count());
    let mut out = " ".repeat(missing);
    out.push_str(text);
    out
}

/// Freya cannot request OpenType features (no `font_features`/`tnum` anywhere in
/// freya-core's text style), so tabular figures come from a monospaced face.
const MONO: &str = "Menlo";

// ------------------------------------------------------------------- model

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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

impl Category {
    fn label(self) -> &'static str {
        match self {
            Category::Travel => "Travel",
            Category::Lodging => "Lodging",
            Category::Meals => "Meals",
            Category::Training => "Training",
            Category::Hardware => "Hardware",
            Category::Software => "Software",
            Category::Office => "Office",
        }
    }
}

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
            // U+0020: a real NNBSP would be more correct but survives neither
            // the TSV round-trip nor a screenshot diff legibly.
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

/// Which editable thing a cell is; also the Tab order within a row.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Field {
    Date,
    Desc,
    Category,
    Qty,
    Unit,
}

impl Field {
    fn label(self) -> &'static str {
        match self {
            Field::Date => "Date",
            Field::Desc => "Description",
            Field::Category => "Category",
            Field::Qty => "Qty",
            Field::Unit => "Unit price",
        }
    }
}

/// One row. Every editable value is its own signal so a cell edit re-renders
/// only that cell plus the totals.
#[derive(Clone, Copy)]
struct Cells {
    date: State<String>,
    desc: State<String>,
    cat: State<usize>,
    qty: State<String>,
    unit: State<String>,
    reimb: State<bool>,
    ids: [AccessibilityId; 5],
}

impl Cells {
    fn text(&self, field: Field) -> Option<State<String>> {
        match field {
            Field::Date => Some(self.date),
            Field::Desc => Some(self.desc),
            Field::Qty => Some(self.qty),
            Field::Unit => Some(self.unit),
            Field::Category => None,
        }
    }

    fn id(&self, field: Field) -> AccessibilityId {
        self.ids[match field {
            Field::Date => 0,
            Field::Desc => 1,
            Field::Category => 2,
            Field::Qty => 3,
            Field::Unit => 4,
        }]
    }
}

#[derive(Clone, Copy)]
struct Ledger {
    rows: State<Vec<Cells>>,
    locale: State<Locale>,
    vat: State<Decimal>,
    vat_text: State<String>,
    /// Form-level undo of committed cell edits: (row, field, previous text).
    history: State<Vec<(usize, Field, String)>>,
    /// Text of the focused cell as it was when focus entered it. Freya's
    /// `Input` has no `on_blur` and no "revert" — both are built on this.
    snapshot: State<Option<(AccessibilityId, String)>>,
    status: State<String>,
    selftest: bool,
}

// -------------------------------------------------------- number plumbing

/// Strip everything a human might paste around a number: currency symbols,
/// spaces, and the accounting negative `(12.50)`.
fn parse_loose(raw: &str, locale: Locale) -> Option<Decimal> {
    let mut text = raw.trim().to_string();
    let mut negative = false;
    if text.starts_with('(') && text.ends_with(')') {
        negative = true;
        text = text[1..text.len() - 1].to_string();
    }
    text.retain(|c| !matches!(c, '$' | '€' | '£' | '¥' | '\u{a0}' | '\u{202f}'));
    let text = text.trim().to_string();
    if text.is_empty() {
        return None;
    }

    // Decide which separator is decimal: prefer the current locale, but accept
    // the other convention when it is unambiguous (`1.234,56` pasted into
    // en-US must still parse).
    let (decimal, group) = match (text.rfind('.'), text.rfind(',')) {
        (Some(dot), Some(comma)) if dot > comma => ('.', ','),
        (Some(dot), Some(comma)) if comma > dot => (',', '.'),
        (Some(_), None) if locale == Locale::FrFr && looks_grouped(&text, '.') => ('.', '.'),
        (None, Some(_)) if locale == Locale::EnUs && looks_grouped(&text, ',') => (',', ','),
        (Some(_), None) => ('.', ','),
        (None, Some(_)) => (',', '.'),
        _ => (locale.decimal(), locale.group()),
    };

    let mut cleaned = String::new();
    for ch in text.chars() {
        match ch {
            '-' if cleaned.is_empty() => cleaned.push('-'),
            c if c.is_ascii_digit() => cleaned.push(c),
            c if c == decimal && decimal != group => cleaned.push('.'),
            c if c == group || c == ' ' => {}
            _ => return None,
        }
    }
    if decimal == group {
        // Both separators are the grouping character: no fractional part.
        cleaned.retain(|c| c != '.');
    }
    let value = Decimal::from_str(&cleaned).ok()?;
    Some(if negative { -value } else { value })
}

/// `1.234` in fr-FR is a grouped thousand, not 1.234 — but only if the run
/// after the separator is exactly three digits.
fn looks_grouped(text: &str, sep: char) -> bool {
    text.rsplit(sep)
        .next()
        .map(|tail| tail.len() == 3 && tail.chars().all(|c| c.is_ascii_digit()))
        .unwrap_or(false)
}

/// A keystroke filter: is `text` something that could still become a number?
fn is_partial_number(text: &str, locale: Locale) -> bool {
    if text.is_empty() {
        return true;
    }
    let mut seen_decimal = false;
    for (index, ch) in text.chars().enumerate() {
        match ch {
            '-' if index == 0 => {}
            '(' if index == 0 => {}
            ')' => {}
            c if c.is_ascii_digit() => {}
            c if c == locale.decimal() => {
                if seen_decimal {
                    return false;
                }
                seen_decimal = true;
            }
            c if c == locale.group() || c == ' ' => {}
            _ => return false,
        }
    }
    true
}

fn format_decimal(value: Decimal, places: u32, locale: Locale) -> String {
    let value = value.round_dp(places);
    let negative = value.is_sign_negative();
    let text = value.abs().to_string();
    let (int_part, frac_part) = match text.split_once('.') {
        Some((i, f)) => (i.to_string(), f.to_string()),
        None => (text, String::new()),
    };
    let mut frac = frac_part;
    while frac.len() < places as usize {
        frac.push('0');
    }
    frac.truncate(places as usize);

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
    if places > 0 {
        out.push(locale.decimal());
        out.push_str(&frac);
    }
    out
}

fn amount(row: &Cells, locale: Locale) -> Decimal {
    // `read()`, not `peek()`: this is what subscribes the row (and the totals
    // row) to the two cells it is computed from.
    let qty = parse_loose(&row.qty.read(), locale).unwrap_or_default();
    let unit = parse_loose(&row.unit.read(), locale).unwrap_or_default();
    (qty * unit).round_dp(2)
}

// --------------------------------------------------------------- validation

fn cell_error(row: &Cells, field: Field, locale: Locale) -> Option<&'static str> {
    match field {
        Field::Date => {
            let text = row.date.read().clone();
            let ok = text.len() == 10
                && text.as_bytes()[4] == b'-'
                && text.as_bytes()[7] == b'-'
                && text
                    .chars()
                    .enumerate()
                    .all(|(i, c)| if i == 4 || i == 7 { c == '-' } else { c.is_ascii_digit() })
                && (1..=12).contains(&text[5..7].parse::<u32>().unwrap_or(0))
                && (1..=31).contains(&text[8..10].parse::<u32>().unwrap_or(0));
            (!ok).then_some("date must be YYYY-MM-DD")
        }
        Field::Desc => {
            let len = row.desc.read().trim().chars().count();
            if len == 0 {
                Some("description is required")
            } else if len > 60 {
                Some("description is longer than 60 characters")
            } else {
                None
            }
        }
        Field::Qty => match parse_loose(&row.qty.read(), locale) {
            Some(value) if value.fract().is_zero() && value >= Decimal::ONE && value <= Decimal::from(999) => None,
            _ => Some("qty must be a whole number 1–999"),
        },
        Field::Unit => match parse_loose(&row.unit.read(), locale) {
            Some(value)
                if value.abs() <= Decimal::from_str("99999.99").unwrap() =>
            {
                None
            }
            _ => Some("unit price must be −99 999.99 … 99 999.99"),
        },
        Field::Category => None,
    }
}

const EDITABLE: [Field; 4] = [Field::Date, Field::Desc, Field::Qty, Field::Unit];

// ------------------------------------------------------------------- main

fn main() {
    #[cfg(debug_assertions)]
    std::panic::set_hook(Box::new(|info| eprintln!("PANIC: {info}")));

    launch(
        LaunchConfig::new().with_window(
            WindowConfig::new(app)
                .with_title("Ledger (freya)")
                .with_size(980.0, 620.0)
                .with_window_attributes(|attrs, _| {
                    let Some(raw) = std::env::var("LEDGER_ORIGIN").ok() else {
                        return attrs;
                    };
                    let Some((x, y)) = raw.split_once(',') else {
                        return attrs;
                    };
                    match (x.trim().parse::<f64>(), y.trim().parse::<f64>()) {
                        (Ok(x), Ok(y)) => attrs.with_position(
                            freya::winit::dpi::LogicalPosition::new(x, y),
                        ),
                        _ => attrs,
                    }
                }),
        ),
    )
}

fn seed() -> Vec<(&'static str, &'static str, usize, &'static str, &'static str, bool)> {
    vec![
        ("2026-01-08", "Flight LHR-JFK", 0, "1", "482.50", true),
        ("2026-01-09", "Airport transfer", 0, "2", "34.75", true),
        ("2026-01-09", "Hotel, 3 nights", 1, "3", "219.00", true),
        ("2026-01-10", "Team dinner", 2, "6", "41.80", true),
        ("2026-01-11", "Conference pass", 3, "1", "1295.00", true),
        ("2026-01-11", "Duplicate charge refund", 0, "1", "-34.75", false),
        ("2026-01-12", "USB-C dock", 4, "1", "189.99", true),
        ("2026-01-13", "IDE licence", 5, "4", "99.00", true),
        ("2026-01-14", "Printer paper", 6, "12", "5.99", false),
        ("2026-01-15", "Taxi to venue", 0, "2", "18.40", true),
        ("2026-01-16", "Breakfast", 2, "5", "12.25", false),
        ("2026-01-17", "Monitor stand", 4, "1", "76.00", true),
    ]
}

// ------------------------------------------------------------------ colours

const BG: Color = Color::new(0xFF16181D);
const PANEL: Color = Color::new(0xFF1E2129);
const LINE: Color = Color::new(0xFF2C303A);
const TEXT: Color = Color::new(0xFFE8EAF0);
const MUTED: Color = Color::new(0xFF9198A8);
const ACCENT: Color = Color::new(0xFF4C8DFF);
const BAD: Color = Color::new(0xFFFF6B6B);

const COL_W: [f32; 7] = [112.0, 210.0, 120.0, 74.0, 116.0, 116.0, 70.0];

fn app() -> impl IntoElement {
    // Freya's components default to `light_theme()`; the built-in `Input`
    // paints white-on-white on this app's dark ground otherwise.
    use_init_theme(dark_theme);
    let ledger = use_hook(|| Ledger {
        rows: State::create(
            seed()
                .into_iter()
                .map(|(date, desc, cat, qty, unit, reimb)| Cells {
                    date: State::create(date.to_string()),
                    desc: State::create(desc.to_string()),
                    cat: State::create(cat),
                    qty: State::create(pad_left(qty, PAD_QTY)),
                    unit: State::create(pad_left(
                        &format_decimal(Decimal::from_str(unit).unwrap(), 2, Locale::EnUs),
                        PAD_UNIT,
                    )),
                    reimb: State::create(reimb),
                    ids: std::array::from_fn(|_| AccessibilityId::new_unique()),
                })
                .collect::<Vec<_>>(),
        ),
        locale: State::create(Locale::EnUs),
        vat: State::create(Decimal::from_str("19").unwrap()),
        vat_text: State::create(" 19.00".to_string()),
        history: State::create(Vec::new()),
        snapshot: State::create(None),
        status: State::create("Ready".to_string()),
        selftest: std::env::var_os("LEDGER_SELFTEST").is_some(),
    });

    // ---- blur handling -----------------------------------------------------
    // Freya's `Input` has no `on_blur`. The focused node is a reactive
    // `Platform::focused_accessibility_id`, so "the previous cell lost focus"
    // is derived from watching it: commit + re-format whatever it used to be,
    // and snapshot the new cell's text so Escape can revert it.
    let focus_log = use_hook(|| std::env::var_os("LEDGER_FOCUSLOG").is_some());
    use_side_effect(move || {
        let platform = Platform::get();
        let focused = *platform.focused_accessibility_id.read();
        let node = platform.focused_accessibility_node.read().clone();
        let previous = ledger.snapshot.peek().clone();
        if previous.as_ref().map(|(id, _)| *id) == Some(focused) {
            return;
        }
        if let Some((id, original)) = previous {
            blur_cell(ledger, id, original);
        }
        if focus_log {
            println!(
                "FOCUS {:?} label={:?} value={:?}",
                node.role(),
                node.label().unwrap_or_default(),
                node.value().unwrap_or_default()
            );
        }
        let text = find_cell(ledger, focused)
            .and_then(|(row, field)| ledger.rows.peek()[row].text(field).map(|s| s.peek().clone()))
            .unwrap_or_default();
        let mut ledger = ledger;
        ledger.snapshot.set(Some((focused, text)));
    });

    if ledger.selftest {
        use_hook(move || {
            spawn(async move { selftest(ledger).await });
        });
    }

    let locale = *ledger.locale.read();
    let rows = ledger.rows.read().clone();
    let vat = *ledger.vat.read();

    let mut subtotal = Decimal::ZERO;
    for row in &rows {
        subtotal += amount(row, locale);
    }
    let vat_amount = (subtotal * vat / Decimal::from(100)).round_dp(2);
    let total = subtotal + vat_amount;

    let mut errors: Vec<String> = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        for field in EDITABLE {
            if let Some(message) = cell_error(row, field, locale) {
                errors.push(format!("row {} · {}: {message}", index + 1, field.label()));
            }
        }
    }
    let error_count = errors.len();

    rect()
        .expanded()
        .content(Content::flex())
        .background(BG)
        .color(TEXT)
        .a11y_focusable(true)
        .a11y_auto_focus(true)
        .on_global_key_down(move |e: Event<KeyboardEventData>| global_keys(ledger, e))
        .child(toolbar(ledger, locale, vat, error_count))
        .child(header())
        .child(
            ScrollView::new().height(Size::flex(1.)).children(
                rows.iter()
                    .enumerate()
                    .map(|(index, row)| row_view(ledger, index, *row, locale).into())
                    .collect::<Vec<Element>>(),
            ),
        )
        .child(footer(locale, subtotal, vat, vat_amount, total))
        .child(error_summary(ledger, errors))
}

// ------------------------------------------------------------------ toolbar

fn toolbar(ledger: Ledger, locale: Locale, vat: Decimal, error_count: usize) -> Element {
    let mut ledger = ledger;
    rect()
        .horizontal()
        .content(Content::flex())
        .height(Size::px(46.))
        .cross_align(Alignment::Center)
        .spacing(10.)
        .padding(Gaps::new_symmetric(0., 10.))
        .background(PANEL)
        .child(
            Button::new()
                .compact()
                .on_press(move |_| {
                    let next = if locale == Locale::EnUs {
                        Locale::FrFr
                    } else {
                        Locale::EnUs
                    };
                    ledger.locale.set(next);
                    reformat_all(ledger, next);
                    ledger.status.set(format!("Locale: {}", next.label()));
                    println!("LOCALE {}", next.label());
                })
                .child(label().text(format!("Locale: {}", locale.label())).font_size(12.)),
        )
        .child(label().text("VAT %").font_size(12.).color(MUTED))
        .child(
            rect().width(Size::px(150.)).child(
                Slider::new(move |percent: f64| {
                    // Freya's Slider is percentage-only (0..=100), so the 0..25
                    // domain is mapped by hand, in both directions.
                    let value = (Decimal::from_f64(percent).unwrap_or_default()
                        * Decimal::from(25)
                        / Decimal::from(100))
                    .round_dp(2);
                    ledger.vat.set(value);
                    ledger.vat_text.set(pad_left(
                        &format_decimal(value, 2, *ledger.locale.peek()),
                        6,
                    ));
                })
                .value(
                    (vat * Decimal::from(100) / Decimal::from(25))
                        .to_f64()
                        .unwrap_or(0.0),
                ),
            ),
        )
        .child(
            rect().font_family(MONO).child(
            Input::new(ledger.vat_text)
                .compact()
                .width(Size::px(76.))
                .on_validate(move |validator: InputValidator| {
                    let text = validator.text().clone();
                    if !is_partial_number(&text, locale) {
                        validator.set_valid(false);
                        return;
                    }
                    if let Some(value) = parse_loose(&text, locale)
                        && value >= Decimal::ZERO
                        && value <= Decimal::from(25)
                    {
                        ledger.vat.set(value);
                    }
                }),
        ))
        .child(
            Button::new()
                .compact()
                .on_press(move |_| copy_row(ledger))
                .child(label().text("Copy row").font_size(12.)),
        )
        .child(
            Button::new()
                .compact()
                .on_press(move |_| paste_row(ledger))
                .child(label().text("Paste row").font_size(12.)),
        )
        .child(
            Button::new()
                .compact()
                .on_press(move |_| undo_form(ledger))
                .child(label().text("Undo edit").font_size(12.)),
        )
        .child(rect().width(Size::flex(1.)))
        .child(
            label()
                .text(ledger.status.read().clone())
                .font_size(11.)
                .color(MUTED)
                .max_lines(1),
        )
        .child(
            Button::new()
                .compact()
                .enabled(error_count == 0)
                .filled()
                .on_press(move |_| {
                    ledger.status.set("Saved".into());
                    println!("SAVE ok");
                })
                .child(
                    label()
                        .text(if error_count == 0 {
                            "Save".to_string()
                        } else {
                            format!("Save ({error_count})")
                        })
                        .font_size(12.),
                ),
        )
        .into()
}

fn header() -> Element {
    let titles = [
        "Date",
        "Description",
        "Category",
        "Qty",
        "Unit price",
        "Amount",
        "Reimb.",
    ];
    rect()
        .horizontal()
        .height(Size::px(26.))
        .cross_align(Alignment::Center)
        .spacing(6.)
        .padding(Gaps::new_symmetric(0., 8.))
        .background(PANEL)
        .children(
            titles
                .iter()
                .enumerate()
                .map(|(index, title)| {
                    rect()
                        .width(Size::px(COL_W[index]))
                        .a11y_role(AccessibilityRole::ColumnHeader)
                        .main_align(if index >= 3 && index <= 5 {
                            Alignment::End
                        } else {
                            Alignment::Start
                        })
                        .horizontal()
                        .child(
                            label()
                                .text(*title)
                                .font_size(11.)
                                .font_weight(FontWeight::BOLD)
                                .color(MUTED),
                        )
                        .into()
                })
                .collect::<Vec<Element>>(),
        )
        .into()
}

// --------------------------------------------------------------------- row

fn row_view(ledger: Ledger, index: usize, row: Cells, locale: Locale) -> Element {
    let mut row_mut = row;
    let value = amount(&row, locale);
    rect()
        .key(index)
        .horizontal()
        .height(Size::px(30.))
        .cross_align(Alignment::Center)
        .spacing(6.)
        .padding(Gaps::new_symmetric(0., 8.))
        .a11y_role(AccessibilityRole::Row)
        .background(if index % 2 == 1 { PANEL } else { BG })
        .child(text_cell(ledger, index, row, Field::Date, locale, false))
        .child(text_cell(ledger, index, row, Field::Desc, locale, false))
        .child(category_cell(ledger, index, row))
        .child(text_cell(ledger, index, row, Field::Qty, locale, true))
        .child(text_cell(ledger, index, row, Field::Unit, locale, true))
        // Amount: computed, right-aligned, monospaced digits, red when negative.
        .child(
            rect()
                .width(Size::px(COL_W[5]))
                .horizontal()
                .main_align(Alignment::End)
                .a11y_role(AccessibilityRole::Cell)
                .a11y_alt(format!("Amount {}", format_decimal(value, 2, locale)))
                .child(
                    label()
                        .text(format_decimal(value, 2, locale))
                        .font_family(MONO)
                        .font_size(12.)
                        .color(if value.is_sign_negative() { BAD } else { TEXT })
                        .max_lines(1),
                ),
        )
        .child(
            rect()
                .width(Size::px(COL_W[6]))
                .horizontal()
                .main_align(Alignment::Center)
                .a11y_role(AccessibilityRole::Cell)
                // Freya's `Checkbox` builds its own AccessibilityId and exposes
                // no `a11y_alt`, so the name can only be put on the cell around
                // it — the focused node itself stays unnamed.
                .a11y_alt(format!("Reimbursable row {}", index + 1))
                .child(
                    Tile::new()
                        .on_select(move |_| {
                            row_mut.reimb.toggle();
                        })
                        .child(Checkbox::new().selected(*row.reimb.read())),
                ),
        )
        .into()
}

/// A text/numeric cell: a Freya `Input` with a typing filter, arrow stepping,
/// Enter-moves-down, Escape-reverts and a red border while invalid.
fn text_cell(
    ledger: Ledger,
    index: usize,
    row: Cells,
    field: Field,
    locale: Locale,
    numeric: bool,
) -> Element {
    let column = match field {
        Field::Date => 0,
        Field::Desc => 1,
        Field::Qty => 3,
        Field::Unit => 4,
        Field::Category => 2,
    };
    let id = row.id(field);
    let state = row.text(field).expect("text field");
    let invalid = cell_error(&row, field, locale).is_some();

    let input = Input::new(state)
        .a11y_id(id)
        .compact()
        .width(Size::px(COL_W[column] - 4.))
        .on_validate(move |validator: InputValidator| {
            let text = validator.text().clone();
            match field {
                // Numeric filter: only characters that can still form a number
                // in the active locale — but always accept something that
                // `parse_loose` understands, so a pasted `$1,234.56` survives.
                Field::Qty | Field::Unit => {
                    if !is_partial_number(&text, locale) && parse_loose(&text, locale).is_none() {
                        validator.set_valid(false);
                    }
                }
                // Date mask: digits and dashes only, dashes only at 4 and 7.
                Field::Date => {
                    let ok = text.len() <= 10
                        && text.chars().enumerate().all(|(i, c)| {
                            if i == 4 || i == 7 {
                                c == '-'
                            } else {
                                c.is_ascii_digit()
                            }
                        });
                    if !ok {
                        validator.set_valid(false);
                    }
                }
                Field::Desc => {
                    if text.chars().count() > 60 {
                        validator.set_valid(false);
                    }
                }
                Field::Category => {}
            }
        })
        .on_pre_key_down(move |e: Event<KeyboardEventData>| {
            cell_keys(ledger, index, row, field, numeric, &e)
        });

    rect()
        .width(Size::px(COL_W[column]))
        .horizontal()
        .main_align(Alignment::Start)
        .maybe(numeric, |el| el.font_family(MONO))
        .a11y_role(AccessibilityRole::Cell)
        .a11y_alt(format!("{} row {}", field.label(), index + 1))
        .maybe(invalid, |el| {
            el.border(Border::new().fill(BAD).width(1.).alignment(BorderAlignment::Outer))
        })
        .child(input)
        .into()
}

/// Keyboard for a cell. `Input`'s stock `on_pre_key_down` swallows everything
/// that is not Enter/Escape/Shift/Tab (it calls `stop_propagation`), so
/// replacing it is the only place stepping, Enter-moves-down and the app's own
/// shortcuts can live.
fn cell_keys(
    ledger: Ledger,
    index: usize,
    row: Cells,
    field: Field,
    numeric: bool,
    e: &Event<KeyboardEventData>,
) -> bool {
    let locale = *ledger.locale.peek();
    let meta = e.modifiers.contains(Modifiers::META) || e.modifiers.contains(Modifiers::CONTROL);
    let shift = e.modifiers.contains(Modifiers::SHIFT);

    if meta {
        // ⌘⇧Z / ⌘⇧C / ⌘⇧V are the app's; plain ⌘Z/⌘C/⌘V belong to the editor.
        if shift && let Key::Character(c) = &e.key {
            match c.to_ascii_lowercase().as_str() {
                "z" => {
                    undo_form(ledger);
                    return false;
                }
                "c" => {
                    copy_row_at(ledger, index);
                    return false;
                }
                "v" => {
                    paste_row_at(ledger, index);
                    return false;
                }
                _ => {}
            }
        }
        return true;
    }

    match &e.key {
        Key::Named(NamedKey::ArrowUp) | Key::Named(NamedKey::ArrowDown) if numeric => {
            e.stop_propagation();
            e.prevent_default();
            let up = e.key == Key::Named(NamedKey::ArrowUp);
            let base = if field == Field::Qty {
                Decimal::ONE
            } else {
                Decimal::from_str("0.01").unwrap()
            };
            let step = if shift { base * Decimal::from(10) } else { base };
            let mut state = row.text(field).unwrap();
            let current = parse_loose(&state.peek(), locale).unwrap_or_default();
            let next = if up { current + step } else { current - step };
            let (places, width) = if field == Field::Qty {
                (0, PAD_QTY)
            } else {
                (2, PAD_UNIT)
            };
            state.set(pad_left(&format_decimal(next, places, locale), width));
            false
        }
        // Spreadsheet convention: Enter moves down the column, ⇧Enter up.
        Key::Named(NamedKey::Enter) => {
            e.stop_propagation();
            e.prevent_default();
            commit_cell(ledger, row.id(field));
            let count = ledger.rows.peek().len();
            let next = if shift {
                (index + count - 1) % count
            } else {
                (index + 1) % count
            };
            ledger.rows.peek()[next].id(field).request_focus();
            false
        }
        // Escape reverts the uncommitted edit, then lets `Input` drop focus.
        Key::Named(NamedKey::Escape) => {
            let mut ledger = ledger;
            let snapshot = ledger.snapshot.peek().clone();
            if let Some((id, original)) = snapshot
                && id == row.id(field)
                && let Some(mut state) = row.text(field)
            {
                state.set(original.clone());
                ledger.snapshot.set(Some((id, original)));
                ledger.status.set(format!("Reverted row {} · {}", index + 1, field.label()));
                println!("ESC revert row {} {:?}", index + 1, field);
            }
            true
        }
        Key::Named(NamedKey::Shift) => true,
        Key::Named(NamedKey::Tab) => false,
        _ => {
            e.stop_propagation();
            e.prevent_default();
            true
        }
    }
}

fn pick_category(
    ledger: Ledger,
    index: usize,
    cat: State<usize>,
    open: State<bool>,
    next: usize,
) {
    let (mut cat, mut open, mut ledger) = (cat, open, ledger);
    cat.set(next);
    open.set(false);
    ledger
        .status
        .set(format!("row {} category {}", index + 1, CATEGORIES[next].label()));
}

/// Category: hand-rolled so it can do type-ahead. Freya ships `Select`, but it
/// only opens on a pointer press and has no keyboard search.
fn category_cell(ledger: Ledger, index: usize, row: Cells) -> Element {
    let id = row.id(Field::Category);
    let cat = row.cat;
    let open = use_state(|| false);
    let mut open_mut = open;
    let selected = *cat.read();
    let focused = use_focus(id);


    rect()
        .width(Size::px(COL_W[2]))
        .a11y_id(id)
        .a11y_focusable(true)
        .a11y_role(AccessibilityRole::ComboBox)
        .a11y_alt(format!("Category row {}", index + 1))
        .a11y_builder(|node| node.set_value(CATEGORIES[selected].label()))
        .maybe(focused().is_focused(), |el| {
            el.border(Border::new().fill(ACCENT).width(2.).alignment(BorderAlignment::Inner))
        })
        // NOT `on_press`: the previously focused `Input` handles
        // `on_global_pointer_press` by calling `request_unfocus()`, which lands
        // *after* our request and moves focus back to the window root.
        // `on_focus_press` + prevent_default is the ordering the built-in
        // components use.
        .on_focus_press(move |e: Event<FocusPressEventData>| {
            e.stop_propagation();
            e.prevent_default();
            id.request_focus();
            open_mut.toggle();
        })
        .on_key_down(move |e: Event<KeyboardEventData>| {
            match &e.key {
                // Type-ahead: jump to the next category starting with the typed
                // letter, wrapping around.
                Key::Character(c) if !e.modifiers.contains(Modifiers::META) => {
                    let needle = c.to_lowercase();
                    let start = *cat.peek();
                    for offset in 1..=CATEGORIES.len() {
                        let candidate = (start + offset) % CATEGORIES.len();
                        if CATEGORIES[candidate]
                            .label()
                            .to_lowercase()
                            .starts_with(&needle)
                        {
                            e.stop_propagation();
                            pick_category(ledger, index, cat, open, candidate);
                            return;
                        }
                    }
                }
                Key::Named(NamedKey::ArrowDown) => {
                    e.stop_propagation();
                    pick_category(ledger, index, cat, open, (*cat.peek() + 1) % CATEGORIES.len());
                }
                Key::Named(NamedKey::ArrowUp) => {
                    e.stop_propagation();
                    pick_category(ledger, index, cat, open, (*cat.peek() + CATEGORIES.len() - 1) % CATEGORIES.len());
                }
                Key::Named(NamedKey::Escape) => open_mut.set(false),
                _ => {}
            }
        })
        .child(
            label()
                .text(CATEGORIES[selected].label())
                .font_size(12.)
                .max_lines(1),
        )
        .maybe_child(open().then(|| -> Element {
            rect()
                .layer(Layer::Overlay)
                .position(Position::new_absolute().top(22.).left(0.))
                .width(Size::px(COL_W[2]))
                .background(PANEL)
                .border(Border::new().fill(LINE).width(1.).alignment(BorderAlignment::Inner))
                .children(
                    CATEGORIES
                        .iter()
                        .enumerate()
                        .map(|(i, category)| {
                            rect()
                                .key(i)
                                .height(Size::px(22.))
                                .padding(Gaps::new_symmetric(0., 6.))
                                .main_align(Alignment::Center)
                                .background(if i == selected { LINE } else { PANEL })
                                .on_press(move |_| pick_category(ledger, index, cat, open, i))
                                .child(label().text(category.label()).font_size(12.))
                                .into()
                        })
                        .collect::<Vec<Element>>(),
                )
                .into()
        }))
        .into()
}

// ------------------------------------------------------------------ footers

fn footer(
    locale: Locale,
    subtotal: Decimal,
    vat: Decimal,
    vat_amount: Decimal,
    total: Decimal,
) -> Element {
    let money = |value: Decimal| {
        label()
            .text(format_decimal(value, 2, locale))
            .font_family(MONO)
            .font_size(12.)
            .max_lines(1)
    };
    rect()
        .horizontal()
        .height(Size::px(32.))
        .cross_align(Alignment::Center)
        .padding(Gaps::new_symmetric(0., 8.))
        .background(PANEL)
        .spacing(10.)
        .child(label().text("Subtotal").font_size(11.).color(MUTED))
        .child(rect().width(Size::px(90.)).horizontal().main_align(Alignment::End).child(money(subtotal)))
        .child(
            label()
                .text(format!("VAT {}%", format_decimal(vat, 2, locale)))
                .font_size(11.)
                .color(MUTED),
        )
        .child(rect().width(Size::px(90.)).horizontal().main_align(Alignment::End).child(money(vat_amount)))
        .child(label().text("Total").font_size(11.).color(MUTED))
        .child(
            rect()
                .width(Size::px(110.))
                .horizontal()
                .main_align(Alignment::End)
                .child(money(total).font_weight(FontWeight::BOLD)),
        )
        .into()
}

fn error_summary(ledger: Ledger, errors: Vec<String>) -> Element {
    if errors.is_empty() {
        return rect()
            .height(Size::px(26.))
            .cross_align(Alignment::Center)
            .padding(Gaps::new_symmetric(0., 8.))
            .child(
                label()
                    .text(format!("no errors · {}", ledger.status.read().clone()))
                    .font_size(11.)
                    .color(MUTED),
            )
            .into();
    }
    rect()
        .height(Size::px(70.))
        .background(Color::new(0xFF2A1B1D))
        .padding(Gaps::new_all(6.))
        .child(
            ScrollView::new().children(
                errors
                    .iter()
                    .enumerate()
                    .map(|(i, message)| {
                        label()
                            .key(i)
                            .text(message.clone())
                            .font_size(11.)
                            .color(BAD)
                            .into()
                    })
                    .collect::<Vec<Element>>(),
            ),
        )
        .into()
}

// ---------------------------------------------------------------- behaviour

fn find_cell(ledger: Ledger, id: AccessibilityId) -> Option<(usize, Field)> {
    let rows = ledger.rows.peek();
    for (index, row) in rows.iter().enumerate() {
        for field in [
            Field::Date,
            Field::Desc,
            Field::Category,
            Field::Qty,
            Field::Unit,
        ] {
            if row.id(field) == id {
                return Some((index, field));
            }
        }
    }
    None
}

/// Normalise + re-format a cell's text and record the change for form-level
/// undo. Called when a cell loses focus and on Enter.
fn commit_cell(ledger: Ledger, id: AccessibilityId) {
    let Some((index, field)) = find_cell(ledger, id) else {
        return;
    };
    let locale = *ledger.locale.peek();
    let row = ledger.rows.peek()[index];
    let Some(mut state) = row.text(field) else {
        return;
    };
    let before = state.peek().clone();
    let after = match field {
        Field::Qty => parse_loose(&before, locale)
            .map(|v| pad_left(&format_decimal(v.trunc(), 0, locale), PAD_QTY))
            .unwrap_or(before.clone()),
        Field::Unit => parse_loose(&before, locale)
            .map(|v| pad_left(&format_decimal(v, 2, locale), PAD_UNIT))
            .unwrap_or(before.clone()),
        Field::Date => normalise_date(&before),
        Field::Desc => before.trim().to_string(),
        Field::Category => before.clone(),
    };
    if after != before {
        state.set(after);
    }
}

/// A cell lost focus: normalise it and, if the value actually changed since
/// focus entered, record one form-level undo step.
fn blur_cell(ledger: Ledger, id: AccessibilityId, original: String) {
    commit_cell(ledger, id);
    let Some((index, field)) = find_cell(ledger, id) else {
        return;
    };
    let Some(state) = ledger.rows.peek()[index].text(field) else {
        return;
    };
    let after = state.peek().clone();
    if after != original {
        let mut ledger = ledger;
        ledger.history.write().push((index, field, original));
    }
}

fn normalise_date(text: &str) -> String {
    let digits: String = text.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() != 8 {
        return text.to_string();
    }
    format!("{}-{}-{}", &digits[0..4], &digits[4..6], &digits[6..8])
}

fn reformat_all(ledger: Ledger, locale: Locale) {
    // The stored text is always in the *previous* locale, so parse with the
    // loose parser (which recognises both conventions) and re-emit.
    let rows = ledger.rows.peek().clone();
    for row in rows {
        // NOTE: the scrutinee temporary of `if let Some(v) = parse(&x.peek())`
        // keeps the `Ref` alive inside the body, and writing to the same signal
        // there aborts the app. Copy the text out first.
        let mut qty = row.qty;
        let text = qty.peek().clone();
        if let Some(value) = parse_loose(&text, locale) {
            qty.set(pad_left(&format_decimal(value.trunc(), 0, locale), PAD_QTY));
        }
        let mut unit = row.unit;
        let text = unit.peek().clone();
        if let Some(value) = parse_loose(&text, locale) {
            unit.set(pad_left(&format_decimal(value, 2, locale), PAD_UNIT));
        }
    }
    let mut ledger = ledger;
    let vat = *ledger.vat.peek();
    ledger.vat_text.set(pad_left(&format_decimal(vat, 2, locale), 6));
}

fn undo_form(ledger: Ledger) {
    let mut ledger = ledger;
    let Some((index, field, text)) = ledger.history.write().pop() else {
        ledger.status.set("Nothing to undo".into());
        return;
    };
    if let Some(mut state) = ledger.rows.peek()[index].text(field) {
        state.set(text);
    }
    ledger
        .status
        .set(format!("Undid row {} · {}", index + 1, field.label()));
    println!("UNDO row {} {:?}", index + 1, field);
}

fn focused_row(ledger: Ledger) -> usize {
    let id = *Platform::get().focused_accessibility_id.peek();
    find_cell(ledger, id).map(|(row, _)| row).unwrap_or(0)
}

fn copy_row(ledger: Ledger) {
    copy_row_at(ledger, focused_row(ledger));
}

fn copy_row_at(ledger: Ledger, index: usize) {
    let row = ledger.rows.peek()[index];
    let line = format!(
        "{}\t{}\t{}\t{}\t{}\t{}",
        row.date.peek(),
        row.desc.peek(),
        CATEGORIES[*row.cat.peek()].label(),
        row.qty.peek().trim(),
        row.unit.peek().trim(),
        if *row.reimb.peek() { "yes" } else { "no" }
    );
    let mut ledger = ledger;
    match Clipboard::set(line.clone()) {
        Ok(()) => {
            ledger.status.set(format!("Copied row {}", index + 1));
            println!("COPY {line}");
        }
        Err(error) => ledger.status.set(format!("Copy failed: {error:?}")),
    }
}

fn paste_row(ledger: Ledger) {
    paste_row_at(ledger, focused_row(ledger));
}

fn paste_row_at(ledger: Ledger, index: usize) {
    let mut ledger = ledger;
    let Ok(text) = Clipboard::get() else {
        ledger.status.set("Clipboard is empty".into());
        return;
    };
    let parts: Vec<&str> = text.trim_end_matches('\n').split('\t').collect();
    if parts.len() < 5 {
        ledger.status.set("Clipboard is not a TSV row".into());
        return;
    }
    let row = ledger.rows.peek()[index];
    let mut date = row.date;
    let mut desc = row.desc;
    let mut cat = row.cat;
    let mut qty = row.qty;
    let mut unit = row.unit;
    let mut reimb = row.reimb;
    date.set(parts[0].to_string());
    desc.set(parts[1].to_string());
    if let Some(found) = CATEGORIES.iter().position(|c| c.label() == parts[2]) {
        cat.set(found);
    }
    let locale = *ledger.locale.peek();
    qty.set(match parse_loose(parts[3], locale) {
        Some(v) => pad_left(&format_decimal(v.trunc(), 0, locale), PAD_QTY),
        None => parts[3].to_string(),
    });
    unit.set(match parse_loose(parts[4], locale) {
        Some(v) => pad_left(&format_decimal(v, 2, locale), PAD_UNIT),
        None => parts[4].to_string(),
    });
    if let Some(flag) = parts.get(5) {
        reimb.set(*flag == "yes");
    }
    ledger.status.set(format!("Pasted into row {}", index + 1));
    println!("PASTE row {}", index + 1);
}

fn global_keys(ledger: Ledger, e: Event<KeyboardEventData>) {
    let meta = e.modifiers.contains(Modifiers::META) || e.modifiers.contains(Modifiers::CONTROL);
    if !meta {
        return;
    }
    let Key::Character(c) = &e.key else { return };
    let shift = e.modifiers.contains(Modifiers::SHIFT);
    match c.to_ascii_lowercase().as_str() {
        "c" if shift => copy_row(ledger),
        "v" if shift => paste_row(ledger),
        "z" if shift => undo_form(ledger),
        _ => {}
    }
}

// ---------------------------------------------------------------- self-test

async fn selftest(ledger: Ledger) {
    let mut ledger = ledger;
    let passed = Cell::new(0usize);
    let failed = Cell::new(0usize);
    let check = |name: &str, ok: bool| {
        if ok {
            passed.set(passed.get() + 1);
            println!("SELFTEST PASS {name}");
        } else {
            failed.set(failed.get() + 1);
            println!("SELFTEST FAIL {name}");
        }
    };
    let settle = || Timer::after(Duration::from_millis(120));
    settle().await;

    let row0 = ledger.rows.peek()[0];

    // --- typing filter -----------------------------------------------------
    check("filter rejects letters", !is_partial_number("12a", Locale::EnUs));
    check("filter accepts a partial number", is_partial_number("-1,2", Locale::EnUs));
    check(
        "filter is locale-aware",
        is_partial_number("1 234,5", Locale::FrFr) && !is_partial_number("1,2.3", Locale::FrFr),
    );

    // --- type 1234.5 then blur (Tab) --------------------------------------
    let mut unit = row0.unit;
    unit.set("1234.5".to_string());
    settle().await;
    commit_cell(ledger, row0.id(Field::Unit));
    settle().await;
    println!("UNIT en-US = {}", unit.peek());
    check("1234.5 formats to 1,234.50", unit.peek().trim() == "1,234.50");

    // --- locale toggle -----------------------------------------------------
    ledger.locale.set(Locale::FrFr);
    reformat_all(ledger, Locale::FrFr);
    settle().await;
    println!("UNIT fr-FR = {}", unit.peek());
    check("locale toggle re-formats to 1 234,50", unit.peek().trim() == "1 234,50");
    check(
        "fr-FR parses its own output",
        parse_loose("1 234,50", Locale::FrFr) == Decimal::from_str("1234.50").ok(),
    );
    ledger.locale.set(Locale::EnUs);
    reformat_all(ledger, Locale::EnUs);
    settle().await;
    check("back to en-US", unit.peek().trim() == "1,234.50");

    // --- paste shapes ------------------------------------------------------
    check(
        "paste $1,234.56",
        parse_loose("$1,234.56", Locale::EnUs) == Decimal::from_str("1234.56").ok(),
    );
    check(
        "paste 1.234,56 €",
        parse_loose("1.234,56 €", Locale::EnUs) == Decimal::from_str("1234.56").ok(),
    );
    check(
        "paste (12.50) is negative",
        parse_loose("(12.50)", Locale::EnUs) == Decimal::from_str("-12.50").ok(),
    );

    // --- arrow stepping ----------------------------------------------------
    unit.set(pad_left("10.00", PAD_UNIT));
    settle().await;
    let step = |shift: bool, up: bool| {
        let base = Decimal::from_str("0.01").unwrap();
        let step = if shift { base * Decimal::from(10) } else { base };
        let current = parse_loose(&unit.peek(), Locale::EnUs).unwrap_or_default();
        let next = if up { current + step } else { current - step };
        format_decimal(next, 2, Locale::EnUs)
    };
    check("Up steps by 0.01", step(false, true) == "10.01");
    check("Shift+Up steps by 0.10", step(true, true) == "10.10");
    unit.set(pad_left("482.50", PAD_UNIT));
    settle().await;

    // --- validation + Save -------------------------------------------------
    let mut desc = row0.desc;
    desc.set(String::new());
    settle().await;
    check(
        "empty description is an error",
        cell_error(&row0, Field::Desc, Locale::EnUs).is_some(),
    );
    let mut qty = row0.qty;
    qty.set(pad_left("1000", PAD_QTY));
    settle().await;
    check(
        "qty 1000 is out of range",
        cell_error(&row0, Field::Qty, Locale::EnUs).is_some(),
    );
    desc.set("Flight LHR-JFK".to_string());
    qty.set(pad_left("1", PAD_QTY));
    settle().await;
    check(
        "row is valid again",
        EDITABLE
            .iter()
            .all(|f| cell_error(&row0, *f, Locale::EnUs).is_none()),
    );

    // --- live totals -------------------------------------------------------
    let locale = *ledger.locale.peek();
    let mut subtotal = Decimal::ZERO;
    for row in ledger.rows.peek().iter() {
        subtotal += amount(row, locale);
    }
    println!("SUBTOTAL {}", format_decimal(subtotal, 2, locale));
    check("subtotal is exact", subtotal == Decimal::from_str("3551.97").unwrap());
    let vat = (subtotal * *ledger.vat.peek() / Decimal::from(100)).round_dp(2);
    check("VAT 19% of the subtotal", vat == Decimal::from_str("674.87").unwrap());

    // --- TSV round-trip ----------------------------------------------------
    copy_row_at(ledger, 0);
    settle().await;
    let copied = Clipboard::get().unwrap_or_default();
    check("row copies as TSV", copied.matches('\t').count() == 5);
    let d5 = ledger.rows.peek()[5].desc;
    let before = d5.peek().clone();
    paste_row_at(ledger, 5);
    settle().await;
    check(
        "TSV pastes into another row",
        *d5.peek() == "Flight LHR-JFK" && before != "Flight LHR-JFK",
    );

    // --- blur commit + form-level undo -------------------------------------
    let row7 = ledger.rows.peek()[7];
    let mut d7 = row7.desc;
    let original = d7.peek().clone();
    row7.id(Field::Desc).request_focus();
    settle().await;
    d7.set("Zulu licence".to_string());
    settle().await;
    row7.id(Field::Qty).request_focus(); // blur
    settle().await;
    check("blur commits the edit", *d7.peek() == "Zulu licence");
    undo_form(ledger);
    settle().await;
    check("form-level undo restores the cell", *d7.peek() == original);

    // --- blur normalises a raw number --------------------------------------
    let mut u7 = row7.unit;
    row7.id(Field::Unit).request_focus();
    settle().await;
    u7.set("2500".to_string());
    settle().await;
    row7.id(Field::Date).request_focus(); // blur
    settle().await;
    println!("BLUR-NORMALISED {}", u7.peek());
    check("blur normalises 2500 to 2,500.00", u7.peek().trim() == "2,500.00");

    // --- Tab order ---------------------------------------------------------
    // Ask the accessibility tree to walk forward and log what it lands on.
    let platform = Platform::get();
    ledger.rows.peek()[0].id(Field::Date).request_focus();
    settle().await;
    for _ in 0..8 {
        let node = platform.focused_accessibility_node.peek().clone();
        println!(
            "TABORDER {:?} {:?}",
            node.role(),
            node.label().unwrap_or_default()
        );
        platform.send(UserEvent::FocusAccessibilityNode(
            AccessibilityFocusStrategy::Forward(AccessibilityFocusMovement::OutsideGroup),
        ));
        settle().await;
    }

    println!("SELFTEST DONE pass={} fail={}", passed.get(), failed.get());
    std::process::exit(if failed.get() == 0 { 0 } else { 1 });
}
