// Ledger (slint) — SPEC-10: forms, numeric input, validation, a11y.
//
// Number model: the model is `Vec<Item>` with real `i64`/`f64` fields; the
// widgets only ever see `SharedString`s that Rust formatted for the active
// locale. Parsing, validation and formatting all live in `num.rs`. See
// FRICTION.md.
//
// Env hooks: LEDGER_SELFTEST=1 runs the scripted checks and exits 0.

mod num;
mod selftest;

use num::{Loc, allow, fmt, parse, valid_date};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;

slint::include_modules!();

pub const CATS: [&str; 6] =
    ["Travel", "Meals", "Lodging", "Software", "Hardware", "Other"];

#[derive(Clone)]
pub struct Item {
    pub date: String,
    pub desc: String,
    pub cat: usize,
    pub qty: i64,
    pub price: f64,
    pub reimb: bool,
    /// text the user last typed that did not parse (kept so the error can be
    /// shown while the committed value stays the old one)
    pub bad: [bool; 4], // date, desc, qty, price
    pub rev: i32,
}

pub struct App {
    pub items: RefCell<Vec<Item>>,
    pub rows: Rc<VecModel<Row>>,
    pub loc: std::cell::Cell<Loc>,
    pub vat: std::cell::Cell<f64>,
    pub undo: RefCell<Vec<(usize, Item)>>,
    pub ui: MainWindow,
}

fn seed() -> Vec<Item> {
    [
        ("2026-01-08", "Flight LHR-JFK", 0usize, 1i64, 482.50f64, true),
        ("2026-01-09", "Airport transfer", 0, 2, 34.75, true),
        ("2026-01-09", "Hotel Manhattan", 2, 3, 219.00, true),
        ("2026-01-10", "Team dinner", 1, 6, 41.80, false),
        ("2026-01-10", "Conference pass", 5, 1, 1295.00, true),
        ("2026-01-11", "Refund: cancelled taxi", 0, 1, -34.75, false),
        ("2026-01-11", "USB-C dock", 4, 1, 189.99, false),
        ("2026-01-12", "Lunch", 1, 4, 18.20, true),
        ("2026-01-12", "IDE licence", 3, 2, 99.00, true),
        ("2026-01-13", "Taxi to airport", 0, 1, 62.40, true),
        ("2026-01-13", "Coffee", 1, 12, 3.95, false),
        ("2026-01-14", "Excess baggage", 0, 1, 120.00, false),
    ]
    .into_iter()
    .map(|(d, s, c, q, p, r)| Item {
        date: d.into(),
        desc: s.into(),
        cat: c,
        qty: q,
        price: p,
        reimb: r,
        bad: [false; 4],
        rev: 0,
    })
    .collect()
}

impl App {
    pub fn row(&self, it: &Item) -> Row {
        let loc = self.loc.get();
        let amount = it.qty as f64 * it.price;
        Row {
            date: it.date.as_str().into(),
            desc: it.desc.as_str().into(),
            cat: it.cat as i32,
            qty: fmt(it.qty as f64, 0, loc).into(),
            price: fmt(it.price, 2, loc).into(),
            amount: fmt(amount, 2, loc).into(),
            reimb: it.reimb,
            neg: amount < 0.0,
            e_date: it.bad[0] || !valid_date(&it.date),
            e_desc: it.bad[1] || it.desc.is_empty() || it.desc.chars().count() > 60,
            e_qty: it.bad[2] || !(1..=999).contains(&it.qty),
            e_price: it.bad[3] || !(-99_999.99..=99_999.99).contains(&it.price),
            rev: it.rev,
        }
    }

    /// Rebuild every visible row + the totals + the validation summary.
    pub fn refresh(&self) {
        let items = self.items.borrow();
        for (i, it) in items.iter().enumerate() {
            self.rows.set_row_data(i, self.row(it));
        }
        let loc = self.loc.get();
        let subtotal: f64 = items.iter().map(|it| it.qty as f64 * it.price).sum();
        let vat = subtotal * self.vat.get() / 100.0;
        self.ui.set_subtotal(fmt(subtotal, 2, loc).into());
        self.ui.set_vat_amount(fmt(vat, 2, loc).into());
        self.ui.set_total(fmt(subtotal + vat, 2, loc).into());
        self.ui.set_locale_name(loc.name().into());

        let mut errs: Vec<String> = Vec::new();
        for (i, it) in items.iter().enumerate() {
            let r = self.row(it);
            if r.e_date {
                errs.push(format!("row {}: date must be YYYY-MM-DD", i + 1));
            }
            if r.e_desc {
                errs.push(format!("row {}: description must be 1–60 chars", i + 1));
            }
            if r.e_qty {
                errs.push(format!("row {}: qty must be 1–999", i + 1));
            }
            if r.e_price {
                errs.push(format!("row {}: unit price out of range / not a number", i + 1));
            }
        }
        self.ui.set_error_count(errs.len() as i32);
        self.ui.set_error_summary(errs.join("  ·  ").into());
        self.ui.set_can_undo(!self.undo.borrow().is_empty());
    }

    fn push_undo(&self, i: usize) {
        if let Some(it) = self.items.borrow().get(i) {
            self.undo.borrow_mut().push((i, it.clone()));
        }
    }

    pub fn commit(&self, r: i32, c: i32, text: &str) {
        let (Ok(i), loc) = (usize::try_from(r), self.loc.get()) else { return };
        if i >= self.items.borrow().len() {
            return;
        }
        self.push_undo(i);
        let mut changed = true;
        {
            let mut items = self.items.borrow_mut();
            let it = &mut items[i];
            match c {
                0 => {
                    it.bad[0] = false;
                    it.date = text.trim().to_string();
                }
                1 => {
                    it.bad[1] = false;
                    it.desc = text.to_string();
                }
                3 => match parse(text, loc) {
                    Some(v) => {
                        it.bad[2] = false;
                        it.qty = v.round() as i64;
                    }
                    None => it.bad[2] = true,
                },
                4 => match parse(text, loc) {
                    Some(v) => {
                        it.bad[3] = false;
                        it.price = (v * 100.0).round() / 100.0;
                    }
                    None => it.bad[3] = true,
                },
                _ => changed = false,
            }
            it.rev += 1;
        }
        if !changed {
            self.undo.borrow_mut().pop();
        }
        self.refresh();
    }

    pub fn stepby(&self, r: i32, c: i32, delta: f32) {
        let Ok(i) = usize::try_from(r) else { return };
        if i >= self.items.borrow().len() {
            return;
        }
        self.push_undo(i);
        {
            let mut items = self.items.borrow_mut();
            let it = &mut items[i];
            match c {
                3 => {
                    it.bad[2] = false;
                    it.qty = (it.qty + delta as i64).max(0);
                }
                4 => {
                    it.bad[3] = false;
                    it.price = ((it.price + delta as f64 * 0.01) * 100.0).round() / 100.0;
                }
                _ => {}
            }
            it.rev += 1;
        }
        self.refresh();
    }

    pub fn revert(&self, r: i32) {
        let Ok(i) = usize::try_from(r) else { return };
        if let Some(it) = self.items.borrow_mut().get_mut(i) {
            it.bad = [false; 4];
            it.rev += 1; // forces the widget to re-read the committed value
        }
        self.refresh();
    }

    pub fn tsv(&self, i: usize) -> String {
        let items = self.items.borrow();
        let it = &items[i];
        format!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            it.date,
            it.desc,
            CATS[it.cat.min(CATS.len() - 1)],
            it.qty,
            fmt(it.price, 2, self.loc.get()),
            if it.reimb { "yes" } else { "no" }
        )
    }

    pub fn paste_tsv(&self, i: usize, text: &str) -> bool {
        let f: Vec<&str> = text.trim_end_matches(['\n', '\r']).split('\t').collect();
        if f.len() < 5 {
            return false;
        }
        self.push_undo(i);
        {
            let mut items = self.items.borrow_mut();
            let it = &mut items[i];
            it.date = f[0].trim().to_string();
            it.desc = f[1].to_string();
            it.cat = CATS.iter().position(|c| c.eq_ignore_ascii_case(f[2].trim())).unwrap_or(5);
            if let Some(q) = parse(f[3], self.loc.get()) {
                it.qty = q.round() as i64;
            }
            if let Some(p) = parse(f[4], self.loc.get()) {
                it.price = p;
            }
            if let Some(r) = f.get(5) {
                it.reimb = matches!(r.trim(), "yes" | "true" | "1");
            }
            it.bad = [false; 4];
            it.rev += 1;
        }
        self.refresh();
        true
    }
}

fn clipboard_set(text: &str) -> bool {
    arboard::Clipboard::new().and_then(|mut c| c.set_text(text.to_string())).is_ok()
}
fn clipboard_get() -> Option<String> {
    arboard::Clipboard::new().and_then(|mut c| c.get_text()).ok()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ui = MainWindow::new()?;
    let items = seed();
    let rows = Rc::new(VecModel::from(Vec::<Row>::new()));
    let app = Rc::new(App {
        items: RefCell::new(items),
        rows: rows.clone(),
        loc: std::cell::Cell::new(Loc::EnUs),
        vat: std::cell::Cell::new(19.0),
        undo: RefCell::new(Vec::new()),
        ui: ui.clone_strong(),
    });
    for it in app.items.borrow().iter() {
        rows.push(app.row(it));
    }
    ui.set_rows(ModelRc::from(rows.clone()));
    ui.set_cats(ModelRc::from(Rc::new(VecModel::from(
        CATS.iter().map(|c| SharedString::from(*c)).collect::<Vec<_>>(),
    ))));
    ui.set_vat(19.0);
    ui.set_vat_text("19.00".into());
    ui.set_status("Ready".into());
    ui.set_on_top(std::env::var("LEDGER_TOP").is_ok());

    {
        let a = app.clone();
        ui.on_commit(move |r, c, t| a.commit(r, c, t.as_str()));
    }
    {
        let a = app.clone();
        ui.on_stepby(move |r, c, d| a.stepby(r, c, d));
    }
    {
        let a = app.clone();
        ui.on_revert(move |r, _c| a.revert(r));
    }
    {
        let a = app.clone();
        // Enter moves down / Shift+Enter up, staying in the same column.
        ui.on_navigate(move |r, c, dy| {
            let n = a.items.borrow().len() as i32;
            let nr = (r + dy).clamp(0, n - 1);
            a.ui.invoke_focus_cell(nr, c);
        });
    }
    {
        let a = app.clone();
        ui.on_set_cat(move |r, c| {
            if let Some(it) = a.items.borrow_mut().get_mut(r as usize) {
                it.cat = c.max(0) as usize;
            }
            a.refresh();
        });
    }
    {
        // ComboBox has arrow selection but no type-ahead; cycle by first letter.
        let a = app.clone();
        ui.on_type_ahead(move |r, text| {
            let Some(ch) = text.chars().next().filter(|c| c.is_ascii_alphabetic()) else {
                return;
            };
            let Some(it) = a.items.borrow().get(r as usize).map(|i| i.cat) else { return };
            let hit = (1..=CATS.len())
                .map(|k| (it + k) % CATS.len())
                .find(|&k| CATS[k].to_lowercase().starts_with(ch.to_ascii_lowercase()));
            if let Some(k) = hit {
                a.items.borrow_mut()[r as usize].cat = k;
                a.refresh();
            }
        });
    }
    {
        let a = app.clone();
        ui.on_set_reimb(move |r, v| {
            if let Some(it) = a.items.borrow_mut().get_mut(r as usize) {
                it.reimb = v;
            }
            a.refresh();
        });
    }
    {
        let a = app.clone();
        ui.on_set_vat(move |v| {
            a.vat.set(v as f64);
            a.ui.set_vat(v);
            a.ui.set_vat_text(fmt(v as f64, 2, a.loc.get()).into());
            a.refresh();
        });
    }
    {
        let a = app.clone();
        ui.on_set_vat_text(move |t| {
            if let Some(v) = parse(t.as_str(), a.loc.get()) {
                let v = v.clamp(0.0, 25.0);
                a.vat.set(v);
                a.ui.set_vat(v as f32); // slider follows the field
                a.refresh();
            }
        });
    }
    {
        let a = app.clone();
        ui.on_toggle_locale(move || {
            a.loc.set(if a.loc.get() == Loc::EnUs { Loc::FrFr } else { Loc::EnUs });
            a.ui.set_vat_text(fmt(a.vat.get(), 2, a.loc.get()).into());
            for it in a.items.borrow_mut().iter_mut() {
                it.rev += 1;
            }
            a.refresh();
        });
    }
    {
        let a = app.clone();
        ui.on_undo_edit(move || {
            if let Some((i, it)) = a.undo.borrow_mut().pop() {
                let rev = a.items.borrow()[i].rev + 1;
                a.items.borrow_mut()[i] = Item { rev, ..it };
            }
            a.refresh();
        });
    }
    {
        let a = app.clone();
        ui.on_copy_row(move |r| {
            let Ok(i) = usize::try_from(r) else { return };
            if i < a.items.borrow().len() {
                let t = a.tsv(i);
                a.ui.set_status(
                    if clipboard_set(&t) { format!("Copied row {}: {t}", i + 1) } else { "Clipboard error".into() }
                        .into(),
                );
            }
        });
    }
    {
        let a = app.clone();
        ui.on_paste_row(move |r| {
            let Ok(i) = usize::try_from(r) else { return };
            match clipboard_get() {
                Some(t) if a.paste_tsv(i, &t) => {
                    a.ui.set_status(format!("Pasted into row {}", i + 1).into())
                }
                _ => a.ui.set_status("Clipboard does not hold a TSV row".into()),
            }
        });
    }
    {
        let a = app.clone();
        ui.on_picked_date(move |r, y, m| {
            if let Some(it) = a.items.borrow_mut().get_mut(r.max(0) as usize) {
                let d: String = it.date.chars().rev().take(2).collect::<Vec<_>>().into_iter().rev().collect();
                it.date = format!("{y:04}-{m:02}-{}", if d.len() == 2 { d } else { "01".into() });
                it.rev += 1;
            }
            a.refresh();
        });
    }
    {
        let a = app.clone();
        ui.on_save(move || {
            a.ui.set_status(format!("Saved {} rows", a.items.borrow().len()).into())
        });
    }
    {
        let a = app.clone();
        ui.on_allow(move |kind, current, ch| allow(kind, current.as_str(), ch.as_str(), a.loc.get()));
    }

    app.refresh();
    // LEDGER_ORIGIN=x,y pins the window (logical px) so parallel scripted runs
    // on one desktop do not fight. Deferred: scale_factor() reads 1.0 until the
    // window is mapped.
    let place = slint::Timer::default();
    if let Ok(o) = std::env::var("LEDGER_ORIGIN") {
        let w = ui.as_weak();
        place.start(slint::TimerMode::SingleShot, std::time::Duration::from_millis(120), move || {
            let n: Vec<f32> = o.split(',').filter_map(|v| v.parse().ok()).collect();
            if let (Some(ui), true) = (w.upgrade(), n.len() == 2) {
                ui.window().set_position(slint::LogicalPosition::new(n[0], n[1]));
            }
        });
    }
    if std::env::var("LEDGER_SELFTEST").as_deref() == Ok("1") {
        selftest::run(app.clone());
    }
    ui.run()?;
    Ok(())
}
