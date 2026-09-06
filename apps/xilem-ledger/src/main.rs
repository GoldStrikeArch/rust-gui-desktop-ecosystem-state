//! "Ledger" (xilem) — SPEC-10 forms & numeric-input test.
//!
//! xilem 0.4 gives you `text_input`, `checkbox`, `slider` and nothing else a
//! business form needs: no numeric field, no locale, no font features, no
//! focus/blur, no validation state, no undo, no combobox, no date control.
//! Everything below is built on top of those four views plus an external
//! winit event loop (see `shell.rs`) for the parts that have no view-layer
//! API at all.

mod model;
mod shell;
mod views;

use std::sync::Arc;
use std::sync::atomic::Ordering;

use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;

use xilem::core::fork;
use xilem::core::{MessageProxy, one_of::Either};
use xilem::masonry::properties::types::AsUnit;
use xilem::masonry::theme::default_property_set;
use xilem::style::{Padding, Style as _};
use xilem::tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use xilem::view::{
    CrossAxisAlignment, FlexExt as _, FlexSpacer, checkbox, flex_col, flex_row, label, portal,
    sized_box, slider, text_button, text_input, worker,
};
use xilem::winit::dpi::{LogicalPosition, LogicalSize};
use xilem::{
    AnyWidgetView, AppState, Color, EventLoop, InsertNewline, TextAlign, WidgetView, WindowId,
    WindowView, Xilem, window,
};

use masonry_winit::app::MasonryState;

use model::{
    CATEGORIES, Field, Locale, Row, error_summary, format_decimal, mask_date, seed_rows,
    split_at_decimal, totals, typing_filter,
};
use shell::{Ev, Shared};
use views::{CellKey, num_label};

// --- MARK: palette (masonry 0.4 ships a dark-only theme) ---
const BG: Color = Color::from_rgb8(0x18, 0x1a, 0x1f);
const ROW_A: Color = Color::from_rgb8(0x1e, 0x21, 0x27);
const ROW_B: Color = Color::from_rgb8(0x23, 0x27, 0x2e);
const HEAD: Color = Color::from_rgb8(0x2b, 0x2f, 0x38);
const FG: Color = Color::from_rgb8(0xe8, 0xea, 0xf0);
const DIM: Color = Color::from_rgb8(0x9a, 0xa0, 0xb8);
const OK_BORDER: Color = Color::from_rgb8(0x3a, 0x3f, 0x4a);
const ERR: Color = Color::from_rgb8(0xe0, 0x5a, 0x5a);

// Column widths (logical px).
const W_DATE: f64 = 104.0;
const W_DESC: f64 = 208.0;
const W_CAT: f64 = 118.0;
const W_QTY: f64 = 58.0;
const W_PRICE: f64 = 104.0;
const W_INT: f64 = 66.0;
const W_FRAC: f64 = 42.0;
const W_REIMB: f64 = 46.0;

struct Edit {
    row: usize,
    field: Field,
    before: String,
    after: String,
}

struct App {
    rows: Vec<Row>,
    loc: Locale,
    vat: Decimal,
    vat_draft: Option<String>,
    dropdown: Option<usize>,
    sel_row: usize,
    undo: Vec<Edit>,
    redo: Vec<Edit>,
    status: String,
    running: bool,
    shared: Arc<Shared>,
    win: WindowId,
}

impl AppState for App {
    fn keep_running(&self) -> bool {
        self.running
    }
}

impl App {
    // --- MARK: editing ---

    /// Called on every keystroke. This is the "filter while typing" hook:
    /// `text_input` cannot constrain input, so we sanitise the string and
    /// write it back; xilem's rebuild resets the widget text when it differs.
    fn on_typed(&mut self, key: CellKey, raw: String) {
        let loc = self.loc;
        match key {
            CellKey::Vat => {
                self.vat_draft = Some(typing_filter(&raw, loc, false, true));
            }
            CellKey::Cell(r, f) => {
                let text = match f {
                    Field::Qty => typing_filter(&raw, loc, false, false),
                    Field::Price => typing_filter(&raw, loc, true, true),
                    Field::Date => mask_date(&raw),
                    _ => raw,
                };
                if f == Field::Cat {
                    self.dropdown = Some(r);
                }
                self.rows[r].draft[Row::field_index(f)] = Some(text);
            }
        }
    }

    fn commit(&mut self, key: CellKey) {
        match key {
            CellKey::Vat => {
                if let Some(d) = self.vat_draft.take() {
                    if let Some(v) = model::parse_decimal(&d, self.loc) {
                        self.vat = v.clamp(Decimal::ZERO, Decimal::from(25));
                    }
                }
            }
            CellKey::Cell(r, f) => {
                if r >= self.rows.len() {
                    return;
                }
                let before = self.rows[r].committed_text(f, self.loc);
                let _ = self.rows[r].commit(f, self.loc);
                let after = self.rows[r].committed_text(f, self.loc);
                if before != after {
                    self.undo.push(Edit {
                        row: r,
                        field: f,
                        before,
                        after,
                    });
                    self.redo.clear();
                    self.status = format!("edited row {} / {}", r + 1, f.name());
                }
            }
        }
    }

    fn set_committed(&mut self, r: usize, f: Field, text: &str) {
        self.rows[r].draft[Row::field_index(f)] = Some(text.to_string());
        let _ = self.rows[r].commit(f, self.loc);
    }

    fn step(&mut self, key: CellKey, up: bool, big: bool) {
        let mult = if big { Decimal::from(10) } else { Decimal::ONE };
        match key {
            CellKey::Vat => {
                let d = if up { Decimal::ONE } else { -Decimal::ONE } * mult;
                self.vat = (self.vat + d).clamp(Decimal::ZERO, Decimal::from(25));
                self.vat_draft = None;
            }
            CellKey::Cell(r, f) if f.numeric() => {
                let delta = if up { f.step() * mult } else { -(f.step() * mult) };
                self.rows[r].draft[Row::field_index(f)] = None;
                let cur = match f {
                    Field::Qty => self.rows[r].qty,
                    _ => self.rows[r].price,
                };
                let next = cur + delta;
                let text = format_decimal(next, f.dp(), self.loc);
                let before = self.rows[r].committed_text(f, self.loc);
                self.set_committed(r, f, &text);
                let after = self.rows[r].committed_text(f, self.loc);
                if before != after {
                    self.undo.push(Edit {
                        row: r,
                        field: f,
                        before,
                        after,
                    });
                    self.redo.clear();
                }
            }
            _ => {}
        }
    }

    fn undo(&mut self) {
        if let Some(e) = self.undo.pop() {
            let text = e.before.clone();
            self.set_committed(e.row, e.field, &text);
            self.status = format!("undo row {} / {}", e.row + 1, e.field.name());
            self.redo.push(e);
        } else {
            self.status = "nothing to undo".into();
        }
    }

    fn redo(&mut self) {
        if let Some(e) = self.redo.pop() {
            let text = e.after.clone();
            self.set_committed(e.row, e.field, &text);
            self.status = format!("redo row {} / {}", e.row + 1, e.field.name());
            self.undo.push(e);
        }
    }

    fn handle(&mut self, ev: Ev) {
        match ev {
            Ev::Focus { from, to } => {
                if let Some(k) = from {
                    self.commit(k);
                }
                if let Some(CellKey::Cell(r, f)) = to {
                    self.sel_row = r;
                    if f != Field::Cat {
                        self.dropdown = None;
                    }
                }
            }
            Ev::Step { key, up, big } => {
                self.step(key, up, big);
                if let CellKey::Cell(r, f) = key {
                    println!(
                        "STEP {} {} -> {}",
                        key.label(),
                        if big { "x10" } else { "x1" },
                        self.rows[r].committed_text(f, self.loc)
                    );
                }
            }
            Ev::Revert(k) => {
                match k {
                    CellKey::Vat => self.vat_draft = None,
                    CellKey::Cell(r, f) => {
                        self.rows[r].draft[Row::field_index(f)] = None;
                        self.rows[r].err[Row::field_index(f)] = None;
                    }
                }
                self.dropdown = None;
                self.status = "edit reverted (Esc)".into();
            }
            Ev::Undo => self.undo(),
            Ev::Redo => self.redo(),
            Ev::CopyRow => {
                let tsv = self.rows[self.sel_row].to_tsv(self.loc);
                match arboard::Clipboard::new().and_then(|mut c| c.set_text(tsv)) {
                    Ok(()) => self.status = format!("copied row {} as TSV", self.sel_row + 1),
                    Err(e) => self.status = format!("copy failed: {e}"),
                }
            }
            Ev::PasteRow(text) => {
                let loc = self.loc;
                let r = self.sel_row;
                let ok = self.rows[r].from_tsv(&text, loc);
                self.status = if ok {
                    format!("pasted TSV into row {}", r + 1)
                } else {
                    format!("pasted TSV into row {} with errors", r + 1)
                };
            }
            Ev::ToggleLocale => {
                self.loc = self.loc.other();
                self.status = format!("locale {}", self.loc.name());
            }
            Ev::SetDraft(k, text) => self.on_typed(k, text),
            Ev::Tick => {}
        }
    }

    /// Publish the display snapshot the self-test asserts against.
    fn mirror(&self) {
        let t = totals(&self.rows, self.vat);
        let errs = error_summary(&self.rows);
        let mut m = self.shared.mirror.lock().unwrap();
        m.insert("price0", format_decimal(self.rows[0].price, 2, self.loc));
        m.insert("amount0", format_decimal(self.rows[0].amount(), 2, self.loc));
        m.insert("subtotal", format_decimal(t.subtotal, 2, self.loc));
        m.insert("total", format_decimal(t.total, 2, self.loc));
        m.insert("errors", errs.len().to_string());
        m.insert(
            "save",
            if errs.is_empty() { "enabled" } else { "disabled" }.into(),
        );
        m.insert("tsv0", self.rows[0].to_tsv(self.loc));
        m.insert("locale", self.loc.name().into());
    }
}

// --- MARK: cell views ---

fn text_cell(
    state: &App,
    r: usize,
    f: Field,
    width: f64,
    align: TextAlign,
) -> impl WidgetView<App> + use<> {
    let key = CellKey::Cell(r, f);
    let idx = Row::field_index(f);
    let bad = state.rows[r].err[idx].is_some();
    let below = CellKey::Cell((r + 1).min(11), f);
    let input = text_input(state.rows[r].text(f, state.loc), move |s: &mut App, t| {
        s.on_typed(key, t)
    })
    .insert_newline(InsertNewline::Never)
    .text_alignment(align)
    .text_color(FG)
    .on_enter(move |s: &mut App, _| {
        // Spreadsheet convention: Enter commits and moves down the column.
        s.commit(key);
        *s.shared.want_focus.lock().unwrap() = Some(below);
    })
    .border(if bad { ERR } else { OK_BORDER }, if bad { 1.5 } else { 1.0 })
    .background_color(if bad {
        Color::from_rgb8(0x3a, 0x22, 0x22)
    } else {
        Color::from_rgb8(0x15, 0x17, 0x1c)
    });
    sized_box(views::cell(key, state.shared.reg.clone(), input)).width(width.px())
}

/// A decimal-aligned, tabular-figures number rendered as two fixed-width
/// halves so the separator lands on the same x in every row regardless of
/// what the font does with digit advances.
fn num_cell(v: Decimal, loc: Locale, accounting: bool) -> impl WidgetView<App> + use<> {
    let neg = v.is_sign_negative() && !v.is_zero();
    let s = if accounting && neg {
        format!("({})", format_decimal(v.abs(), 2, loc))
    } else {
        format_decimal(v, 2, loc)
    };
    let (int, frac) = split_at_decimal(&s, loc);
    let c = if neg { ERR } else { FG };
    // LEDGER_MONO=1 swaps the numeric face to the generic monospace family;
    // used to check whether the `tnum` feature request alone is enough.
    static MONO: std::sync::LazyLock<bool> =
        std::sync::LazyLock::new(|| std::env::var("LEDGER_MONO").is_ok());
    flex_row((
        sized_box(num_label(int).align(TextAlign::End).mono(*MONO).size(13.0).color(c))
            .width(W_INT.px()),
        sized_box(num_label(frac).align(TextAlign::Start).mono(*MONO).size(13.0).color(c))
            .width(W_FRAC.px()),
    ))
    .gap(0.0.px())
}

fn head(text: &str, width: f64, right: bool) -> impl WidgetView<App> + use<> {
    sized_box(
        label(text.to_string())
            .text_size(12.0)
            .text_alignment(if right { TextAlign::End } else { TextAlign::Start })
            .color(DIM),
    )
    .width(width.px())
}

// --- MARK: app logic ---

fn table(state: &App) -> Vec<Box<AnyWidgetView<App>>> {
    let mut out: Vec<Box<AnyWidgetView<App>>> = Vec::new();
    for r in 0..state.rows.len() {
        let bg = if r % 2 == 0 { ROW_A } else { ROW_B };
        let reimb = state.rows[r].reimb;
        let row = flex_row((
            text_cell(state, r, Field::Date, W_DATE, TextAlign::Start),
            text_cell(state, r, Field::Desc, W_DESC, TextAlign::Start),
            text_cell(state, r, Field::Cat, W_CAT, TextAlign::Start),
            text_cell(state, r, Field::Qty, W_QTY, TextAlign::End),
            text_cell(state, r, Field::Price, W_PRICE, TextAlign::End),
            num_cell(state.rows[r].amount(), state.loc, true),
            sized_box(checkbox("", reimb, move |s: &mut App, v| {
                s.rows[r].reimb = v;
            }))
            .width(W_REIMB.px()),
        ))
        .gap(4.0.px())
        .cross_axis_alignment(CrossAxisAlignment::Center);
        out.push(
            sized_box(row)
                .expand_width()
                .background_color(bg)
                .padding(Padding::from_vh(2.0, 4.0))
                .boxed(),
        );

        // Category "dropdown": xilem 0.4 has no combobox and no popup/overlay
        // positioned relative to a widget, so the option list is spliced into
        // the table right below the row it belongs to. Type-ahead = the list
        // is filtered by what has been typed into the cell.
        if state.dropdown == Some(r) {
            let typed = state.rows[r].text(Field::Cat, state.loc).to_lowercase();
            let opts: Vec<_> = CATEGORIES
                .iter()
                .filter(|c| c.to_lowercase().starts_with(&typed))
                .map(|c| {
                    let name = c.to_string();
                    text_button(name.clone(), move |s: &mut App| {
                        s.rows[r].draft[Row::field_index(Field::Cat)] = Some(name.clone());
                        s.commit(CellKey::Cell(r, Field::Cat));
                        s.dropdown = None;
                    })
                })
                .collect();
            out.push(
                sized_box(flex_row(opts).gap(4.0.px()))
                    .expand_width()
                    .background_color(HEAD)
                    .padding(Padding::from_vh(2.0, 8.0))
                    .boxed(),
            );
        }
    }
    out
}

fn app_logic(state: &mut App) -> impl Iterator<Item = WindowView<App>> + use<> {
    state.mirror();
    let t = totals(&state.rows, state.vat);
    let errs = error_summary(&state.rows);
    let loc = state.loc;

    let toolbar = flex_row((
        text_button(format!("Locale: {}", loc.name()), |s: &mut App| {
            s.handle(Ev::ToggleLocale)
        }),
        label("VAT").color(DIM),
        sized_box(slider(0.0, 25.0, state.vat.to_f64().unwrap_or(0.0), |s: &mut App, v| {
            s.vat = Decimal::from((v.round()) as i64);
            s.vat_draft = None;
        }))
        .width(150.px()),
        sized_box(views::cell(
            CellKey::Vat,
            state.shared.reg.clone(),
            text_input(
                state
                    .vat_draft
                    .clone()
                    .unwrap_or_else(|| format_decimal(state.vat, 0, loc)),
                |s: &mut App, t| s.on_typed(CellKey::Vat, t),
            )
            .insert_newline(InsertNewline::Never)
            .text_alignment(TextAlign::End)
            .text_color(FG)
            .on_enter(|s: &mut App, _| s.commit(CellKey::Vat))
            .border(OK_BORDER, 1.0),
        ))
        .width(52.px()),
        label("%").color(DIM),
        FlexSpacer::Flex(1.0),
        text_button(
            if errs.is_empty() {
                "Save".to_string()
            } else {
                format!("Save ({} errors)", errs.len())
            },
            |s: &mut App| s.status = "saved".into(),
        )
        .disabled(!errs.is_empty()),
    ))
    .gap(8.0.px())
    .cross_axis_alignment(CrossAxisAlignment::Center);

    let header = flex_row((
        head("Date", W_DATE, false),
        head("Description", W_DESC, false),
        head("Category", W_CAT, false),
        head("Qty", W_QTY, true),
        head("Unit price", W_PRICE, true),
        head("Amount", W_INT + W_FRAC, true),
        head("Reimb", W_REIMB, false),
    ))
    .gap(4.0.px());

    let footer_line = |name: &str, v: Decimal| {
        flex_row((
            FlexSpacer::Flex(1.0),
            sized_box(label(name.to_string()).text_alignment(TextAlign::End).color(DIM))
                .width(90.px()),
            num_cell(v, loc, false),
            sized_box(label("").color(DIM)).width(W_REIMB.px()),
        ))
        .gap(4.0.px())
    };

    let summary = if errs.is_empty() {
        Either::A(label(state.status.clone()).color(DIM))
    } else {
        Either::B(
            flex_col(
                errs.iter()
                    .take(4)
                    .map(|(r, f, m)| {
                        label(format!("row {} / {}: {}", r + 1, f.name(), m)).color(ERR)
                    })
                    .collect::<Vec<_>>(),
            )
            .gap(1.0.px()),
        )
    };

    let body = flex_col((
        sized_box(header)
            .expand_width()
            .background_color(HEAD)
            .padding(Padding::from_vh(4.0, 4.0)),
        portal(flex_col(table(state)).gap(1.0.px())).flex(1.0),
        sized_box(flex_col((
            footer_line("Subtotal", t.subtotal),
            footer_line(&format!("VAT {}%", format_decimal(state.vat, 0, loc)), t.vat),
            footer_line("Total", t.total),
        )))
        .expand_width()
        .background_color(HEAD)
        .padding(Padding::from_vh(4.0, 4.0)),
        summary,
        label(
            "Tab/Shift+Tab: next cell  ·  Enter/Shift+Enter: down/up  ·  Up/Down: step \
             (Shift = x10)  ·  Esc: revert  ·  Cmd+Z / Cmd+Shift+Z: undo/redo  ·  \
             Cmd+Shift+C / Cmd+Shift+V: row TSV",
        )
        .text_size(11.0)
        .color(DIM),
    ))
    .gap(6.0.px());

    let root = fork(
        flex_col((toolbar, body.flex(1.0)))
            .gap(8.0.px())
            .padding(Padding::all(10.0)),
        worker(
            |proxy: MessageProxy<Ev>, mut rx: UnboundedReceiver<Ev>| async move {
                while let Some(ev) = rx.recv().await {
                    if proxy.message(ev).is_err() {
                        break;
                    }
                }
            },
            |s: &mut App, tx: UnboundedSender<Ev>| {
                *s.shared.tx.lock().unwrap() = Some(tx);
            },
            |s: &mut App, ev: Ev| s.handle(ev),
        ),
    );

    let pos = std::env::var("LEDGER_POS")
        .ok()
        .and_then(|v| {
            let (x, y) = v.split_once(',')?;
            Some((x.parse::<f64>().ok()?, y.parse::<f64>().ok()?))
        })
        .unwrap_or((860.0, 60.0));

    std::iter::once(
        window(state.win, "Ledger (xilem)", root)
            .with_options(|o| {
                // Verification aid only: parallel research apps put
                // always-on-top windows over this one during scripted runs.
                let o = if std::env::var("LEDGER_TOPMOST").is_ok() {
                    o.with_window_level(xilem::winit::window::WindowLevel::AlwaysOnTop)
                } else {
                    o
                };
                o.with_initial_inner_size(LogicalSize::new(820.0, 560.0))
                    .with_initial_position(LogicalPosition::new(pos.0, pos.1))
                    .with_min_inner_size(LogicalSize::new(700.0, 360.0))
                    .on_close(|s: &mut App| s.running = false)
            })
            .with_base_color(BG),
    )
}

fn main() {
    let selftest = std::env::var("LEDGER_SELFTEST").is_ok_and(|v| v == "1");
    let demo = std::env::var("LEDGER_DEMO")
        .ok()
        .and_then(|v| v.parse::<usize>().ok());
    let shared = Arc::new(Shared {
        selftest,
        demo,
        ..Default::default()
    });

    let win = WindowId::next();
    let state = App {
        rows: seed_rows(),
        loc: Locale::EnUs,
        vat: Decimal::from(20),
        vat_draft: None,
        dropdown: None,
        sel_row: 0,
        undo: Vec::new(),
        redo: Vec::new(),
        status: "ready".into(),
        running: true,
        shared: shared.clone(),
        win,
    };

    // Heartbeat: wakes the loop so the shell layer can advance the self-test
    // and so focus polling happens even without user input.
    {
        let sh = shared.clone();
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_millis(if selftest || demo.is_some()
                {
                    200
                } else {
                    400
                }));
                sh.tick.fetch_add(1, Ordering::SeqCst);
                sh.send(Ev::Tick);
            }
        });
    }

    let xilem = Xilem::new(state, app_logic);
    let event_loop = EventLoop::with_user_event().build().unwrap();
    let proxy = event_loop.create_proxy();
    let (driver, windows) =
        xilem.into_driver_and_windows(move |event| proxy.send_event(event).map_err(|err| err.0));
    let masonry_state =
        MasonryState::new(event_loop.create_proxy(), windows, default_property_set());

    let mut app = shell::ShellApp::new(masonry_state, Box::new(driver), win, shared);
    event_loop.run_app(&mut app).unwrap();
}
