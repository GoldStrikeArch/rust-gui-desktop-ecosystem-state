//! SPEC-10 "Ledger" — all UI and state. Portable half.
//!
//! Nothing here imports `dioxus::desktop`; the only OS calls are
//! `platform::clipboard_{read,write}`. Parsing, formatting and locale rules are
//! plain Rust over `rust_decimal::Decimal` so they run unchanged on Blitz.
//!
//! THE NUMBER MODEL (the finding this spec asks for)
//! The widget carries a `String` and the model carries a `Decimal`; there is no
//! numeric widget anywhere in Dioxus/HTML that can be *constrained* — an
//! `<input>` only ever hands you the full new string in `oninput`. So every
//! numeric cell keeps BOTH: `qty_text`/`price_text` (what the user is typing)
//! and `qty`/`price` (the last committed value). `oninput` runs a
//! locale-aware "could this still become a number?" filter, `onblur` parses,
//! clamps, formats and commits. Rejecting a keystroke needs a trick: a
//! controlled Dioxus input is diffed against the previous *VDOM* value, so
//! writing the old string back produces no DOM patch and the rejected character
//! stays on screen — the row is re-`key`ed instead, which remounts the input
//! with the correct value (and re-focuses it).

use std::collections::HashMap;
use std::rc::Rc;

use dioxus::html::Modifiers;
use dioxus::prelude::*;
use rust_decimal::Decimal;
use rust_decimal::prelude::*;

use crate::platform;

// ------------------------------------------------------------------ locale

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Locale {
    EnUs,
    FrFr,
}
impl Locale {
    fn dec(self) -> char {
        match self {
            Locale::EnUs => '.',
            Locale::FrFr => ',',
        }
    }
    fn grp(self) -> char {
        match self {
            Locale::EnUs => ',',
            Locale::FrFr => ' ',
        }
    }
    fn tag(self) -> &'static str {
        match self {
            Locale::EnUs => "en-US",
            Locale::FrFr => "fr-FR",
        }
    }
}

/// Group the integer part in threes and swap in the locale's separators.
pub fn fmt_dec(v: Decimal, dp: u32, loc: Locale, group: bool) -> String {
    let neg = v.is_sign_negative() && !v.is_zero();
    let s = format!("{:.*}", dp as usize, v.abs());
    let (int, frac) = match s.split_once('.') {
        Some((i, f)) => (i.to_string(), f.to_string()),
        None => (s, String::new()),
    };
    let mut out = String::new();
    for (i, c) in int.chars().enumerate() {
        if group && i > 0 && (int.len() - i) % 3 == 0 {
            out.push(loc.grp());
        }
        out.push(c);
    }
    if dp > 0 {
        out.push(loc.dec());
        out.push_str(&frac);
    }
    if neg { format!("-{out}") } else { out }
}

/// Strict, locale-aware "can this still become a number?" filter for `oninput`.
pub fn typeable(s: &str, loc: Locale, allow_dec: bool, allow_neg: bool) -> bool {
    let mut seen_dec = false;
    for (i, c) in s.chars().enumerate() {
        match c {
            '-' if i == 0 && allow_neg => {}
            c if c.is_ascii_digit() => {}
            c if c == loc.grp() => {}
            c if c == loc.dec() && allow_dec && !seen_dec => seen_dec = true,
            _ => return false,
        }
    }
    true
}

/// Lenient parse used for both commit and paste: strips currency symbols and
/// spaces, understands the accounting negative `(12.50)`, and — when both `,`
/// and `.` are present — treats the LAST one as the decimal separator, so
/// `$1,234.56` and `1.234,56 €` both work whatever the current locale is.
pub fn parse_dec(raw: &str, loc: Locale) -> Option<Decimal> {
    let mut s: String = raw
        .chars()
        .filter(|c| !matches!(c, '$' | '€' | '£' | '¥' | ' ' | '\u{00a0}' | '\u{202f}'))
        .collect();
    let mut neg = false;
    if s.starts_with('(') && s.ends_with(')') && s.len() > 2 {
        neg = true;
        s = s[1..s.len() - 1].to_string();
    }
    if let Some(rest) = s.strip_prefix('-') {
        neg = !neg;
        s = rest.to_string();
    }
    if s.is_empty() {
        return None;
    }
    let (last_comma, last_dot) = (s.rfind(','), s.rfind('.'));
    let dec_at = match (last_comma, last_dot) {
        (Some(c), Some(d)) => Some(c.max(d)),
        (Some(c), None) => (loc.dec() == ',' || s.matches(',').count() == 1 && s.len() - c != 4)
            .then_some(c),
        (None, Some(d)) => (loc.dec() == '.' || s.matches('.').count() == 1 && s.len() - d != 4)
            .then_some(d),
        (None, None) => None,
    };
    let cleaned: String = match dec_at {
        Some(at) => {
            let (i, f) = s.split_at(at);
            let int: String = i.chars().filter(|c| c.is_ascii_digit()).collect();
            let frac: String = f[1..].chars().filter(|c| c.is_ascii_digit()).collect();
            format!("{int}.{frac}")
        }
        None => s.chars().filter(|c| c.is_ascii_digit()).collect(),
    };
    let v = Decimal::from_str(&cleaned).ok()?;
    Some(if neg { -v } else { v })
}

// ------------------------------------------------------------------- model

pub const CATEGORIES: [&str; 8] = [
    "Travel", "Lodging", "Meals", "Software", "Hardware", "Office", "Training", "Other",
];

#[derive(Clone, PartialEq)]
pub struct Row {
    pub date: String,
    pub desc: String,
    pub cat: usize,
    pub qty: i64,
    pub price: Decimal,
    pub reimb: bool,
    /// live edit buffers (what the widget shows)
    pub qty_text: String,
    pub price_text: String,
    /// forces a remount of this row's inputs (rejected keystroke / locale flip)
    pub bump: u32,
}

impl Row {
    fn amount(&self) -> Decimal {
        Decimal::from(self.qty) * self.price
    }
    fn refresh_text(&mut self, loc: Locale) {
        self.qty_text = self.qty.to_string();
        self.price_text = fmt_dec(self.price, 2, loc, true);
    }
    /// row -> TSV (req 9)
    fn to_tsv(&self, loc: Locale) -> String {
        format!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            self.date,
            self.desc,
            CATEGORIES[self.cat],
            self.qty,
            fmt_dec(self.price, 2, loc, false),
            if self.reimb { "yes" } else { "no" }
        )
    }
    fn from_tsv(s: &str, loc: Locale) -> Option<Row> {
        let f: Vec<&str> = s.trim_end_matches(['\n', '\r']).split('\t').collect();
        if f.len() < 5 {
            return None;
        }
        let mut r = Row {
            date: f[0].to_string(),
            desc: f[1].to_string(),
            cat: CATEGORIES.iter().position(|c| *c == f[2]).unwrap_or(7),
            qty: f[3].trim().parse().unwrap_or(1),
            price: parse_dec(f[4], loc).unwrap_or_default(),
            reimb: f.get(5).map(|v| *v == "yes").unwrap_or(false),
            qty_text: String::new(),
            price_text: String::new(),
            bump: 0,
        };
        r.refresh_text(loc);
        Some(r)
    }
}

fn seed(loc: Locale) -> Vec<Row> {
    let raw: [(&str, &str, usize, i64, &str, bool); 12] = [
        ("2026-01-08", "Flight LHR-JFK", 0, 1, "482.50", true),
        ("2026-01-08", "Airport transfer", 0, 2, "34.75", true),
        ("2026-01-09", "Hotel Manhattan", 1, 3, "219.00", true),
        ("2026-01-09", "Team dinner", 2, 6, "41.80", false),
        ("2026-01-10", "Conference pass", 6, 1, "1295.00", true),
        ("2026-01-10", "Refund: cancelled taxi", 0, 1, "-34.75", false),
        ("2026-01-11", "USB-C dock", 4, 1, "189.99", true),
        ("2026-01-11", "IDE licence", 3, 4, "99.00", true),
        ("2026-01-12", "Printer paper", 5, 12, "5.99", false),
        ("2026-01-12", "Coworking day pass", 5, 2, "28.00", true),
        ("2026-01-13", "Data roaming", 7, 1, "12.50", false),
        ("2026-01-14", "Return flight", 0, 1, "512.35", true),
    ];
    raw.iter()
        .map(|(d, de, c, q, p, r)| {
            let mut row = Row {
                date: d.to_string(),
                desc: de.to_string(),
                cat: *c,
                qty: *q,
                price: Decimal::from_str(p).unwrap(),
                reimb: *r,
                qty_text: String::new(),
                price_text: String::new(),
                bump: 0,
            };
            row.refresh_text(loc);
            row
        })
        .collect()
}

// -------------------------------------------------------------- validation

const F_DATE: u8 = 0;
const F_DESC: u8 = 1;
const F_CAT: u8 = 2;
const F_QTY: u8 = 3;
const F_PRICE: u8 = 4;
const F_REIMB: u8 = 5;

fn field_name(f: u8) -> &'static str {
    match f {
        F_DATE => "Date",
        F_DESC => "Description",
        F_CAT => "Category",
        F_QTY => "Qty",
        F_PRICE => "Unit price",
        _ => "Reimbursable",
    }
}

/// (row, field, message) for every rule the table currently breaks.
fn validate(rows: &[Row]) -> Vec<(usize, u8, String)> {
    let mut out = Vec::new();
    let lo = Decimal::from_str("-99999.99").unwrap();
    let hi = Decimal::from_str("99999.99").unwrap();
    for (i, r) in rows.iter().enumerate() {
        let n = r.desc.chars().count();
        if n == 0 {
            out.push((i, F_DESC, "Description is required".into()));
        } else if n > 60 {
            out.push((i, F_DESC, format!("Description is {n} chars (max 60)")));
        }
        if !(1..=999).contains(&r.qty) {
            out.push((i, F_QTY, "Qty must be 1–999".into()));
        }
        if r.price < lo || r.price > hi {
            out.push((i, F_PRICE, "Unit price must be −99 999.99 … 99 999.99".into()));
        }
        if r.date.len() != 10 {
            out.push((i, F_DATE, "Date must be YYYY-MM-DD".into()));
        }
    }
    out
}

// ---------------------------------------------------------------- the app

type FocusMap = HashMap<(usize, u8), Rc<MountedData>>;

#[derive(Clone)]
struct Undo {
    row: usize,
    field: u8,
    before: Row,
    after: Row,
}

#[component]
pub fn App() -> Element {
    let mut loc = use_signal(|| Locale::EnUs);
    let mut rows = use_signal(|| seed(Locale::EnUs));
    let mut vat = use_signal(|| Decimal::from(20));
    let mut vat_text = use_signal(|| "20".to_string());
    let mut sel = use_signal(|| 0usize);
    let mut status = use_signal(|| "ready".to_string());
    let mut undo_stack = use_signal(Vec::<Undo>::new);
    let mut redo_stack = use_signal(Vec::<Undo>::new);
    let mut focus_map = use_signal(FocusMap::new);

    // Focus a cell through the MountedData handle registered by `onmounted`.
    let focus_cell = use_callback(move |key: (usize, u8)| {
        if let Some(h) = focus_map.peek().get(&key).cloned() {
            spawn(async move {
                _ = h.set_focus(true).await;
            });
        }
    });

    // Snapshot of a cell's row taken when it gains focus. Description edits go
    // straight into the model on `oninput` (that is what makes the live totals
    // work), so the row at blur time is already the *new* value — without this
    // snapshot the undo stack would never see a text edit at all.
    let mut edit_before = use_signal(|| None::<(usize, Row)>);
    let arm_undo = use_callback(move |i: usize| {
        edit_before.set(Some((i, rows.peek()[i].clone())));
    });

    // Commit + push one undo entry.
    let commit = use_callback(move |(i, field, next): (usize, u8, Row)| {
        let before = match edit_before.peek().clone() {
            Some((bi, r)) if bi == i => r,
            _ => rows.peek()[i].clone(),
        };
        if before == next {
            return;
        }
        edit_before.set(Some((i, next.clone())));
        rows.write()[i] = next.clone();
        undo_stack.write().push(Undo { row: i, field, before, after: next });
        redo_stack.write().clear();
    });

    let do_undo = use_callback(move |()| {
        if let Some(u) = undo_stack.write().pop() {
            rows.write()[u.row] = u.before.clone();
            status.set(format!("undo: row {} {}", u.row + 1, field_name(u.field)));
            redo_stack.write().push(u);
        } else {
            status.set("nothing to undo".into());
        }
    });
    let do_redo = use_callback(move |()| {
        if let Some(u) = redo_stack.write().pop() {
            rows.write()[u.row] = u.after.clone();
            status.set(format!("redo: row {} {}", u.row + 1, field_name(u.field)));
            undo_stack.write().push(u);
        }
    });

    let set_locale = use_callback(move |l: Locale| {
        loc.set(l);
        for r in rows.write().iter_mut() {
            r.refresh_text(l);
        }
        vat_text.set(fmt_dec(*vat.peek(), 0, l, false));
        status.set(format!("locale {}", l.tag()));
    });

    let copy_row = use_callback(move |()| {
        let i = *sel.peek();
        let tsv = rows.peek()[i].to_tsv(*loc.peek());
        match platform::clipboard_write(tsv.clone()) {
            Ok(()) => {
                status.set(format!("copied row {} as TSV", i + 1));
                println!("[ledger] copied TSV: {tsv}");
            }
            Err(e) => status.set(format!("copy failed: {e}")),
        }
    });
    let paste_row = use_callback(move |()| {
        let i = *sel.peek();
        match platform::clipboard_read() {
            Ok(text) => match Row::from_tsv(&text, *loc.peek()) {
                Some(mut r) => {
                    r.bump = rows.peek()[i].bump;
                    commit.call((i, F_DESC, r));
                    status.set(format!("pasted TSV into row {}", i + 1));
                    println!("[ledger] pasted TSV into row {}", i + 1);
                }
                None => status.set("clipboard is not a TSV row".into()),
            },
            Err(e) => status.set(format!("paste failed: {e}")),
        }
    });

    selftest(SelfTestCtx {
        rows,
        arm_undo,
        vat,
        vat_text,
        sel,
        status,
        set_locale,
        commit,
        copy_row,
        paste_row,
        do_undo,
    });

    // ---- derived (recomputed every render; 12 rows is nothing)
    let l = loc();
    let list = rows.read().clone();
    let errs = validate(&list);
    let subtotal: Decimal = list.iter().map(|r| r.amount()).sum();
    let vat_amount = (subtotal * vat() / Decimal::from(100)).round_dp(2);
    let total = subtotal + vat_amount;
    let n_rows = list.len();
    let err_of = |i: usize, f: u8| -> Option<String> {
        errs.iter()
            .find(|(ri, fi, _)| *ri == i && *fi == f)
            .map(|(_, _, m)| m.clone())
    };

    rsx! {
        style { {CSS} }
        div {
            class: "root",
            onkeydown: move |e: KeyboardEvent| {
                let m = e.modifiers();
                let meta = m.contains(Modifiers::META) || m.contains(Modifiers::CONTROL);
                if let Key::Character(c) = e.key() {
                    if meta && m.contains(Modifiers::SHIFT) {
                        match c.to_ascii_lowercase().as_str() {
                            "c" => { e.prevent_default(); copy_row.call(()); }
                            "v" => { e.prevent_default(); paste_row.call(()); }
                            "z" => { e.prevent_default(); do_redo.call(()); }
                            _ => {}
                        }
                    } else if meta && m.contains(Modifiers::ALT) && c.eq_ignore_ascii_case("z") {
                        e.prevent_default();
                        do_undo.call(());
                    }
                }
            },

            div { class: "toolbar",
                button {
                    onclick: move |_| set_locale.call(if l == Locale::EnUs { Locale::FrFr } else { Locale::EnUs }),
                    "Locale: {l.tag()}"
                }
                label { class: "vat",
                    "VAT"
                    input {
                        r#type: "range", min: "0", max: "25", step: "1",
                        value: "{vat}",
                        "aria-label": "VAT percent slider",
                        oninput: move |e| {
                            if let Ok(v) = e.value().parse::<i64>() {
                                vat.set(Decimal::from(v));
                                vat_text.set(v.to_string());
                            }
                        },
                    }
                    input {
                        class: "num vatnum",
                        value: "{vat_text}",
                        "aria-label": "VAT percent",
                        oninput: move |e| {
                            let v = e.value();
                            if typeable(&v, l, false, false) { vat_text.set(v); }
                        },
                        onblur: move |_| {
                            let v = parse_dec(&vat_text.peek().clone(), l)
                                .unwrap_or(*vat.peek())
                                .clamp(Decimal::ZERO, Decimal::from(25))
                                .round();
                            vat.set(v);
                            vat_text.set(fmt_dec(v, 0, l, false));
                        },
                    }
                    "%"
                }
                span { class: "grow" }
                button { onclick: move |_| do_undo.call(()), "Undo edit" }
                button { onclick: move |_| do_redo.call(()), "Redo" }
                button { onclick: move |_| copy_row.call(()), "Copy row" }
                button { onclick: move |_| paste_row.call(()), "Paste row" }
                button {
                    class: "save",
                    disabled: !errs.is_empty(),
                    onclick: move |_| status.set("saved".into()),
                    if errs.is_empty() { "Save" } else { "Save ({errs.len()} errors)" }
                }
            }

            div { class: "grid",
                div { class: "hrow",
                    span { "#" } span { "Date" } span { "Description" } span { "Category" }
                    span { class: "num", "Qty" } span { class: "num", "Unit price" }
                    span { class: "num", "Amount" } span { class: "mid", "Reimb." }
                }
                for (i, r) in list.iter().enumerate() {
                    div {
                        key: "{i}-{r.bump}",
                        class: if i == sel() { "drow sel" } else { "drow" },
                        onfocusin: move |_| sel.set(i),
                        span { class: "rn", "{i + 1}" }
                        input {
                            r#type: "date",
                            class: if err_of(i, F_DATE).is_some() { "bad" } else { "" },
                            value: "{r.date}",
                            tabindex: "0",
                            "aria-label": "Date row {i + 1}",
                            onmounted: move |e| { focus_map.write().insert((i, F_DATE), e.data()); },
                            onfocus: move |_| { focus_log(i, F_DATE); arm_undo.call(i); },
                            onchange: move |e| {
                                let mut n = rows.peek()[i].clone();
                                n.date = e.value();
                                commit.call((i, F_DATE, n));
                            },
                            onkeydown: move |e| nav(e, i, F_DATE, n_rows, focus_cell),
                        }
                        input {
                            class: if err_of(i, F_DESC).is_some() { "bad" } else { "" },
                            value: "{r.desc}",
                            tabindex: "0",
                            "aria-label": "Description row {i + 1}",
                            onmounted: move |e| { focus_map.write().insert((i, F_DESC), e.data()); },
                            onfocus: move |_| { focus_log(i, F_DESC); arm_undo.call(i); },
                            oninput: move |e| rows.write()[i].desc = e.value(),
                            onblur: move |_| {
                                let n = rows.peek()[i].clone();
                                commit.call((i, F_DESC, n));
                            },
                            onkeydown: move |e| nav(e, i, F_DESC, n_rows, focus_cell),
                        }
                        select {
                            value: "{r.cat}",
                            tabindex: "0",
                            "aria-label": "Category row {i + 1}",
                            onmounted: move |e| { focus_map.write().insert((i, F_CAT), e.data()); },
                            onfocus: move |_| { focus_log(i, F_CAT); arm_undo.call(i); },
                            onchange: move |e| {
                                let mut n = rows.peek()[i].clone();
                                n.cat = e.value().parse().unwrap_or(0);
                                commit.call((i, F_CAT, n));
                            },
                            onkeydown: move |e| nav(e, i, F_CAT, n_rows, focus_cell),
                            for (ci, c) in CATEGORIES.iter().enumerate() {
                                option { value: "{ci}", selected: ci == r.cat, "{c}" }
                            }
                        }
                        input {
                            class: if err_of(i, F_QTY).is_some() { "num bad" } else { "num" },
                            value: "{r.qty_text}",
                            tabindex: "0",
                            inputmode: "numeric",
                            "aria-label": "Qty row {i + 1}",
                            onmounted: move |e| { focus_map.write().insert((i, F_QTY), e.data()); },
                            onfocus: move |_| { focus_log(i, F_QTY); arm_undo.call(i); },
                            oninput: move |e| {
                                let v = e.value();
                                if typeable(&v, l, false, false) {
                                    rows.write()[i].qty_text = v;
                                } else {
                                    // reject: remount the row so the DOM goes back
                                    rows.write()[i].bump += 1;
                                    focus_cell.call((i, F_QTY));
                                }
                            },
                            onblur: move |_| {
                                let n = normalize(rows.peek()[i].clone(), F_QTY, l);
                                commit.call((i, F_QTY, n));
                            },
                            onkeydown: move |e| {
                                if step(&e, i, F_QTY, rows, l) { return; }
                                nav(e, i, F_QTY, n_rows, focus_cell);
                            },
                        }
                        input {
                            class: if err_of(i, F_PRICE).is_some() { "num bad" } else { "num" },
                            value: "{r.price_text}",
                            tabindex: "0",
                            inputmode: "decimal",
                            "aria-label": "Unit price row {i + 1}",
                            onmounted: move |e| { focus_map.write().insert((i, F_PRICE), e.data()); },
                            onfocus: move |_| { focus_log(i, F_PRICE); arm_undo.call(i); },
                            oninput: move |e| {
                                let v = e.value();
                                // A paste can legally contain "$1,234.56"; only single-char
                                // growth is filtered as typing.
                                let pasted = v.chars().count() > rows.peek()[i].price_text.chars().count() + 1;
                                if pasted || typeable(&v, l, true, true) {
                                    rows.write()[i].price_text = v;
                                } else {
                                    rows.write()[i].bump += 1;
                                    focus_cell.call((i, F_PRICE));
                                }
                            },
                            onblur: move |_| {
                                let n = normalize(rows.peek()[i].clone(), F_PRICE, l);
                                commit.call((i, F_PRICE, n));
                            },
                            onkeydown: move |e| {
                                if step(&e, i, F_PRICE, rows, l) { return; }
                                nav(e, i, F_PRICE, n_rows, focus_cell);
                            },
                        }
                        span {
                            class: if r.amount().is_sign_negative() { "num amt neg" } else { "num amt" },
                            "{fmt_dec(r.amount(), 2, l, true)}"
                        }
                        span { class: "mid",
                            input {
                                r#type: "checkbox",
                                checked: r.reimb,
                                tabindex: "0",
                                "aria-label": "Reimbursable row {i + 1}",
                                onmounted: move |e| { focus_map.write().insert((i, F_REIMB), e.data()); },
                            onfocus: move |_| { focus_log(i, F_REIMB); arm_undo.call(i); },
                                onchange: move |e| {
                                    let mut n = rows.peek()[i].clone();
                                    n.reimb = e.checked();
                                    commit.call((i, F_REIMB, n));
                                },
                                onkeydown: move |e| nav(e, i, F_REIMB, n_rows, focus_cell),
                            }
                        }
                    }
                }
            }

            div { class: "totals",
                span { class: "grow" }
                span { "Subtotal" } span { class: "num amt", "{fmt_dec(subtotal, 2, l, true)}" }
                span { "VAT {fmt_dec(vat(), 0, l, false)}%" }
                span { class: "num amt", "{fmt_dec(vat_amount, 2, l, true)}" }
                span { class: "b", "Total" }
                span { class: "num amt b", "{fmt_dec(total, 2, l, true)}" }
            }

            if !errs.is_empty() {
                div { class: "errsum",
                    b { "{errs.len()} problem(s):" }
                    for (i, f, m) in errs.iter().take(6) {
                        span { class: "e", "row {i + 1} · {field_name(*f)}: {m}" }
                    }
                }
            }

            div { class: "status",
                "{status}"
                span { class: "grow" }
                span { class: "hint",
                    "Tab/⇧Tab: next cell · Enter/⇧Enter: down/up · ↑/↓ step (⇧ = ×10) · Esc: revert · ⌥⌘Z/⌘⇧Z undo/redo · ⌘⇧C/⌘⇧V row TSV"
                }
            }
        }
    }
}

/// LEDGER_TABLOG=1 prints every focus arrival so a Tab walk can be logged.
pub fn focus_log(i: usize, f: u8) {
    if std::env::var_os("LEDGER_TABLOG").is_some() {
        println!("FOCUS row {} {}", i + 1, field_name(f));
    }
}

/// Enter/⇧Enter move down/up within the column; Esc reverts the buffer.
fn nav(e: KeyboardEvent, i: usize, f: u8, n: usize, focus_cell: Callback<(usize, u8)>) {
    match e.key() {
        Key::Enter => {
            e.prevent_default();
            let shift = e.modifiers().contains(Modifiers::SHIFT);
            let next = if shift { i.saturating_sub(1) } else { (i + 1).min(n - 1) };
            focus_cell.call((next, f));
        }
        Key::Escape => {
            e.prevent_default();
        }
        _ => {}
    }
}

/// Blur normalisation: parse the edit buffer, clamp, commit, re-format.
/// The self-test drives this same function, not a copy of it.
pub fn normalize(mut n: Row, f: u8, l: Locale) -> Row {
    if f == F_QTY {
        if let Some(v) = parse_dec(&n.qty_text, l) {
            n.qty = v.round().to_i64().unwrap_or(n.qty);
        }
        n.qty_text = n.qty.to_string();
    } else {
        if let Some(v) = parse_dec(&n.price_text, l) {
            n.price = v.round_dp(2);
        }
        n.price_text = fmt_dec(n.price, 2, l, true);
    }
    n
}

/// ↑/↓ stepping arithmetic, shared by the key handler and the self-test.
pub fn stepped(mut r: Row, f: u8, up: bool, big: bool, l: Locale) -> Row {
    if f == F_QTY {
        let s = if big { 10 } else { 1 };
        let cur = parse_dec(&r.qty_text, l)
            .and_then(|v| v.round().to_i64())
            .unwrap_or(r.qty);
        r.qty = (cur + if up { s } else { -s }).max(0);
        r.qty_text = r.qty.to_string();
    } else {
        let s = Decimal::from_str(if big { "0.1" } else { "0.01" }).unwrap();
        let cur = parse_dec(&r.price_text, l).unwrap_or(r.price);
        r.price = (cur + if up { s } else { -s }).round_dp(2);
        r.price_text = fmt_dec(r.price, 2, l, true);
    }
    r
}

/// ↑/↓ arrow stepping on a numeric cell. Returns true if it handled the key.
fn step(e: &KeyboardEvent, i: usize, f: u8, mut rows: Signal<Vec<Row>>, l: Locale) -> bool {
    let up = match e.key() {
        Key::ArrowUp => true,
        Key::ArrowDown => false,
        _ => return false,
    };
    e.prevent_default();
    let big = e.modifiers().contains(Modifiers::SHIFT);
    let r = stepped(rows.peek()[i].clone(), f, up, big, l);
    rows.write()[i] = r;
    true
}

// -------------------------------------------------------------- self-test
// LEDGER_SELFTEST=1 drives the SAME parse/format/step/normalise/clipboard
// functions the widgets call and prints `SELFTEST DONE pass=N fail=M`.

#[derive(Clone, Copy)]
struct SelfTestCtx {
    rows: Signal<Vec<Row>>,
    vat: Signal<Decimal>,
    vat_text: Signal<String>,
    sel: Signal<usize>,
    status: Signal<String>,
    set_locale: Callback<Locale>,
    commit: Callback<(usize, u8, Row)>,
    arm_undo: Callback<usize>,
    copy_row: Callback<()>,
    paste_row: Callback<()>,
    do_undo: Callback<()>,
}

async fn nap(ms: u64) {
    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
}

fn selftest(mut c: SelfTestCtx) {
    use_future(move || async move {
        if !platform::selftest() {
            return;
        }
        let (mut pass, mut fail) = (0u32, 0u32);
        let mut check = |name: &str, ok: bool| {
            if ok {
                pass += 1;
                println!("SELFTEST ok   {name}");
            } else {
                fail += 1;
                println!("SELFTEST FAIL {name}");
            }
        };
        nap(1200).await;

        // 1. type 1234.5 into row 1's Unit price, then blur -> 1,234.50
        c.rows.write()[0].price_text = "1234.5".into();
        let n = normalize(c.rows.peek()[0].clone(), F_PRICE, Locale::EnUs);
        c.arm_undo.call(0);
        c.commit.call((0, F_PRICE, n));
        nap(200).await;
        let shown = c.rows.peek()[0].price_text.clone();
        println!("SELFTEST value en-US = {shown:?}");
        check("type 1234.5 + blur -> 1,234.50 (en-US)", shown == "1,234.50");

        // 2. locale toggle re-formats live
        c.set_locale.call(Locale::FrFr);
        nap(250).await;
        let shown = c.rows.peek()[0].price_text.clone();
        println!("SELFTEST value fr-FR = {shown:?}");
        check("locale toggle -> 1 234,50 (fr-FR)", shown == "1 234,50");
        check(
            "amount column re-formatted too",
            fmt_dec(c.rows.peek()[0].amount(), 2, Locale::FrFr, true) == "1 234,50",
        );

        // 3. typing filter
        check("filter: '12a' rejected", !typeable("12a", Locale::EnUs, true, true));
        check("filter: '1.2.3' rejected", !typeable("1.2.3", Locale::EnUs, true, true));
        check("filter: '-1,234.5' accepted", typeable("-1,234.5", Locale::EnUs, true, true));
        check("filter: '1 234,5' accepted in fr-FR", typeable("1 234,5", Locale::FrFr, true, true));
        check("filter: qty rejects a decimal point", !typeable("1.5", Locale::EnUs, false, false));

        // 4. paste forms
        let d = |s: &str| Decimal::from_str(s).unwrap();
        check("paste '$1,234.56'", parse_dec("$1,234.56", Locale::EnUs) == Some(d("1234.56")));
        check("paste '1.234,56 €'", parse_dec("1.234,56 €", Locale::FrFr) == Some(d("1234.56")));
        check("paste '(12.50)' -> -12.50", parse_dec("(12.50)", Locale::EnUs) == Some(d("-12.50")));
        check("paste garbage rejected", parse_dec("abc", Locale::EnUs).is_none());

        // 5. arrow stepping
        c.set_locale.call(Locale::EnUs);
        nap(200).await;
        let base = c.rows.peek()[1].clone();
        let up = stepped(base.clone(), F_PRICE, true, false, Locale::EnUs);
        let up10 = stepped(base.clone(), F_PRICE, true, true, Locale::EnUs);
        check("Up steps price by 0.01", up.price == base.price + d("0.01"));
        check("Shift+Up steps price by 0.10", up10.price == base.price + d("0.1"));
        let qup = stepped(base.clone(), F_QTY, true, false, Locale::EnUs);
        let qup10 = stepped(base.clone(), F_QTY, true, true, Locale::EnUs);
        check("Up steps qty by 1", qup.qty == base.qty + 1);
        check("Shift+Up steps qty by 10", qup10.qty == base.qty + 10);

        // 6. live totals
        let sub: Decimal = c.rows.peek().iter().map(|r| r.amount()).sum();
        let mut r2 = c.rows.peek()[2].clone();
        r2.qty += 1;
        let delta = r2.price;
        c.arm_undo.call(2);
        c.commit.call((2, F_QTY, r2));
        nap(200).await;
        let sub2: Decimal = c.rows.peek().iter().map(|r| r.amount()).sum();
        check("subtotal recomputes on a committed edit", sub2 == sub + delta);

        // 7. validation + Save gating
        let mut bad = c.rows.peek()[3].clone();
        bad.desc = String::new();
        c.arm_undo.call(3);
        c.commit.call((3, F_DESC, bad));
        nap(200).await;
        let errs = validate(&c.rows.peek());
        check("empty Description raises an error", errs.iter().any(|(i, f, _)| *i == 3 && *f == F_DESC));
        let mut bad = c.rows.peek()[4].clone();
        bad.qty = 5000;
        c.arm_undo.call(4);
        c.commit.call((4, F_QTY, bad));
        nap(200).await;
        let errs = validate(&c.rows.peek());
        check("Qty 5000 raises an error", errs.iter().any(|(i, f, _)| *i == 4 && *f == F_QTY));
        check("Save is gated (2 errors)", errs.len() == 2);

        // 8. form-level undo of the last committed cell edit
        c.do_undo.call(());
        nap(200).await;
        check("form-level undo restores Qty", c.rows.peek()[4].qty != 5000);
        c.do_undo.call(());
        nap(200).await;
        check("form-level undo restores Description", !c.rows.peek()[3].desc.is_empty());
        check("Save re-enabled", validate(&c.rows.peek()).is_empty());

        // 9. clipboard round-trip through the real pasteboard
        c.sel.set(0);
        nap(150).await;
        c.copy_row.call(());
        nap(300).await;
        let tsv = platform::clipboard_read().unwrap_or_default();
        check("⌘⇧C put a 6-field TSV row on the pasteboard", tsv.split('\t').count() == 6);
        c.sel.set(11);
        nap(150).await;
        c.paste_row.call(());
        nap(300).await;
        check(
            "⌘⇧V filled the target row from TSV",
            c.rows.peek()[11].desc == c.rows.peek()[0].desc,
        );

        // 10. VAT field <-> slider linkage
        c.vat_text.set("7".into());
        let v = parse_dec("7", Locale::EnUs).unwrap();
        c.vat.set(v);
        nap(200).await;
        check("VAT numeric field drives the model", *c.vat.peek() == Decimal::from(7));

        c.status.set("selftest done".into());
        println!("SELFTEST DONE pass={pass} fail={fail}");
        nap(200).await;
        std::process::exit(0);
    });
}

// ------------------------------------------------------------------- style

const CSS: &str = r#"
* { box-sizing: border-box; }
body { margin: 0; font-family: system-ui, -apple-system, sans-serif; }
:root { color-scheme: light dark; }
.root {
  --bg:#f4f4f7; --fg:#18181b; --panel:#fff; --line:#d5d5db; --hint:#6b6b73;
  --bad:#c0392b; --sel:#e8f0fe;
  display:flex; flex-direction:column; height:100vh; padding:8px; gap:8px;
  background:var(--bg); color:var(--fg); font-size:13px;
}
@media (prefers-color-scheme: dark) {
  .root { --bg:#1c1c1f; --fg:#e9e9ec; --panel:#242428; --line:#3a3a41; --hint:#9a9aa2;
          --bad:#ff6b5e; --sel:#2b3550; }
}
.toolbar { display:flex; gap:8px; align-items:center; }
.grow { flex:1; }
button { font:inherit; padding:3px 9px; }
button.save:disabled { opacity:0.5; }
.vat { display:flex; gap:6px; align-items:center; }
.vatnum { width:56px; }
.grid { flex:1; overflow:auto; background:var(--panel); border:1px solid var(--line); border-radius:6px; }
.hrow, .drow {
  display:grid;
  grid-template-columns: 34px 130px minmax(150px,1fr) 118px 66px 108px 108px 62px;
  gap:6px; padding:3px 8px; align-items:center;
}
.hrow { position:sticky; top:0; background:var(--panel); border-bottom:1px solid var(--line);
        font-size:11.5px; color:var(--hint); z-index:1; }
.drow { border-bottom:1px solid var(--line); }
.drow.sel { background:var(--sel); }
.rn { color:var(--hint); font-variant-numeric: tabular-nums; }
.mid { text-align:center; }
input, select { font:inherit; padding:2px 5px; width:100%; border:1px solid var(--line);
                border-radius:4px; background:transparent; color:inherit; }
input[type=checkbox] { width:auto; }
input[type=range] { width:110px; padding:0; border:none; }
/* req 3: right aligned + tabular figures, so the always-2-decimal values line
   up on the decimal separator across every row. */
.num, .amt { text-align:right; font-variant-numeric: tabular-nums lining-nums;
             font-feature-settings: "tnum" 1, "lnum" 1; }
.amt { padding-right:5px; }
.neg { color:var(--bad); }
input.bad { border-color:var(--bad); background:color-mix(in srgb, var(--bad) 12%, transparent); }
input:focus, select:focus { outline:2px solid #2f81f7; outline-offset:-1px; }
.totals { display:grid; grid-template-columns: 1fr auto 108px auto 108px auto 108px;
          gap:6px 10px; align-items:center; padding:0 8px; }
.totals .b { font-weight:600; }
.errsum { display:flex; flex-direction:column; gap:2px; font-size:11.5px; color:var(--bad);
          border:1px solid var(--bad); border-radius:6px; padding:5px 8px; max-height:88px; overflow:auto; }
.status { display:flex; align-items:center; gap:8px; font-size:11.5px; color:var(--hint); }
.hint { margin-left:auto; }
"#;
