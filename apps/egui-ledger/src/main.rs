//! "Ledger" per apps/SPEC-10.md — business-forms / numeric-input test for
//! egui 0.35 / eframe 0.35 on macOS.
//!
//! Number model in one sentence: the model holds `rust_decimal::Decimal`, the
//! widget holds a `String` (egui's `TextEdit` takes `&mut String` and offers
//! no mask, validator or numeric variant), and every cell owns *both* — the
//! committed `Decimal` plus the buffer being typed. Parsing lives at the
//! seams: a character filter runs on `response.changed()` (post-hoc, because
//! egui gives you the edited string, never a veto), and a full parse runs on
//! `response.lost_focus()`.

mod model;
mod selftest;

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use model::*;
use rust_decimal::Decimal;

const N_COLS: usize = 7;
pub const COL_TITLES: [&str; N_COLS] = [
    "Date",
    "Description",
    "Category",
    "Qty",
    "Unit price",
    "Amount",
    "Reimb",
];
/// `LEDGER_TRACE=1` prints one line per committed edit. Used only to make the
/// scripted macOS verification deterministic on a shared desktop (the same
/// technique as apps/egui-windows).
pub fn trace(msg: &str) {
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    if *ON.get_or_init(|| std::env::var("LEDGER_TRACE").is_ok_and(|v| v == "1")) {
        println!("TRACE {msg}");
    }
}

pub fn cell_id(col: usize, row: usize) -> egui::Id {
    egui::Id::new(("cell", col, row))
}

// ------------------------------------------------------------------ app ---

pub struct LedgerApp {
    pub rows: Vec<Row>,
    pub loc: Locale,
    pub vat: f64,
    pub selected: usize,
    /// Form-level undo/redo: whole-table snapshots pushed on every commit.
    undo: Vec<Vec<Row>>,
    redo: Vec<Vec<Row>>,
    /// Focus to claim next frame (Enter-moves-down, Tab-order self-test).
    focus_next: Option<egui::Id>,
    /// Hand-rolled combo type-ahead: (buffer, last keystroke time).
    typeahead: (String, f64),
    header_ids: Vec<egui::Id>,
    pub status: String,
    pub selftest: Option<selftest::SelfTest>,
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Ledger (egui)")
            .with_inner_size([820.0, 560.0])
            .with_position([40.0, 60.0])
            .with_resizable(true),
        ..Default::default()
    };
    eframe::run_native(
        "Ledger (egui)",
        options,
        Box::new(|_cc| Ok(Box::new(LedgerApp::new()))),
    )
}

impl LedgerApp {
    fn new() -> Self {
        let loc = Locale::EnUs;
        Self {
            rows: seed(loc),
            loc,
            vat: 20.0,
            selected: 0,
            undo: Vec::new(),
            redo: Vec::new(),
            focus_next: None,
            typeahead: (String::new(), 0.0),
            header_ids: Vec::new(),
            status: String::new(),
            selftest: selftest::SelfTest::from_env(),
        }
    }

    pub fn subtotal(&self) -> Decimal {
        self.rows.iter().map(|r| r.amount()).sum::<Decimal>()
    }
    pub fn vat_amount(&self) -> Decimal {
        (self.subtotal() * Decimal::try_from(self.vat / 100.0).unwrap_or_default()).round_dp(2)
    }
    pub fn total(&self) -> Decimal {
        self.subtotal() + self.vat_amount()
    }

    /// Row/field validation. Also drives the Save button and error summary.
    pub fn errors(&self) -> Vec<(usize, &'static str, String)> {
        let mut out = Vec::new();
        for (i, r) in self.rows.iter().enumerate() {
            if !date_valid(&r.date) {
                out.push((i, "Date", "not a valid YYYY-MM-DD date".into()));
            }
            let n = r.desc.trim().chars().count();
            if n == 0 {
                out.push((i, "Description", "required".into()));
            } else if n > 60 {
                out.push((i, "Description", format!("{n} chars, max 60")));
            }
            if let Some(e) = &r.qty_err {
                out.push((i, "Qty", e.clone()));
            } else if r.qty < Decimal::ONE || r.qty > Decimal::from(999) {
                out.push((i, "Qty", "must be 1..999".into()));
            }
            if let Some(e) = &r.price_err {
                out.push((i, "Unit price", e.clone()));
            }
        }
        out
    }

    pub fn push_undo(&mut self) {
        self.undo.push(self.rows.clone());
        if self.undo.len() > 64 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn undo(&mut self) -> bool {
        if let Some(prev) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.rows, prev));
            self.status = "undo".into();
            true
        } else {
            false
        }
    }
    pub fn redo(&mut self) -> bool {
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.rows, next));
            self.status = "redo".into();
            true
        } else {
            false
        }
    }

    pub fn set_locale(&mut self, loc: Locale) {
        trace(&format!("locale -> {}", loc.label()));
        self.loc = loc;
        for r in &mut self.rows {
            r.reformat(loc);
        }
    }

    // ----------------------------------------------------------- toolbar --
    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label("Locale:");
            let mut loc = self.loc;
            egui::ComboBox::from_id_salt("locale")
                .selected_text(loc.label())
                .width(84.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut loc, Locale::EnUs, "en-US");
                    ui.selectable_value(&mut loc, Locale::FrFr, "fr-FR");
                });
            if loc != self.loc {
                self.set_locale(loc);
            }

            ui.separator();
            ui.label("VAT %:");
            // Slider and DragValue bound to the same f64 => two-way linkage
            // with no glue at all.
            ui.add(
                egui::Slider::new(&mut self.vat, 0.0..=25.0)
                    .step_by(0.5)
                    .show_value(false),
            );
            ui.add(egui::DragValue::new(&mut self.vat).speed(0.1).range(0.0..=25.0).max_decimals(1));

            ui.separator();
            let errs = self.errors();
            ui.add_enabled_ui(errs.is_empty(), |ui| {
                if ui.button("Save").clicked() {
                    self.status = format!("saved {} rows", self.rows.len());
                }
            });
            if errs.is_empty() {
                ui.colored_label(egui::Color32::from_rgb(0x2e, 0x7d, 0x32), "no errors");
            } else {
                ui.colored_label(
                    egui::Color32::from_rgb(0xc6, 0x28, 0x28),
                    format!("{} error(s)", errs.len()),
                );
            }
            ui.separator();
            ui.small("Cmd+L locale · Cmd+C/V row TSV · Cmd+Z / Cmd+Shift+Z undo · Enter = next row");
            if !self.status.is_empty() {
                ui.separator();
                ui.small(&self.status);
            }
        });
    }

    // ------------------------------------------------------------- table --
    fn table(&mut self, ui: &mut egui::Ui) {
        let loc = self.loc;
        let mono = egui::FontId::monospace(13.0);
        let row_h = 26.0;
        let mut select: Option<usize> = None;
        let mut commit_undo = false;
        let mut nav: Option<(usize, usize)> = None; // (col, row) to focus
        let mut header_ids = Vec::with_capacity(N_COLS);

        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::exact(28.0)) // row number / selection
            .column(Column::initial(104.0).at_least(90.0)) // Date
            .column(Column::initial(168.0).at_least(90.0).clip(true)) // Description
            .column(Column::initial(104.0).at_least(88.0)) // Category
            .column(Column::initial(66.0).at_least(50.0)) // Qty
            .column(Column::initial(110.0).at_least(80.0)) // Unit price
            .column(Column::initial(102.0).at_least(80.0)) // Amount
            .column(Column::remainder().at_least(56.0)) // Reimb
            .header(22.0, |mut h| {
                h.col(|ui| {
                    ui.small("#");
                });
                for t in COL_TITLES {
                    h.col(|ui| {
                        // The header label's Id is reused as the accessible
                        // name of every cell below it (`Response::labelled_by`).
                        header_ids.push(ui.label(egui::RichText::new(t).strong()).id);
                    });
                }
            })
            .body(|body| {
                body.rows(row_h, self.rows.len(), |mut tr| {
                    let i = tr.index();
                    tr.set_selected(self.selected == i);
                    tr.col(|ui| {
                        if ui.selectable_label(self.selected == i, format!("{}", i + 1)).clicked() {
                            select = Some(i);
                        }
                    });

                    // --- Date (masked text) ---
                    tr.col(|ui| {
                        let bad = !date_valid(&self.rows[i].date);
                        let r = text_cell(
                            ui,
                            cell_id(0, i),
                            &mut self.rows[i].date,
                            &mono,
                            bad,
                            self.header_ids.first().copied(),
                        );
                        if r.changed() {
                            filter_date(&mut self.rows[i].date);
                        }
                        if let Some(d) = enter_nav(ui, &r) {
                            nav = Some((0, step_row(i, d, self.rows.len())));
                        }
                        if r.lost_focus() {
                            commit_undo = true;
                        }
                    });

                    // --- Description ---
                    tr.col(|ui| {
                        let n = self.rows[i].desc.trim().chars().count();
                        let bad = n == 0 || n > 60;
                        let r = text_cell(
                            ui,
                            cell_id(1, i),
                            &mut self.rows[i].desc,
                            &egui::FontId::proportional(13.0),
                            bad,
                            self.header_ids.get(1).copied(),
                        );
                        if let Some(d) = enter_nav(ui, &r) {
                            nav = Some((1, step_row(i, d, self.rows.len())));
                        }
                        if r.lost_focus() {
                            commit_undo = true;
                        }
                    });

                    // --- Category (combo + hand-rolled type-ahead) ---
                    tr.col(|ui| {
                        let mut cat = self.rows[i].cat;
                        let resp = egui::ComboBox::from_id_salt(cell_id(2, i))
                            .selected_text(CATEGORIES[cat])
                            .width(ui.available_width())
                            .show_ui(ui, |ui| {
                                if let Some(hit) = type_ahead(ui, &mut self.typeahead) {
                                    cat = hit;
                                }
                                for (k, c) in CATEGORIES.iter().enumerate() {
                                    ui.selectable_value(&mut cat, k, *c);
                                }
                            });
                        if let Some(id) = self.header_ids.get(2) {
                            resp.response.labelled_by(*id);
                        }
                        if cat != self.rows[i].cat {
                            self.rows[i].cat = cat;
                            commit_undo = true;
                        }
                    });

                    // --- Qty (integer) ---
                    tr.col(|ui| {
                        let out = num_cell(
                            ui,
                            cell_id(3, i),
                            NumCfg {
                                loc,
                                dp: 0,
                                step: Decimal::ONE,
                                min: Decimal::ZERO,
                                max: Decimal::from(999),
                                allow_neg: false,
                                font: mono.clone(),
                                label: self.header_ids.get(3).copied(),
                            },
                            {
                                let r = &mut self.rows[i];
                                (&mut r.qty, &mut r.qty_buf, &mut r.qty_err)
                            },
                        );
                        if out.committed {
                            commit_undo = true;
                        }
                        if let Some(d) = out.nav {
                            nav = Some((3, step_row(i, d, self.rows.len())));
                        }
                    });

                    // --- Unit price (2 dp, may be negative) ---
                    tr.col(|ui| {
                        let out = num_cell(
                            ui,
                            cell_id(4, i),
                            NumCfg {
                                loc,
                                dp: 2,
                                step: Decimal::new(1, 2),
                                min: Decimal::new(-9_999_999, 2),
                                max: Decimal::new(9_999_999, 2),
                                allow_neg: true,
                                font: mono.clone(),
                                label: self.header_ids.get(4).copied(),
                            },
                            {
                                let r = &mut self.rows[i];
                                (&mut r.price, &mut r.price_buf, &mut r.price_err)
                            },
                        );
                        if out.committed {
                            commit_undo = true;
                        }
                        if let Some(d) = out.nav {
                            nav = Some((4, step_row(i, d, self.rows.len())));
                        }
                    });

                    // --- Amount (computed) ---
                    tr.col(|ui| {
                        let a = self.rows[i].amount();
                        money_label(ui, a, loc, &mono);
                    });

                    // --- Reimbursable ---
                    tr.col(|ui| {
                        let r = ui.checkbox(&mut self.rows[i].reimb, "");
                        let changed = r.changed();
                        if let Some(id) = self.header_ids.get(6) {
                            r.labelled_by(*id);
                        }
                        if changed {
                            commit_undo = true;
                        }
                    });
                });
            });

        if !header_ids.is_empty() {
            self.header_ids = header_ids;
        }
        if let Some(i) = select {
            self.selected = i;
        }
        if commit_undo {
            self.push_undo();
        }
        if let Some((c, r)) = nav {
            self.focus_next = Some(cell_id(c, r));
            self.selected = r;
        }
    }

    // ------------------------------------------------------------ footer --
    fn footer(&mut self, ui: &mut egui::Ui) {
        let mono = egui::FontId::monospace(13.0);
        let errs = self.errors();
        ui.horizontal(|ui| {
            ui.label("Subtotal");
            money_label_fixed(ui, self.subtotal(), self.loc, &mono, 120.0);
            ui.separator();
            ui.label(format!("VAT {:.1}%", self.vat));
            money_label_fixed(ui, self.vat_amount(), self.loc, &mono, 120.0);
            ui.separator();
            ui.strong("Total");
            money_label_fixed(ui, self.total(), self.loc, &mono, 120.0);
        });
        if !errs.is_empty() {
            ui.separator();
            egui::ScrollArea::vertical().max_height(74.0).show(ui, |ui| {
                for (row, field, msg) in errs.iter().take(24) {
                    ui.colored_label(
                        egui::Color32::from_rgb(0xc6, 0x28, 0x28),
                        format!("row {} · {field}: {msg}", row + 1),
                    );
                }
            });
        }
    }

    // --------------------------------------------------------- shortcuts --
    fn shortcuts(&mut self, ctx: &egui::Context) {
        // A focused TextEdit owns ⌘Z/⌘C/⌘V (egui's own per-field undoer and
        // clipboard). Form-level versions only run when no field has focus.
        let field_focused = ctx.memory(|m| m.focused()).is_some();
        let cmd = egui::Modifiers::COMMAND;
        let hit = |m: egui::Modifiers, k: egui::Key| {
            ctx.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(m, k)))
        };
        // Cmd+L toggles the locale from the keyboard (same code path as the
        // toolbar combo) — a real feature, and the only way to drive the
        // toggle reliably from a script on a shared desktop.
        if hit(cmd, egui::Key::L) {
            self.set_locale(if self.loc == Locale::EnUs { Locale::FrFr } else { Locale::EnUs });
        }
        if !field_focused {
            if hit(cmd, egui::Key::Z) {
                self.undo();
            }
            if hit(cmd | egui::Modifiers::SHIFT, egui::Key::Z) {
                self.redo();
            }
            // NOTE: egui-winit turns Cmd+C / Cmd+V into `Event::Copy` /
            // `Event::Paste(String)` before egui sees a key press, so
            // `consume_shortcut(COMMAND, Key::C)` never fires. Match the
            // semantic event instead.
            if ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Copy))) {
                let tsv = self.rows[self.selected].to_tsv(self.loc);
                ctx.copy_text(tsv.clone());
                self.status = format!("copied row {}", self.selected + 1);
                trace(&format!("copy row {} = {tsv:?}", self.selected + 1));
            }
            let pasted: Option<String> = ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Paste(t) => Some(t.clone()),
                    _ => None,
                })
            });
            if let Some(t) = pasted {
                self.push_undo();
                let loc = self.loc;
                let i = self.selected;
                if self.rows[i].from_tsv(&t, loc) {
                    self.status = format!("pasted into row {}", i + 1);
                    trace(&format!("paste row {} <- {t:?}", i + 1));
                } else {
                    self.status = "clipboard is not a TSV row".into();
                }
            }
        }
    }
}

// ------------------------------------------------------------- widgets ---

fn step_row(i: usize, d: i32, n: usize) -> usize {
    (i as i32 + d).clamp(0, n as i32 - 1) as usize
}

/// Enter / ⇧Enter on a just-blurred cell = spreadsheet down/up.
fn enter_nav(ui: &egui::Ui, r: &egui::Response) -> Option<i32> {
    if !r.lost_focus() {
        return None;
    }
    ui.input(|i| {
        i.events.iter().find_map(|e| match e {
            egui::Event::Key { key: egui::Key::Enter, pressed: true, modifiers, .. } => {
                Some(if modifiers.shift { -1 } else { 1 })
            }
            _ => None,
        })
    })
}

fn text_cell(
    ui: &mut egui::Ui,
    id: egui::Id,
    buf: &mut String,
    font: &egui::FontId,
    bad: bool,
    label: Option<egui::Id>,
) -> egui::Response {
    let r = ui.add(
        egui::TextEdit::singleline(buf)
            .id(id)
            .font(font.clone())
            .char_limit(60)
            .margin(egui::Margin::symmetric(4, 2))
            .desired_width(ui.available_width()),
    );
    if bad {
        error_ring(ui, r.rect);
    }
    if let Some(l) = label {
        r.clone().labelled_by(l);
    }
    r
}

fn error_ring(ui: &egui::Ui, rect: egui::Rect) {
    ui.painter().rect_stroke(
        rect,
        2.0,
        egui::Stroke::new(1.5, egui::Color32::from_rgb(0xc6, 0x28, 0x28)),
        egui::StrokeKind::Inside,
    );
}

struct NumCfg {
    loc: Locale,
    dp: u32,
    step: Decimal,
    min: Decimal,
    max: Decimal,
    allow_neg: bool,
    font: egui::FontId,
    label: Option<egui::Id>,
}

#[derive(Default)]
struct NumOut {
    committed: bool,
    nav: Option<i32>,
}

/// The numeric field SPEC-10 asks for, hand-rolled on `TextEdit`.
///
/// egui ships `DragValue`, which is a numeric field — but it steps only with
/// unmodified ↑/↓ by its `speed` (no ⇧×10: `drag_value.rs:496` reads
/// `count_and_consume_key(Modifiers::NONE, …)`), it has no locale, no error
/// state, and its `custom_parser` cannot reject input while typing. So the
/// cell is a `TextEdit` plus everything below.
fn num_cell(
    ui: &mut egui::Ui,
    id: egui::Id,
    cfg: NumCfg,
    cell: (&mut Decimal, &mut String, &mut Option<String>),
) -> NumOut {
    let (value, buf, err) = cell;
    let mut out = NumOut::default();
    let r = ui.add(
        egui::TextEdit::singleline(buf)
            .id(id)
            .font(cfg.font)
            .horizontal_align(egui::Align::RIGHT)
            .margin(egui::Margin::symmetric(4, 2))
            .char_limit(16)
            .desired_width(ui.available_width()),
    );
    if let Some(l) = cfg.label {
        r.clone().labelled_by(l);
    }

    // 1. Filter while typing (post-hoc: egui hands back the edited String).
    if r.changed() {
        filter_typing(buf, cfg.loc, cfg.allow_neg, cfg.dp);
    }

    // 2. Arrow stepping while focused; ⇧ multiplies by 10.
    //
    // NOTE: `Response::has_focus()` is `input.focused && memory.has_focus(id)`
    // — it is false whenever the *OS window* is not focused, which silently
    // disables this branch for any scripted/background run. Keyboard focus
    // inside the app is `Memory::has_focus`.
    if ui.ctx().memory(|m| m.has_focus(id)) {
        // Read the modifiers off the *event*, not `InputState::modifiers`:
        // the latter is a separate `RawInput` field, so a synthesised key
        // event carries its own shift state and the two can disagree.
        let arrow = ui.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Key { key, pressed: true, modifiers, .. } => match key {
                    egui::Key::ArrowUp => Some((true, modifiers.shift)),
                    egui::Key::ArrowDown => Some((false, modifiers.shift)),
                    _ => None,
                },
                _ => None,
            })
        });
        if let Some((up, shift)) = arrow {
            let mut s = cfg.step;
            if shift {
                s *= Decimal::TEN;
            }
            let base = parse_dec(buf, cfg.loc).unwrap_or(*value);
            let next = (if up { base + s } else { base - s }).clamp(cfg.min, cfg.max);
            *value = next.round_dp(cfg.dp);
            *buf = fmt_dec(*value, cfg.dp, cfg.loc);
            *err = None;
            out.committed = true;
        }
        // Esc reverts the uncommitted edit.
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            *buf = fmt_dec(*value, cfg.dp, cfg.loc);
            *err = None;
        }
    }

    // 3. Commit + normalise on blur.
    if r.lost_focus() {
        out.nav = enter_nav(ui, &r);
        match parse_dec(buf, cfg.loc) {
            Some(v) if v >= cfg.min && v <= cfg.max => {
                *value = v.round_dp(cfg.dp);
                *buf = fmt_dec(*value, cfg.dp, cfg.loc);
                *err = None;
                out.committed = true;
                trace(&format!("commit {:?} = {}", id, buf));
            }
            Some(_) => {
                trace(&format!("reject {:?} out of range: {buf:?}", id));
                *err = Some(format!(
                    "out of range ({} … {})",
                    fmt_dec(cfg.min, cfg.dp, cfg.loc),
                    fmt_dec(cfg.max, cfg.dp, cfg.loc)
                ));
            }
            None => {
                trace(&format!("reject {:?} not a number: {buf:?}", id));
                *err = Some("not a number".into());
            }
        }
    }

    if err.is_some() {
        error_ring(ui, r.rect);
    }
    out
}

/// Right-aligned money with a fixed number of decimals; negatives in red and
/// in accounting parentheses.
fn money_label(ui: &mut egui::Ui, v: Decimal, loc: Locale, font: &egui::FontId) {
    let neg = v.is_sign_negative() && !v.is_zero();
    // A trailing space on positives keeps the decimal separator in the same
    // column as the parenthesised negatives (monospace digits + equal
    // trailing width is the only alignment tool egui offers).
    let text = if neg {
        format!("({})", fmt_dec(-v, 2, loc))
    } else {
        format!("{} ", fmt_dec(v, 2, loc))
    };
    let color = if neg {
        egui::Color32::from_rgb(0xc6, 0x28, 0x28)
    } else {
        ui.visuals().text_color()
    };
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.label(
            egui::RichText::new(text)
                .font(font.clone())
                .color(color),
        );
    });
}

fn money_label_fixed(ui: &mut egui::Ui, v: Decimal, loc: Locale, font: &egui::FontId, w: f32) {
    ui.allocate_ui_with_layout(
        egui::vec2(w, 18.0),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            ui.label(egui::RichText::new(fmt_dec(v, 2, loc)).font(font.clone()));
        },
    );
}

/// Type-ahead for the category popup: egui's `ComboBox` has none, so collect
/// `Event::Text` while the popup is open and jump to the first prefix match.
fn type_ahead(ui: &egui::Ui, state: &mut (String, f64)) -> Option<usize> {
    let now = ui.input(|i| i.time);
    let typed: String = ui.input(|i| {
        i.events
            .iter()
            .filter_map(|e| match e {
                egui::Event::Text(t) => Some(t.as_str()),
                _ => None,
            })
            .collect()
    });
    if typed.is_empty() {
        return None;
    }
    if now - state.1 > 1.0 {
        state.0.clear();
    }
    state.1 = now;
    state.0.push_str(&typed.to_lowercase());
    let pref = state.0.clone();
    CATEGORIES
        .iter()
        .position(|c| c.to_lowercase().starts_with(&pref))
}

// ---------------------------------------------------------------- eframe --

impl eframe::App for LedgerApp {
    /// The self-test injects real key events here, before the pass reads them.
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        if let Some(st) = &mut self.selftest {
            raw_input.events.append(&mut st.inject);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.shortcuts(&ctx);

        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.add_space(2.0);
            self.toolbar(ui);
            ui.add_space(2.0);
        });
        egui::Panel::bottom("footer").show(ui, |ui| {
            ui.add_space(2.0);
            self.footer(ui);
            ui.add_space(2.0);
        });
        egui::CentralPanel::default().show(ui, |ui| self.table(ui));

        if let Some(id) = self.focus_next.take() {
            ctx.memory_mut(|m| m.request_focus(id));
        }
        selftest::drive(&ctx, self);
    }
}
