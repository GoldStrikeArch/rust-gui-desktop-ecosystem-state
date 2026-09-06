// Ledger (Tauri) — SPEC-10: numeric input, locale, validation, tabular figures.
//
// Boundary decision (the "number model", see FRICTION.md): the row model lives
// in Rust and carries `rust_decimal::Decimal`, never f64 and never String.
// The webview never parses a number: it filters keystrokes (which characters
// can still form a number in the current locale) and it FORMATS for display
// with the platform's own `Intl.NumberFormat`, which WKWebView/WebView2/
// WebKitGTK ship with full ICU data. Everything between those two ends —
// parsing, stepping, clamping, validation, totals, undo — is a
// `#[tauri::command]` returning a fresh snapshot.
//
// LEDGER_SELFTEST=1 drives the real DOM from Rust and prints
// `SELFTEST DONE pass=N fail=M`, then exits 0.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::str::FromStr;
use std::sync::Mutex;
use std::time::Duration;

use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::Serialize;
use tauri::menu::{Menu, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;

const CATEGORIES: [&str; 6] = ["Travel", "Meals", "Hardware", "Software", "Office", "Other"];

// ------------------------------------------------------------------- model

#[derive(Clone)]
struct Row {
    date: String, // ISO yyyy-mm-dd
    desc: String,
    cat: String,
    qty: i64,
    unit: Decimal, // exact 2dp money, may be negative (refunds)
    reimb: bool,
}

struct Model {
    rows: Vec<Row>,
    vat: Decimal, // percent 0..=25
    locale: String,
    undo: Vec<(usize, String, String)>, // (row, field, previous raw value)
    redo: Vec<(usize, String, String)>,
}

struct AppState(Mutex<Model>);

fn dec(s: &str) -> Decimal {
    Decimal::from_str(s).unwrap()
}

fn seed() -> Vec<Row> {
    let d = |date: &str, desc: &str, cat: &str, qty: i64, unit: &str, r: bool| Row {
        date: date.into(),
        desc: desc.into(),
        cat: cat.into(),
        qty,
        unit: dec(unit),
        reimb: r,
    };
    vec![
        d("2026-01-08", "Flight LHR-BER", "Travel", 1, "238.40", true),
        d("2026-01-09", "Airport transfer", "Travel", 2, "31.75", true),
        d("2026-01-09", "Team dinner", "Meals", 6, "24.90", true),
        d("2026-01-10", "Conference ticket", "Other", 1, "1250.00", false),
        d("2026-01-11", "Duplicate charge refund", "Travel", 1, "-31.75", true),
        d("2026-01-12", "USB-C dock", "Hardware", 1, "189.99", false),
        d("2026-01-12", "Editor licence", "Software", 4, "99.00", false),
        d("2026-01-13", "Printer paper", "Office", 12, "6.45", false),
        d("2026-01-14", "Hotel, 3 nights", "Travel", 3, "142.00", true),
        d("2026-01-15", "Client lunch", "Meals", 3, "18.25", true),
        d("2026-01-15", "Monitor arm", "Hardware", 2, "74.50", false),
        d("2026-01-16", "Coffee beans", "Office", 5, "12.80", false),
    ]
}

// ------------------------------------------------------ locale-aware parser
//
// SPEC-10 §2: `$1,234.56`, `1.234,56 €` and `(12.50)` must all parse.
// Strategy: strip currency/space noise, then decide which of `.` and `,` is
// the DECIMAL separator from their positions — the last one wins when both
// appear. When only one appears and it could be either (exactly three digits
// behind it), the active locale breaks the tie. That is the one place the
// Locale toggle changes *parsing* rather than display.
fn parse_decimal(raw: &str, locale: &str) -> Result<Decimal, String> {
    let mut t = raw.trim().to_string();
    if t.is_empty() {
        return Err("empty".into());
    }
    let mut neg = false;
    if t.starts_with('(') && t.ends_with(')') {
        neg = true; // accounting negative
        t = t[1..t.len() - 1].to_string();
    }
    t.retain(|c| !matches!(c, '$' | '€' | '£' | '¥' | ' ' | '\u{a0}' | '\u{202f}' | '\u{2009}' | '\'' | '%'));
    if t.starts_with('-') {
        neg = !neg;
        t.remove(0);
    }
    if t.starts_with('+') {
        t.remove(0);
    }
    let last_dot = t.rfind('.');
    let last_com = t.rfind(',');
    let decimal_sep = match (last_dot, last_com) {
        (Some(d), Some(c)) => Some(if d > c { '.' } else { ',' }),
        (Some(d), None) => {
            let frac = t.len() - d - 1;
            if frac == 3 {
                // "1.234" — ambiguous; fr-FR reads it as grouping, en-US as decimal
                if locale == "fr-FR" { None } else { Some('.') }
            } else {
                Some('.')
            }
        }
        (None, Some(c)) => {
            let frac = t.len() - c - 1;
            if frac == 3 {
                if locale == "fr-FR" { None } else { Some(',') }
            } else {
                Some(',')
            }
        }
        (None, None) => None,
    };
    let cleaned: String = match decimal_sep {
        Some(sep) => {
            let mut out = String::new();
            for (i, ch) in t.char_indices() {
                if ch == sep && Some(i) == t.rfind(sep) {
                    out.push('.');
                } else if ch == '.' || ch == ',' {
                    // grouping separator: drop
                } else {
                    out.push(ch);
                }
            }
            out
        }
        None => t.chars().filter(|c| *c != '.' && *c != ',').collect(),
    };
    if cleaned.is_empty() || !cleaned.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return Err(format!("not a number: {raw}"));
    }
    let mut v = Decimal::from_str(&cleaned).map_err(|e| e.to_string())?;
    if neg {
        v = -v;
    }
    Ok(v)
}

// ------------------------------------------------------------------- DTOs

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RowDto {
    date: String,
    desc: String,
    cat: String,
    qty: String,       // canonical, unformatted — JS formats with Intl
    unit: String,      // canonical "-31.75"
    amount: String,    // canonical qty * unit
    reimb: bool,
    errors: Vec<String>, // field names with a validation error
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Snap {
    rows: Vec<RowDto>,
    categories: [&'static str; 6],
    locale: String,
    vat: String,
    subtotal: String,
    vat_amount: String,
    total: String,
    errors: Vec<String>, // human-readable "row N · field: message"
    can_undo: bool,
    can_redo: bool,
    selftest: bool,
}

fn row_errors(r: &Row) -> Vec<String> {
    let mut e = Vec::new();
    let n = r.desc.chars().count();
    if n == 0 || n > 60 {
        e.push("desc".into());
    }
    if r.qty < 1 || r.qty > 999 {
        e.push("qty".into());
    }
    if r.unit < dec("-99999.99") || r.unit > dec("99999.99") {
        e.push("unit".into());
    }
    // Date needs no validator: <input type="date"> only ever hands back
    // "" or a well-formed yyyy-mm-dd.
    e
}

fn snap(m: &Model) -> Snap {
    let mut subtotal = Decimal::ZERO;
    let mut errors = Vec::new();
    let rows: Vec<RowDto> = m
        .rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let amount = (Decimal::from(r.qty) * r.unit).round_dp(2);
            subtotal += amount;
            let errs = row_errors(r);
            for f in &errs {
                let msg = match f.as_str() {
                    "desc" => "description must be 1–60 characters",
                    "qty" => "quantity must be 1–999",
                    _ => "unit price must be −99 999.99 … 99 999.99",
                };
                errors.push(format!("row {} · {f}: {msg}", i + 1));
            }
            RowDto {
                date: r.date.clone(),
                desc: r.desc.clone(),
                cat: r.cat.clone(),
                qty: r.qty.to_string(),
                unit: format!("{:.2}", r.unit),
                amount: format!("{:.2}", amount),
                reimb: r.reimb,
                errors: errs,
            }
        })
        .collect();
    let vat_amount = (subtotal * m.vat / dec("100")).round_dp(2);
    Snap {
        rows,
        categories: CATEGORIES,
        locale: m.locale.clone(),
        vat: format!("{:.2}", m.vat),
        subtotal: format!("{:.2}", subtotal),
        vat_amount: format!("{:.2}", vat_amount),
        total: format!("{:.2}", subtotal + vat_amount),
        errors,
        can_undo: !m.undo.is_empty(),
        can_redo: !m.redo.is_empty(),
        selftest: std::env::var("LEDGER_SELFTEST").is_ok(),
    }
}

fn broadcast(app: &AppHandle) {
    let st = app.state::<AppState>();
    let s = { snap(&st.0.lock().unwrap()) };
    let _ = app.emit("model", s);
}

/// The canonical raw value of one cell, used for the undo stack.
fn raw_of(r: &Row, field: &str) -> String {
    match field {
        "date" => r.date.clone(),
        "desc" => r.desc.clone(),
        "cat" => r.cat.clone(),
        "qty" => r.qty.to_string(),
        "unit" => format!("{:.2}", r.unit),
        "reimb" => r.reimb.to_string(),
        _ => String::new(),
    }
}

/// Single write path. Returns Ok(canonical) or Err(message); on Err the
/// previous committed value is kept (SPEC-10 §2 last bullet).
fn apply(m: &mut Model, i: usize, field: &str, value: &str) -> Result<String, String> {
    let locale = m.locale.clone();
    let r = m.rows.get_mut(i).ok_or("no such row")?;
    match field {
        "date" => r.date = value.to_string(),
        "desc" => r.desc = value.to_string(),
        "cat" => r.cat = value.to_string(),
        "reimb" => r.reimb = value == "true",
        "qty" => {
            let v = parse_decimal(value, &locale)?;
            let n = v.round().to_i64().ok_or("out of range")?;
            if !(0..=99_999).contains(&n) {
                return Err("quantity out of range".into());
            }
            r.qty = n;
        }
        "unit" => {
            let v = parse_decimal(value, &locale)?.round_dp(2);
            if v.abs() > dec("999999.99") {
                return Err("price out of range".into());
            }
            r.unit = v;
        }
        _ => return Err("unknown field".into()),
    }
    Ok(raw_of(&m.rows[i], field))
}

// ---------------------------------------------------------------- commands

#[tauri::command]
fn get_model(state: State<'_, AppState>) -> Snap {
    snap(&state.0.lock().unwrap())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CellResult {
    ok: bool,
    value: String,
    message: String,
}

#[tauri::command]
fn set_cell(app: AppHandle, row: usize, field: String, value: String) -> CellResult {
    let res = {
        let st = app.state::<AppState>();
        let mut m = st.0.lock().unwrap();
        let before = m.rows.get(row).map(|r| raw_of(r, &field)).unwrap_or_default();
        match apply(&mut m, row, &field, &value) {
            Ok(v) => {
                if v != before {
                    m.undo.push((row, field.clone(), before));
                    m.redo.clear();
                }
                CellResult { ok: true, value: v, message: String::new() }
            }
            Err(e) => CellResult {
                ok: false,
                value: m.rows.get(row).map(|r| raw_of(r, &field)).unwrap_or_default(),
                message: e,
            },
        }
    };
    broadcast(&app);
    res
}

/// ↑/↓ stepping. `units` is +1/-1, scaled by the field's step and by ×10 when
/// Shift is held. Parsing stays in Rust: the webview sends the text it has.
#[tauri::command]
fn step_cell(app: AppHandle, row: usize, field: String, text: String, units: i64, shift: bool) -> CellResult {
    let res = {
        let st = app.state::<AppState>();
        let mut m = st.0.lock().unwrap();
        let locale = m.locale.clone();
        let cur = parse_decimal(&text, &locale)
            .unwrap_or_else(|_| m.rows.get(row).map(|r| if field == "qty" { Decimal::from(r.qty) } else { r.unit }).unwrap_or(Decimal::ZERO));
        let base = if field == "qty" { dec("1") } else { dec("0.01") };
        let mult = if shift { dec("10") } else { dec("1") };
        let next = cur + Decimal::from(units) * base * mult;
        let before = m.rows.get(row).map(|r| raw_of(r, &field)).unwrap_or_default();
        match apply(&mut m, row, &field, &format!("{next}")) {
            Ok(v) => {
                if v != before {
                    m.undo.push((row, field.clone(), before));
                    m.redo.clear();
                }
                CellResult { ok: true, value: v, message: String::new() }
            }
            Err(e) => CellResult { ok: false, value: before, message: e },
        }
    };
    broadcast(&app);
    res
}

#[tauri::command]
fn set_locale(app: AppHandle, locale: String) {
    app.state::<AppState>().0.lock().unwrap().locale = locale;
    broadcast(&app);
}

#[tauri::command]
fn set_vat(app: AppHandle, value: String) -> CellResult {
    let res = {
        let st = app.state::<AppState>();
        let mut m = st.0.lock().unwrap();
        let locale = m.locale.clone();
        match parse_decimal(&value, &locale) {
            Ok(v) if v >= Decimal::ZERO && v <= dec("25") => {
                m.vat = v.round_dp(2);
                CellResult { ok: true, value: format!("{:.2}", m.vat), message: String::new() }
            }
            _ => CellResult { ok: false, value: format!("{:.2}", m.vat), message: "VAT must be 0–25".into() },
        }
    };
    broadcast(&app);
    res
}

/// Form-level undo of the last committed cell edit (SPEC-10 §7).
#[tauri::command]
fn undo(app: AppHandle, redo: bool) -> bool {
    let done = {
        let st = app.state::<AppState>();
        let mut m = st.0.lock().unwrap();
        let popped = if redo { m.redo.pop() } else { m.undo.pop() };
        match popped {
            Some((row, field, prev)) => {
                let cur = raw_of(&m.rows[row], &field);
                let _ = apply(&mut m, row, &field, &prev);
                if redo { m.undo.push((row, field, cur)) } else { m.redo.push((row, field, cur)) }
                true
            }
            None => false,
        }
    };
    broadcast(&app);
    done
}

#[tauri::command]
fn copy_row(app: AppHandle, row: usize) -> String {
    let tsv = {
        let st = app.state::<AppState>();
        let m = st.0.lock().unwrap();
        match m.rows.get(row) {
            Some(r) => format!(
                "{}\t{}\t{}\t{}\t{:.2}\t{}",
                r.date, r.desc, r.cat, r.qty, r.unit, r.reimb
            ),
            None => String::new(),
        }
    };
    let _ = app.clipboard().write_text(tsv.clone());
    println!("[ledger] copied row {row}: {tsv:?}");
    tsv
}

#[tauri::command]
fn paste_row(app: AppHandle, row: usize) -> CellResult {
    let text = match app.clipboard().read_text() {
        Ok(t) => t,
        Err(e) => return CellResult { ok: false, value: String::new(), message: e.to_string() },
    };
    let cells: Vec<&str> = text.trim_end_matches('\n').split('\t').collect();
    if cells.len() < 5 {
        return CellResult { ok: false, value: text, message: "clipboard is not a 6-column TSV row".into() };
    }
    {
        let st = app.state::<AppState>();
        let mut m = st.0.lock().unwrap();
        for (field, v) in ["date", "desc", "cat", "qty", "unit", "reimb"].iter().zip(cells.iter()) {
            let before = raw_of(&m.rows[row], field);
            if apply(&mut m, row, field, v).is_ok() {
                m.undo.push((row, field.to_string(), before));
            }
        }
        m.redo.clear();
    }
    broadcast(&app);
    println!("[ledger] pasted TSV into row {row}: {text:?}");
    CellResult { ok: true, value: text, message: String::new() }
}

/// Parser probe used by the self-test (and handy for the evidence log).
#[tauri::command]
fn parse_probe(state: State<'_, AppState>, text: String) -> CellResult {
    let locale = state.0.lock().unwrap().locale.clone();
    match parse_decimal(&text, &locale) {
        Ok(v) => CellResult { ok: true, value: format!("{v}"), message: locale },
        Err(e) => CellResult { ok: false, value: String::new(), message: e },
    }
}

#[tauri::command]
fn report(line: String) {
    println!("{line}");
}

// ---------------------------------------------------------------- selftest

fn selftest(app: AppHandle) {
    std::thread::sleep(Duration::from_millis(2500));
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.eval("window.__ledgerSelftest && window.__ledgerSelftest()");
    }
    // The JS harness prints SELFTEST … lines through `report` and calls
    // `report('SELFTEST DONE …')` last; give it room, then quit.
    std::thread::sleep(Duration::from_secs(14));
    let a = app.clone();
    let _ = app.run_on_main_thread(move || a.exit(0));
}

// -------------------------------------------------------------------- main

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(AppState(Mutex::new(Model {
            rows: seed(),
            vat: dec("19.00"),
            locale: "en-US".into(),
            undo: Vec::new(),
            redo: Vec::new(),
        })))
        .invoke_handler(tauri::generate_handler![
            get_model, set_cell, step_cell, set_locale, set_vat, undo, copy_row, paste_row,
            parse_probe, report
        ])
        .setup(|app| {
            // The Edit menu's predefined roles are what make ⌘Z/⌘C/⌘V reach
            // WKWebView at all on macOS — without them, field-level undo in a
            // text input simply does not exist.
            let edit = Submenu::with_items(
                app,
                "Edit",
                true,
                &[
                    &PredefinedMenuItem::undo(app, None)?,
                    &PredefinedMenuItem::redo(app, None)?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::cut(app, None)?,
                    &PredefinedMenuItem::copy(app, None)?,
                    &PredefinedMenuItem::paste(app, None)?,
                    &PredefinedMenuItem::select_all(app, None)?,
                ],
            )?;
            let app_menu = Submenu::with_items(
                app,
                "Ledger",
                true,
                &[&PredefinedMenuItem::hide(app, None)?, &PredefinedMenuItem::quit(app, None)?],
            )?;
            app.set_menu(Menu::with_items(app, &[&app_menu, &edit])?)?;
            if std::env::var("LEDGER_SELFTEST").is_ok() {
                let h = app.handle().clone();
                std::thread::spawn(move || selftest(h));
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
