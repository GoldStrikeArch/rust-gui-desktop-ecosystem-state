// Grid (slint) — SPEC-7: 100k-row virtualized table.
//
// Architecture: the 100,000 rows live in Rust inside a custom `slint::Model`
// implementation (`GridModel`). Slint's ListView only instantiates visible
// items and calls `row_data(i)` lazily — this is Slint's intended big-data
// path. Filtering/sorting never touch the UI-side model: they recompute a
// `view: Vec<u32>` index vector and fire `ModelNotify::reset()`.
//
// Prints (spec): `BUILD_MS <ms>` once at startup, `FILTER_MS <query_len> <ms>`
// per filter application. `GRID_SELFTEST=1` runs a scripted verification
// sequence (filters, sorts, selection, column resize, full-range autoscroll)
// and prints ROWDATA_CALLS counters proving virtualization.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

use slint::{ComponentHandle, Model, ModelNotify, ModelRc, ModelTracker, SharedString, VecModel};

slint::include_modules!();

const N_ROWS: usize = 100_000;

const ADJ: [&str; 24] = [
    "amber", "brisk", "coral", "dusty", "eager", "fuzzy", "glossy", "hazel", "icy", "jolly",
    "keen", "lunar", "mossy", "noble", "opal", "prime", "quiet", "rusty", "solar", "tidal",
    "umber", "vivid", "wired", "zesty",
];
const NOUN: [&str; 24] = [
    "anchor", "beacon", "cobalt", "delta", "ember", "falcon", "garnet", "harbor", "island",
    "jasper", "kernel", "lagoon", "marble", "nectar", "orbit", "prism", "quartz", "ridge",
    "summit", "tundra", "umbra", "vertex", "willow", "zenith",
];
const CATS: [&str; 8] =
    ["Alpha", "Bravo", "Charlie", "Delta", "Echo", "Foxtrot", "Golf", "Hotel"];

/// xorshift* PRNG — deterministic, avoids the `rand` crate.
struct Rng(u64);
impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0.wrapping_mul(0x2545F4914F6CDD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }
}

/// Days since 2020-01-01 -> ISO yyyy-mm-dd (avoids chrono).
fn iso_date(mut days: u32) -> String {
    let mut y = 2020u32;
    let is_leap = |y: u32| y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    loop {
        let dy = if is_leap(y) { 366 } else { 365 };
        if days >= dy {
            days -= dy;
            y += 1;
        } else {
            break;
        }
    }
    let ml = [31, if is_leap(y) { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut m = 0usize;
    while days >= ml[m] {
        days -= ml[m];
        m += 1;
    }
    format!("{:04}-{:02}-{:02}", y, m + 1, days + 1)
}

struct RowData {
    id: u32,
    name: SharedString,
    name_lc: String, // lowercase copy for substring filtering
    category: u8,
    value: f64,
    value_s: SharedString, // pre-formatted, 2 decimals
    date_days: u32,        // sort key
    date_s: SharedString,
    status: u8, // 0 Ok, 1 Warn, 2 Err
}

fn generate_rows() -> Vec<RowData> {
    let mut rng = Rng(0x9E3779B97F4A7C15);
    (0..N_ROWS)
        .map(|i| {
            let name = format!(
                "{}-{}-{:04}",
                ADJ[rng.below(24) as usize],
                NOUN[rng.below(24) as usize],
                rng.below(10000)
            );
            let value = (rng.below(1_000_000) as f64) / 100.0;
            let date_days = rng.below(2557) as u32; // 2020-01-01 .. 2026-12-31
            let status = match rng.below(100) {
                0..=69 => 0u8,
                70..=89 => 1,
                _ => 2,
            };
            RowData {
                id: i as u32 + 1,
                name_lc: name.to_lowercase(),
                name: name.into(),
                category: rng.below(8) as u8,
                value,
                value_s: format!("{value:.2}").into(),
                date_days,
                date_s: iso_date(date_days).into(),
                status,
            }
        })
        .collect()
}

struct GridModel {
    rows: Vec<RowData>,
    cat_names: [SharedString; 8],
    view: RefCell<Vec<u32>>,           // filtered+sorted indices into `rows`
    sel: Cell<Option<(usize, usize)>>, // (anchor, cursor) as view indices
    sort: Cell<(i32, bool)>,           // (column, ascending); column -1 = none
    filter: RefCell<String>,
    notify: ModelNotify,
    row_data_calls: Cell<u64>, // verification: proves lazy materialization
}

impl GridModel {
    fn new(rows: Vec<RowData>) -> Self {
        let view = (0..rows.len() as u32).collect();
        GridModel {
            rows,
            cat_names: std::array::from_fn(|i| SharedString::from(CATS[i])),
            view: RefCell::new(view),
            sel: Cell::new(None),
            sort: Cell::new((-1, true)),
            filter: RefCell::new(String::new()),
            notify: ModelNotify::default(),
            row_data_calls: Cell::new(0),
        }
    }

    /// Recompute the view (filter then sort), clear selection, reset model.
    fn apply_view(&self) {
        let filter = self.filter.borrow();
        let mut v: Vec<u32> = if filter.is_empty() {
            (0..self.rows.len() as u32).collect()
        } else {
            self.rows
                .iter()
                .enumerate()
                .filter(|(_, r)| r.name_lc.contains(filter.as_str()))
                .map(|(i, _)| i as u32)
                .collect()
        };
        let (col, asc) = self.sort.get();
        let rows = &self.rows;
        match col {
            0 => v.sort_unstable_by_key(|&i| rows[i as usize].id),
            1 => v.sort_unstable_by(|&a, &b| {
                rows[a as usize].name_lc.cmp(&rows[b as usize].name_lc)
            }),
            2 => v.sort_unstable_by_key(|&i| (rows[i as usize].category, rows[i as usize].id)),
            3 => v.sort_unstable_by(|&a, &b| {
                rows[a as usize].value.total_cmp(&rows[b as usize].value)
            }),
            4 => v.sort_unstable_by_key(|&i| (rows[i as usize].date_days, rows[i as usize].id)),
            5 => v.sort_unstable_by_key(|&i| (rows[i as usize].status, rows[i as usize].id)),
            _ => {}
        }
        if col >= 0 && !asc {
            v.reverse();
        }
        *self.view.borrow_mut() = v;
        self.sel.set(None);
        self.notify.reset();
    }

    fn select(&self, index: usize, shift: bool) {
        let old = self.sel.get();
        let new = match (shift, old) {
            (true, Some((anchor, _))) => Some((anchor, index)),
            _ => Some((index, index)),
        };
        self.sel.set(new);
        // Notify only the affected view rows; fall back to reset for huge spans.
        let bounds = |r: Option<(usize, usize)>| r.map(|(a, c)| (a.min(c), a.max(c)));
        match (bounds(old), bounds(new)) {
            (o, Some((nlo, nhi))) => {
                let (lo, hi) = match o {
                    Some((olo, ohi)) => (olo.min(nlo), ohi.max(nhi)),
                    None => (nlo, nhi),
                };
                if hi - lo <= 4096 {
                    for r in lo..=hi.min(self.view.borrow().len().saturating_sub(1)) {
                        self.notify.row_changed(r);
                    }
                } else {
                    self.notify.reset();
                }
            }
            _ => self.notify.reset(),
        }
    }
}

impl Model for GridModel {
    type Data = RowItem;

    fn row_count(&self) -> usize {
        self.view.borrow().len()
    }

    fn row_data(&self, row: usize) -> Option<RowItem> {
        let view = self.view.borrow();
        let &ri = view.get(row)?;
        self.row_data_calls.set(self.row_data_calls.get() + 1);
        let r = &self.rows[ri as usize];
        let selected = match self.sel.get() {
            Some((a, c)) => row >= a.min(c) && row <= a.max(c),
            None => false,
        };
        Some(RowItem {
            id: r.id as i32,
            name: r.name.clone(),
            category: self.cat_names[r.category as usize].clone(),
            value: r.value_s.clone(),
            date: r.date_s.clone(),
            status: r.status as i32,
            selected,
        })
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }
}

fn main() {
    let t0 = Instant::now();
    let model = Rc::new(GridModel::new(generate_rows()));

    let ui = MainWindow::new().expect("failed to create window");

    let cols = Rc::new(VecModel::from(vec![
        ColDef { title: "ID".into(), width: 70.0, min_width: 50.0, numeric: true },
        ColDef { title: "Name".into(), width: 220.0, min_width: 80.0, numeric: false },
        ColDef { title: "Category".into(), width: 120.0, min_width: 70.0, numeric: false },
        ColDef { title: "Value".into(), width: 110.0, min_width: 70.0, numeric: true },
        ColDef { title: "Date".into(), width: 130.0, min_width: 90.0, numeric: false },
        ColDef { title: "Status".into(), width: 100.0, min_width: 70.0, numeric: false },
    ]));
    ui.set_cols(ModelRc::from(cols.clone()));
    ui.set_rows(ModelRc::from(model.clone() as Rc<dyn Model<Data = RowItem>>));
    ui.set_total_rows(N_ROWS as i32);
    ui.set_shown_rows(model.row_count() as i32);
    println!("BUILD_MS {}", t0.elapsed().as_millis());

    // filter-as-you-type
    {
        let model = model.clone();
        let ui_weak = ui.as_weak();
        ui.on_filter_edited(move |text| {
            let t = Instant::now();
            *model.filter.borrow_mut() = text.to_lowercase();
            model.apply_view();
            let shown = model.row_count();
            let ui = ui_weak.unwrap();
            ui.set_shown_rows(shown as i32);
            println!("FILTER_MS {} {:.2}", text.len(), t.elapsed().as_secs_f64() * 1000.0);
        });
    }

    // header click -> sort toggle
    {
        let model = model.clone();
        let ui_weak = ui.as_weak();
        ui.on_sort_clicked(move |col| {
            let (c, asc) = model.sort.get();
            let new_asc = if c == col { !asc } else { true };
            model.sort.set((col, new_asc));
            let t = Instant::now();
            model.apply_view();
            let ui = ui_weak.unwrap();
            ui.set_sort_col(col);
            ui.set_sort_asc(new_asc);
            println!("SORT_MS {} {} {:.2}", col, new_asc, t.elapsed().as_secs_f64() * 1000.0);
        });
    }

    // row click / shift-click range selection
    {
        let model = model.clone();
        ui.on_row_clicked(move |index, shift| {
            model.select(index as usize, shift);
        });
    }

    if std::env::var("GRID_SELFTEST").as_deref() == Ok("1") {
        run_selftest(&ui, model.clone(), cols);
    }

    ui.run().expect("event loop failed");
}

// ---------------------------------------------------------------------------
// Verification harness (GRID_SELFTEST=1): drives the app through the REAL
// input pipeline via `Window::dispatch_event` (synthetic key strokes into the
// focused LineEdit, header clicks, shift-clicks on rows, a divider drag, and
// wheel scrolling), then a full-range programmatic scroll sweep, then quits.
// Everything below is verification code, not production code.
// ---------------------------------------------------------------------------
/// Render the window to <app-dir>/verify-snapshot.ppm (pixel evidence).
fn save_snapshot(ui: &MainWindow) {
    let path = std::env::current_exe()
        .ok()
        .and_then(|p| p.ancestors().nth(3).map(|a| a.join("verify-snapshot.ppm")))
        .unwrap();
    match ui.window().take_snapshot() {
        Ok(buf) => {
            let (w, h) = (buf.width(), buf.height());
            let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
            for px in buf.as_slice() {
                out.extend_from_slice(&[px.r, px.g, px.b]);
            }
            std::fs::write(&path, out).unwrap();
            println!("SNAPSHOT_SAVED {} {w}x{h}", path.display());
        }
        Err(e) => println!("SNAPSHOT_FAILED {e}"),
    }
}

fn run_selftest(ui: &MainWindow, model: Rc<GridModel>, cols: Rc<VecModel<ColDef>>) {
    use slint::platform::{Key, PointerEventButton, WindowEvent};
    use slint::{LogicalPosition, Timer, TimerMode};
    use std::time::Duration;

    fn click(ui: &MainWindow, x: f32, y: f32) {
        let pos = LogicalPosition::new(x, y);
        ui.window().try_dispatch_event(WindowEvent::PointerMoved { position: pos }).unwrap();
        ui.window()
            .try_dispatch_event(WindowEvent::PointerPressed {
                position: pos,
                button: PointerEventButton::Left,
            })
            .unwrap();
        ui.window()
            .try_dispatch_event(WindowEvent::PointerReleased {
                position: pos,
                button: PointerEventButton::Left,
            })
            .unwrap();
    }
    fn type_key(ui: &MainWindow, text: &str) {
        let t: slint::SharedString = text.into();
        ui.window().try_dispatch_event(WindowEvent::KeyPressed { text: t.clone() }).unwrap();
        ui.window().try_dispatch_event(WindowEvent::KeyReleased { text: t }).unwrap();
    }

    // Column geometry (initial widths from `main`): ID 70 | Name 220 | ...
    const NAME_HDR_X: f32 = 70.0 + 110.0; // center of Name header
    const VALUE_HDR_X: f32 = 70.0 + 220.0 + 120.0 + 55.0; // center of Value header
    const DIVIDER_X: f32 = 70.0 + 220.0; // Name/Category divider
    const ROW_H: f32 = 28.0;

    let ui_weak = ui.as_weak();
    let mk_ui = |f: Box<dyn Fn(&MainWindow)>| {
        let u = ui_weak.clone();
        Box::new(move || f(&u.unwrap())) as Box<dyn Fn()>
    };

    let m1 = model.clone();
    let m2 = model.clone();
    let m3 = model.clone();
    let steps: Vec<(u64, Box<dyn Fn()>)> = vec![
        (800, Box::new(move || {
            // virtualization probe: after initial render only ~visible rows
            // should have been materialized out of 100,000
            println!("ROWDATA_CALLS_INITIAL {}", m1.row_data_calls.get());
        })),
        // filter-as-you-type through the focused LineEdit: a, m, b, e
        (1200, mk_ui(Box::new(|ui| type_key(ui, "a")))),
        (1500, mk_ui(Box::new(|ui| type_key(ui, "m")))),
        (1800, mk_ui(Box::new(|ui| type_key(ui, "b")))),
        (2100, mk_ui(Box::new(|ui| type_key(ui, "e")))),
        // clear with backspaces
        (2600, mk_ui(Box::new(|ui| {
            for _ in 0..4 {
                type_key(ui, &slint::SharedString::from(char::from(Key::Backspace)));
            }
        }))),
        // header clicks: Name asc, Name desc, Value asc
        (3200, mk_ui(Box::new(|ui| click(ui, NAME_HDR_X, ui.get_header_y() + 15.0)))),
        (3700, mk_ui(Box::new(|ui| click(ui, NAME_HDR_X, ui.get_header_y() + 15.0)))),
        (4200, mk_ui(Box::new(|ui| click(ui, VALUE_HDR_X, ui.get_header_y() + 15.0)))),
        // row click + shift-click range through real hit-testing
        (4800, mk_ui(Box::new(|ui| {
            let rows_y = ui.get_header_y() + 30.0;
            click(ui, 300.0, rows_y + 2.0 * ROW_H + 14.0);
        }))),
        (5200, Box::new({
            let u = ui_weak.clone();
            let m = m2.clone();
            move || {
                let ui = u.unwrap();
                let rows_y = ui.get_header_y() + 30.0;
                let shift = slint::SharedString::from(char::from(Key::Shift));
                ui.window()
                    .try_dispatch_event(WindowEvent::KeyPressed { text: shift.clone() })
                    .unwrap();
                click(&ui, 300.0, rows_y + 9.0 * ROW_H + 14.0);
                ui.window().try_dispatch_event(WindowEvent::KeyReleased { text: shift }).unwrap();
                println!("SELECTION {:?}", m.sel.get());
            }
        })),
        // drag the Name/Category divider 40px to the right
        (5800, Box::new({
            let u = ui_weak.clone();
            move || {
                let ui = u.unwrap();
                let y = ui.get_header_y() + 15.0;
                let w = ui.window();
                let p = |x: f32| LogicalPosition::new(x, y);
                w.try_dispatch_event(WindowEvent::PointerMoved { position: p(DIVIDER_X - 0.5) })
                    .unwrap();
                w.try_dispatch_event(WindowEvent::PointerPressed {
                    position: p(DIVIDER_X - 0.5),
                    button: PointerEventButton::Left,
                })
                .unwrap();
                for i in 1..=8 {
                    w.try_dispatch_event(WindowEvent::PointerMoved {
                        position: p(DIVIDER_X - 0.5 + 5.0 * i as f32),
                    })
                    .unwrap();
                }
                w.try_dispatch_event(WindowEvent::PointerReleased {
                    position: p(DIVIDER_X - 0.5 + 40.0),
                    button: PointerEventButton::Left,
                })
                .unwrap();
                println!("COL1_WIDTH_AFTER_DRAG {}", cols.row_data(1).unwrap().width);
            }
        })),
        // pixel evidence: selection band, sort indicator, resized column, chips
        // (skipped with GRID_SNAPSHOT=0 to keep RSS measurements unpolluted)
        (6150, mk_ui(Box::new(|ui| {
            if std::env::var("GRID_SNAPSHOT").as_deref() != Ok("0") {
                save_snapshot(ui);
            }
        }))),
        // real wheel scrolling through the Flickable
        (6400, mk_ui(Box::new(|ui| {
            let pos = LogicalPosition::new(500.0, ui.get_header_y() + 130.0);
            for _ in 0..3 {
                ui.window()
                    .try_dispatch_event(WindowEvent::PointerScrolled {
                        position: pos,
                        delta_x: 0.0,
                        delta_y: -400.0,
                    })
                    .unwrap();
            }
            println!("VIEWPORT_AFTER_WHEEL {}", ui.get_table_viewport_y());
        }))),
        (10800, Box::new({
            let m = m3.clone();
            move || {
                println!("ROWDATA_CALLS_TOTAL {}", m.row_data_calls.get());
                println!("SELFTEST_DONE");
                let _ = slint::quit_event_loop();
            }
        })),
    ];

    let timers: Vec<Timer> = steps
        .into_iter()
        .map(|(ms, f)| {
            let t = Timer::default();
            t.start(TimerMode::SingleShot, Duration::from_millis(ms), f);
            t
        })
        .collect();

    // long scroll: 7.0s..10.5s sweep the whole ~2.8M-px viewport in 25ms steps
    let ui_weak2 = ui_weak.clone();
    let start = Timer::default();
    start.start(TimerMode::SingleShot, Duration::from_millis(7000), move || {
        let inner = Timer::default();
        let u = ui_weak2.clone();
        let step = Cell::new(0u32);
        const STEPS: u32 = 140;
        inner.start(TimerMode::Repeated, Duration::from_millis(25), move || {
            let i = step.get() + 1;
            if i > STEPS {
                return;
            }
            step.set(i);
            let ui = match u.upgrade() {
                Some(ui) => ui,
                None => return,
            };
            let total = ui.get_table_viewport_height() - ui.get_table_visible_height();
            let y = -(total * i as f32 / STEPS as f32);
            ui.set_table_viewport_y(y);
            if i == STEPS {
                println!("AUTOSCROLL_DONE viewport_y={y}");
            }
        });
        // keep the repeating timer alive by leaking it (verification-only)
        std::mem::forget(inner);
    });
    std::mem::forget(start);
    std::mem::forget(timers);
    println!("SELFTEST_ARMED");
}
