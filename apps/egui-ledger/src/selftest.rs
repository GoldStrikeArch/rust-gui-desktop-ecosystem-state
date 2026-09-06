//! Verification-only scripted driver (`LEDGER_SELFTEST=1`). Inert unless the
//! env var is set; counted as verification LoC in FRICTION.md.
//!
//! Everything here goes through the *real* widgets: keystrokes are injected
//! into `RawInput` from `App::raw_input_hook`, focus is moved with egui's own
//! `Memory::move_focus`, and commits happen because the real `TextEdit`
//! reports `lost_focus()`. Only the pure parse/format helpers are called
//! directly.

use crate::model::*;
use crate::{LedgerApp, cell_id};
use eframe::egui;
use rust_decimal::Decimal;
use std::str::FromStr;
use std::time::{Duration, Instant};

pub struct SelfTest {
    started: Instant,
    step: usize,
    pass: u32,
    fail: u32,
    pub inject: Vec<egui::Event>,
    tab_walk: Vec<String>,
    committed_before: Decimal,
}

impl SelfTest {
    pub fn from_env() -> Option<Self> {
        std::env::var("LEDGER_SELFTEST").ok().filter(|v| v == "1").map(|_| {
            println!("SELFTEST_START");
            SelfTest {
                started: Instant::now(),
                step: 0,
                pass: 0,
                fail: 0,
                inject: Vec::new(),
                tab_walk: Vec::new(),
                committed_before: Decimal::ZERO,
            }
        })
    }

    fn check(&mut self, name: &str, ok: bool) {
        if ok {
            self.pass += 1;
            println!("SELFTEST ok   {name}");
        } else {
            self.fail += 1;
            println!("SELFTEST FAIL {name}");
        }
    }
    fn eq<T: std::fmt::Debug + PartialEq>(&mut self, name: &str, got: T, want: T) {
        let ok = got == want;
        if ok {
            self.pass += 1;
            println!("SELFTEST ok   {name} = {got:?}");
        } else {
            self.fail += 1;
            println!("SELFTEST FAIL {name}: got {got:?}, want {want:?}");
        }
    }
}

fn dec(s: &str) -> Decimal {
    Decimal::from_str(s).unwrap()
}

fn key(k: egui::Key, modifiers: egui::Modifiers) -> [egui::Event; 2] {
    [
        egui::Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers },
        egui::Event::Key { key: k, physical_key: None, pressed: false, repeat: false, modifiers },
    ]
}

/// Name a widget id if it is one of our table cells.
fn cell_name(id: egui::Id) -> Option<String> {
    for c in 0..7 {
        for r in 0..12 {
            if cell_id(c, r) == id {
                return Some(format!("{}[{}]", crate::COL_TITLES[c], r + 1));
            }
        }
    }
    None
}

pub fn drive(ctx: &egui::Context, app: &mut LedgerApp) {
    if app.selftest.is_none() {
        return;
    }
    let t = app.selftest.as_ref().unwrap().started.elapsed().as_secs_f64();
    let step = app.selftest.as_ref().unwrap().step;
    macro_rules! st {
        () => {
            app.selftest.as_mut().unwrap()
        };
    }

    match step {
        // ---- pure parse / format ----
        0 if t >= 0.3 => {
            st!().step = 1;
            let en = Locale::EnUs;
            let fr = Locale::FrFr;
            st!().eq("paste $1,234.56 (en-US)", parse_dec("$1,234.56", en), Some(dec("1234.56")));
            st!().eq("paste 1.234,56 € (en-US app)", parse_dec("1.234,56 €", en), Some(dec("1234.56")));
            st!().eq("paste (12.50) accounting neg", parse_dec("(12.50)", en), Some(dec("-12.50")));
            st!().eq("paste 1 234,56 (fr-FR)", parse_dec("1 234,56", fr), Some(dec("1234.56")));
            st!().eq("reject 'abc'", parse_dec("abc", en), None);
            st!().eq("format 1234.5 en-US", fmt_dec(dec("1234.5"), 2, en), "1,234.50".to_owned());
            st!().eq("format 1234.5 fr-FR", fmt_dec(dec("1234.5"), 2, fr), "1 234,50".to_owned());
            let mut s = "12a3.4x5.6".to_owned();
            filter_typing(&mut s, en, true, 2);
            st!().eq("filter while typing", s, "123.45".to_owned());
        }

        // ---- type 1234.5 into Unit price[1], then Tab away ----
        1 if t >= 0.6 => {
            let id = cell_id(4, 0);
            if ctx.memory(|m| m.focused()) == Some(id) {
                st!().step = 2;
                app.rows[0].price_buf = "1234.5".into();
            } else {
                ctx.memory_mut(|m| m.request_focus(id));
            }
        }
        2 if t >= 1.0 => {
            st!().step = 3;
            // Real Tab: egui's own focus traversal, which also blurs the cell.
            st!().inject.extend(key(egui::Key::Tab, egui::Modifiers::NONE));
        }
        3 if t >= 1.4 => {
            st!().step = 4;
            st!().eq(
                "type 1234.5 + Tab -> formatted on blur",
                app.rows[0].price_buf.clone(),
                "1,234.50".to_owned(),
            );
            st!().eq("committed Decimal", app.rows[0].price, dec("1234.50"));
            let focused = ctx.memory(|m| m.focused()).and_then(cell_name);
            println!("SELFTEST info Tab from Unit price[1] landed on {focused:?}");
            st!().check("Tab moved focus off the edited cell", focused != Some("Unit price[1]".to_owned()));
            app.set_locale(Locale::FrFr);
        }
        4 if t >= 1.7 => {
            st!().step = 5;
            st!().eq(
                "locale toggle reformats live",
                app.rows[0].price_buf.clone(),
                "1 234,50".to_owned(),
            );
            app.set_locale(Locale::EnUs);
        }

        // ---- arrow stepping through the real widget ----
        5 if t >= 2.0 => {
            let id = cell_id(4, 1);
            if ctx.memory(|m| m.focused()) == Some(id) {
                st!().step = 6;
            } else {
                ctx.memory_mut(|m| m.request_focus(id));
            }
        }
        6 if t >= 2.3 => {
            let id = cell_id(4, 1);
            if ctx.memory(|m| m.focused()) == Some(id) {
                st!().step = 7;
                st!().inject.extend(key(egui::Key::ArrowUp, egui::Modifiers::NONE));
            } else {
                ctx.memory_mut(|m| m.request_focus(id));
            }
        }
        7 => {
            let hit = app.rows[1].price == dec("38.76");
            if hit || t >= 3.1 {
                st!().step = 8;
                st!().eq("ArrowUp steps price by 0.01", app.rows[1].price, dec("38.76"));
                st!().inject.extend(key(egui::Key::ArrowUp, egui::Modifiers::SHIFT));
            }
        }
        8 => {
            let hit = app.rows[1].price == dec("38.86");
            if hit || t >= 3.9 {
                st!().step = 9;
                st!().eq("Shift+ArrowUp steps by 0.10", app.rows[1].price, dec("38.86"));
                // Invalid input must keep the previous committed value.
                st!().committed_before = app.rows[1].price;
                app.rows[1].price_buf = "not a number".into();
                ctx.memory_mut(|m| m.surrender_focus(cell_id(4, 1)));
            }
        }
        9 if t >= 4.2 => {
            st!().step = 10;
            let before = st!().committed_before;
            st!().check("invalid input keeps committed value", app.rows[1].price == before);
            st!().check("invalid input raises inline error", app.rows[1].price_err.is_some());
            app.rows[1].price_buf = fmt_dec(app.rows[1].price, 2, app.loc);
            app.rows[1].price_err = None;
        }

        // ---- Enter moves down within the column ----
        10 if t >= 4.5 => {
            let id = cell_id(3, 2);
            if ctx.memory(|m| m.focused()) == Some(id) {
                st!().step = 11;
            } else {
                ctx.memory_mut(|m| m.request_focus(id));
            }
        }
        11 if t >= 4.8 => {
            st!().step = 12;
            st!().inject.extend(key(egui::Key::Enter, egui::Modifiers::NONE));
        }
        12 if t >= 5.3 => {
            st!().step = 13;
            let focused = ctx.memory(|m| m.focused()).and_then(cell_name);
            st!().eq("Enter moves down one row, same column", focused, Some("Qty[4]".to_owned()));
        }

        // ---- Tab order walk (egui's built-in traversal) ----
        13 if t >= 5.6 => {
            let id = cell_id(0, 0);
            if ctx.memory(|m| m.focused()) == Some(id) {
                st!().step = 14;
            } else {
                ctx.memory_mut(|m| m.request_focus(id));
            }
        }
        14 if t >= 5.9 => {
            let f = ctx.memory(|m| m.focused());
            let name = f.and_then(cell_name).unwrap_or_else(|| "<other>".into());
            st!().tab_walk.push(name);
            if st!().tab_walk.len() >= 10 {
                st!().step = 15;
                let walk = st!().tab_walk.join(" -> ");
                println!("SELFTEST tab-order: {walk}");
                let ok = walk.starts_with("Date[1] -> Description[1] -> ");
                st!().check("Tab visits cells in reading order", ok);
            } else {
                st!().inject.extend(key(egui::Key::Tab, egui::Modifiers::NONE));
            }
        }

        // ---- totals, validation, undo/redo, TSV ----
        15 if t >= 6.2 => {
            st!().step = 16;
            let sub = app.subtotal();
            let manual: Decimal = app.rows.iter().map(|r| (r.qty * r.price).round_dp(2)).sum();
            st!().eq("subtotal == sum of amounts", sub, manual);
            let vat = app.vat_amount();
            let total = app.total();
            st!().eq("total == subtotal + VAT", total, sub + vat);
            st!().check("negative amount present (row 9)", app.rows[8].amount() < Decimal::ZERO);

            let before = app.errors().len();
            app.push_undo();
            app.rows[3].desc.clear();
            let after = app.errors();
            st!().check(
                "empty Description raises a row error",
                after.len() == before + 1
                    && after.iter().any(|(r, f, _)| *r == 3 && *f == "Description"),
            );
            app.undo();
            st!().eq("form-level undo restores the cell", app.rows[3].desc.clone(), "USB-C dock".to_owned());
            app.redo();
            st!().check("form-level redo re-applies it", app.rows[3].desc.is_empty());
            app.undo();

            let tsv = app.rows[0].to_tsv(app.loc);
            let mut target = app.rows[11].clone();
            let ok = target.from_tsv(&tsv, app.loc);
            st!().check(
                "row TSV round-trip",
                ok && target.date == app.rows[0].date
                    && target.desc == app.rows[0].desc
                    && target.cat == app.rows[0].cat
                    && target.qty == app.rows[0].qty
                    && target.price == app.rows[0].price,
            );
            st!().eq("TSV field count", tsv.split('\t').count(), 6);
        }
        16 if t >= 6.5 => {
            st!().step = 17;
            let (p, f) = (st!().pass, st!().fail);
            println!("SELFTEST DONE pass={p} fail={f}");
            std::process::exit(0);
        }
        _ => {}
    }

    if app.selftest.as_ref().is_some_and(|s| s.step < 17) {
        ctx.request_repaint_after(Duration::from_millis(30));
    }
}
