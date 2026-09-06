// Verification harness (LEDGER_SELFTEST=1). Test code only.
// Typing goes through the REAL input pipeline (`Window::try_dispatch_event`),
// so the key-pressed filter, Tab traversal and Enter navigation are exercised
// exactly as a user would exercise them.

use crate::App;
use slint::platform::{Key, WindowEvent};
use slint::{ComponentHandle, Model, SharedString, Timer, TimerMode};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

thread_local! {
    static PASS: Cell<u32> = const { Cell::new(0) };
    static FAIL: Cell<u32> = const { Cell::new(0) };
    static FOCUS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

const COLS: [&str; 6] = ["date", "desc", "cat", "qty", "price", "reimb"];

fn check(name: &str, ok: bool, detail: impl std::fmt::Display) {
    if ok {
        PASS.with(|c| c.set(c.get() + 1));
        println!("PASS {name}: {detail}");
    } else {
        FAIL.with(|c| c.set(c.get() + 1));
        println!("FAIL {name}: {detail}");
    }
}

fn key(a: &App, text: impl Into<SharedString>) {
    let t: SharedString = text.into();
    let w = a.ui.window();
    let _ = w.try_dispatch_event(WindowEvent::KeyPressed { text: t.clone() });
    let _ = w.try_dispatch_event(WindowEvent::KeyReleased { text: t });
}

fn shift_key(a: &App, k: Key) {
    let w = a.ui.window();
    let sh: SharedString = char::from(Key::Shift).into();
    let t: SharedString = char::from(k).into();
    let _ = w.try_dispatch_event(WindowEvent::KeyPressed { text: sh.clone() });
    let _ = w.try_dispatch_event(WindowEvent::KeyPressed { text: t.clone() });
    let _ = w.try_dispatch_event(WindowEvent::KeyReleased { text: t });
    let _ = w.try_dispatch_event(WindowEvent::KeyReleased { text: sh });
}

fn type_str(a: &App, s: &str) {
    for c in s.chars() {
        key(a, c.to_string());
    }
}

fn snapshot(a: &App, name: &str) {
    let dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.ancestors().nth(3).map(|x| x.join("evidence")))
        .unwrap_or_else(|| "evidence".into());
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(buf) = a.ui.window().take_snapshot() {
        let (w, h) = (buf.width(), buf.height());
        let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
        for p in buf.as_slice() {
            out.extend_from_slice(&[p.r, p.g, p.b]);
        }
        let path = dir.join(format!("{name}.ppm"));
        let _ = std::fs::write(&path, out);
        println!("SNAPSHOT {}", path.display());
    }
}

fn price(a: &App, row: usize) -> String {
    a.rows.row_data(row).unwrap().price.to_string()
}

pub fn run(app: Rc<App>) {
    {
        let a = app.clone();
        a.ui.on_cell_focused(move |r, c| {
            FOCUS.with(|f| {
                f.borrow_mut()
                    .push(format!("r{}:{}", r, COLS[c.clamp(0, 5) as usize]))
            });
        });
    }

    let mut steps: Vec<(u64, Box<dyn Fn()>)> = Vec::new();
    // The window must be mapped and the ListView populated before a focus
    // request can land, so everything is offset from a settle delay.
    const BASE: u64 = 900;
    let mut at = |ms: u64, f: Box<dyn Fn()>| steps.push((BASE + ms, f));

    // --- 1. typed number + Tab -> normalised, grouped, 2 decimals ----------
    {
        let a = app.clone();
        at(400, Box::new(move || a.ui.invoke_focus_cell(0, 4)));
    }
    {
        let a = app.clone();
        at(
            700,
            Box::new(move || {
                type_str(&a, "1234.5"); // focus selected the old value, so this replaces it
                key(&a, "\t"); // Tab commits via blur
            }),
        );
    }
    {
        let a = app.clone();
        at(
            1000,
            Box::new(move || {
                check(
                    "type-1234.5-tab-en-US",
                    price(&a, 0) == "1,234.50",
                    format!("row 1 unit price = {:?}", price(&a, 0)),
                );
                snapshot(&a, "en-us-aligned");
            }),
        );
    }
    // --- 2. locale toggle reformats parse + display live -------------------
    {
        let a = app.clone();
        at(1200, Box::new(move || a.ui.invoke_toggle_locale()));
    }
    {
        let a = app.clone();
        at(
            1400,
            Box::new(move || {
                check(
                    "locale-toggle-fr-FR",
                    price(&a, 0) == "1 234,50",
                    format!("row 1 unit price = {:?}, total = {:?}", price(&a, 0), a.ui.get_total()),
                );
                snapshot(&a, "fr-fr-aligned");
            }),
        );
    }
    // --- 3. paste-shaped input in both locales -----------------------------
    {
        let a = app.clone();
        at(
            1600,
            Box::new(move || {
                a.commit(1, 4, "1.234,56 €"); // fr-FR
                check(
                    "parse-euro-fr",
                    price(&a, 1) == "1 234,56",
                    format!("'1.234,56 €' -> {:?}", price(&a, 1)),
                );
                a.ui.invoke_toggle_locale(); // back to en-US
                a.commit(2, 4, "$1,234.56");
                check(
                    "parse-dollar-en",
                    price(&a, 2) == "1,234.56",
                    format!("'$1,234.56' -> {:?}", price(&a, 2)),
                );
                a.commit(3, 4, "(12.50)");
                check(
                    "parse-accounting-negative",
                    price(&a, 3) == "-12.50" && a.rows.row_data(3).unwrap().neg,
                    format!("'(12.50)' -> {:?} (rendered red)", price(&a, 3)),
                );
            }),
        );
    }
    // --- 4. typing filter --------------------------------------------------
    {
        let a = app.clone();
        at(1900, Box::new(move || a.ui.invoke_focus_cell(4, 4)));
    }
    {
        let a = app.clone();
        at(
            2100,
            Box::new(move || {
                let before = price(&a, 4);
                type_str(&a, "abc$%^");
                key(&a, "\t");
                check(
                    "typing-filter-rejects-letters",
                    price(&a, 4) == before,
                    format!("'abc$%^' typed into a decimal cell; value still {before:?}"),
                );
            }),
        );
    }
    // --- 5. step keys ------------------------------------------------------
    {
        let a = app.clone();
        at(2300, Box::new(move || a.ui.invoke_focus_cell(5, 3)));
    }
    {
        let a = app.clone();
        at(
            2500,
            Box::new(move || {
                let q0 = a.rows.row_data(5).unwrap().qty.to_string();
                key(&a, char::from(Key::UpArrow).to_string());
                let q1 = a.rows.row_data(5).unwrap().qty.to_string();
                shift_key(&a, Key::UpArrow);
                let q2 = a.rows.row_data(5).unwrap().qty.to_string();
                check(
                    "qty-step-keys",
                    q0 == "1" && q1 == "2" && q2 == "12",
                    format!("qty {q0} --Up--> {q1} --Shift+Up--> {q2}"),
                );
                a.ui.invoke_focus_cell(5, 4);
            }),
        );
    }
    {
        let a = app.clone();
        at(
            2700,
            Box::new(move || {
                let p0 = price(&a, 5);
                key(&a, char::from(Key::DownArrow).to_string());
                let p1 = price(&a, 5);
                shift_key(&a, Key::DownArrow);
                let p2 = price(&a, 5);
                check(
                    "price-step-keys",
                    p0 == "-34.75" && p1 == "-34.76" && p2 == "-34.86",
                    format!("price {p0} --Down--> {p1} --Shift+Down--> {p2}"),
                );
            }),
        );
    }
    // --- 6. Enter moves down, Shift+Enter up -------------------------------
    {
        let a = app.clone();
        at(
            2900,
            Box::new(move || {
                a.ui.invoke_focus_cell(6, 4);
                FOCUS.with(|f| f.borrow_mut().clear());
            }),
        );
    }
    {
        let a = app.clone();
        at(
            3100,
            Box::new(move || {
                key(&a, char::from(Key::Return).to_string());
            }),
        );
    }
    {
        let a = app.clone();
        at(
            3300,
            Box::new(move || {
                let down = (a.ui.get_focus_row(), a.ui.get_focus_col());
                shift_key(&a, Key::Return);
                check(
                    "enter-moves-down",
                    down == (7, 4),
                    format!("Enter from r6c4 -> r{}c{}", down.0, down.1),
                );
            }),
        );
    }
    {
        let a = app.clone();
        at(
            3500,
            Box::new(move || {
                check(
                    "shift-enter-moves-up",
                    (a.ui.get_focus_row(), a.ui.get_focus_col()) == (6, 4),
                    format!("Shift+Enter -> r{}c{}", a.ui.get_focus_row(), a.ui.get_focus_col()),
                );
            }),
        );
    }
    // --- 7. Tab-order walk logged from focus events ------------------------
    {
        let a = app.clone();
        at(
            3700,
            Box::new(move || {
                a.ui.invoke_focus_cell(8, 0);
                FOCUS.with(|f| f.borrow_mut().clear());
            }),
        );
    }
    {
        let a = app.clone();
        at(
            3900,
            Box::new(move || {
                for _ in 0..8 {
                    key(&a, "\t");
                }
                let walk = FOCUS.with(|f| f.borrow().join(" -> "));
                println!("TAB_ORDER {walk}");
                check(
                    "tab-order-reading-order",
                    walk.starts_with("r8:date -> r8:desc -> r8:qty -> r8:price -> r9:date"),
                    walk,
                );
            }),
        );
    }
    // --- 8. Esc reverts the uncommitted edit -------------------------------
    {
        let a = app.clone();
        at(4200, Box::new(move || a.ui.invoke_focus_cell(10, 4)));
    }
    {
        let a = app.clone();
        at(
            4400,
            Box::new(move || {
                let before = price(&a, 10);
                type_str(&a, "999");
                key(&a, char::from(Key::Escape).to_string());
                key(&a, "\t");
                check(
                    "esc-reverts-cell",
                    price(&a, 10) == before,
                    format!("typed 999 then Esc; committed value still {before:?}"),
                );
            }),
        );
    }
    // --- 9. validation + disabled Save + error summary ---------------------
    {
        let a = app.clone();
        at(
            4700,
            Box::new(move || {
                a.commit(11, 1, ""); // empty description
                check(
                    "validation-blocks-save",
                    a.ui.get_error_count() == 1
                        && a.ui.get_error_summary().contains("row 12: description"),
                    format!(
                        "errors={} summary={:?}",
                        a.ui.get_error_count(),
                        a.ui.get_error_summary()
                    ),
                );
                snapshot(&a, "validation-error");
                a.ui.invoke_undo_edit();
            }),
        );
    }
    {
        let a = app.clone();
        at(
            4900,
            Box::new(move || {
                check(
                    "form-level-undo",
                    a.ui.get_error_count() == 0
                        && a.rows.row_data(11).unwrap().desc == "Excess baggage",
                    format!("undo restored row 12 desc = {:?}", a.rows.row_data(11).unwrap().desc),
                );
            }),
        );
    }
    // --- 10. live totals ---------------------------------------------------
    {
        let a = app.clone();
        at(
            5100,
            Box::new(move || {
                let t0 = a.ui.get_total().to_string();
                a.commit(0, 3, "10");
                let t1 = a.ui.get_total().to_string();
                check(
                    "live-totals",
                    t0 != t1,
                    format!("total {t0} -> {t1} after qty edit; subtotal {} vat {}",
                        a.ui.get_subtotal(), a.ui.get_vat_amount()),
                );
                a.ui.invoke_set_vat(25.0);
            }),
        );
    }
    {
        let a = app.clone();
        at(
            5300,
            Box::new(move || {
                check(
                    "slider-drives-field",
                    a.ui.get_vat_text() == "25.00",
                    format!("VAT field = {:?}", a.ui.get_vat_text()),
                );
                a.ui.invoke_set_vat_text("7.5".into());
            }),
        );
    }
    {
        let a = app.clone();
        at(
            5500,
            Box::new(move || {
                check(
                    "field-drives-slider",
                    (a.ui.get_vat() - 7.5).abs() < 0.001,
                    format!("slider value = {}", a.ui.get_vat()),
                );
            }),
        );
    }
    // --- 11. category type-ahead ------------------------------------------
    {
        let a = app.clone();
        at(
            5700,
            Box::new(move || {
                a.ui.invoke_type_ahead(0, "s".into());
                let cat = a.rows.row_data(0).unwrap().cat;
                check(
                    "combobox-type-ahead",
                    crate::CATS[cat as usize] == "Software",
                    format!("typing 's' selected {:?}", crate::CATS[cat as usize]),
                );
            }),
        );
    }
    // --- 12. row copy / paste as TSV --------------------------------------
    {
        let a = app.clone();
        at(
            5900,
            Box::new(move || {
                a.ui.invoke_copy_row(0);
                let ok = a.ui.get_status().contains("Copied row 1");
                a.ui.invoke_paste_row(9);
                let same = a.rows.row_data(9).unwrap().desc == a.rows.row_data(0).unwrap().desc;
                check(
                    "row-tsv-copy-paste",
                    ok && same,
                    format!("{} ; row 10 desc now {:?}", a.ui.get_status(), a.rows.row_data(9).unwrap().desc),
                );
                snapshot(&a, "final");
            }),
        );
    }
    {
        at(
            6200,
            Box::new(move || {
                println!(
                    "SELFTEST DONE pass={} fail={}",
                    PASS.with(|c| c.get()),
                    FAIL.with(|c| c.get())
                );
                let _ = slint::quit_event_loop();
            }),
        );
    }

    let timers: Vec<Timer> = steps
        .into_iter()
        .map(|(ms, f)| {
            let t = Timer::default();
            t.start(TimerMode::SingleShot, Duration::from_millis(ms), f);
            t
        })
        .collect();
    std::mem::forget(timers); // verification-only leak
    println!("SELFTEST ARMED");
}
