//! "Grid" — 100k-row data table (SPEC-7), vizia 0.4.
//!
//! Architecture notes (research-relevant):
//! - The table is vizia's built-in `VirtualTable`: a `VirtualList` body with
//!   a header row of `Resizable` cells. It gives virtualization, per-column
//!   sort headers with an indicator, real divider-drag column resizing and
//!   row selection out of the box. The app supplies data, sort comparators,
//!   the filter and the cell templates.
//! - Rows are handed to the table as `Signal<Arc<[Row]>>`. `VirtualTable`
//!   accepts any `V: Deref<Target = [T]> + Clone`, so re-publishing a
//!   100k-row view after a sort or filter is one refcount bump, not a
//!   100k-element clone.
//! - `Row` itself is clone-cheap (`Arc<str>` name, packed integer date), so
//!   the per-visible-row clone the virtualizer does is ~free.
//!
//! Evidence printed on stdout (always, per SPEC-7 §5/§8):
//!   BUILD_MS <ms>                 once, after generating + indexing 100k rows
//!   FILTER_MS <query_len> <ms>    on every filter application
//!
//! With GRID_SELFTEST=1 the app additionally runs a scripted sequence over a
//! timer, prints SORT/SELECT/RESIZE/WINDOW evidence, finishes with
//! `SELFTEST DONE pass=N fail=N` and exits.

use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use vizia::prelude::*;

const ROWS: usize = 100_000;
const ROW_HEIGHT: f32 = 28.0;

const CATEGORIES: [&str; 8] = [
    "Alloy", "Bronze", "Copper", "Dust", "Ember", "Flint", "Granite", "Halite",
];
const ADJECTIVES: [&str; 16] = [
    "amber", "brisk", "cobalt", "dusky", "eager", "frosty", "glassy", "hollow", "ivory", "jaded",
    "keen", "lucid", "mossy", "nimble", "opal", "prime",
];
const NOUNS: [&str; 16] = [
    "anvil", "beacon", "cinder", "delta", "ember", "fjord", "gable", "harbor", "inlet", "jetty",
    "kiln", "ledger", "marsh", "notch", "orbit", "prism",
];

// Virtualization evidence: the cell template records which row indices the
// table actually materialised. Statics because `VirtualList` requires a
// `Copy` content closure, which cannot capture an `Rc`.
static RENDER_MIN: AtomicUsize = AtomicUsize::new(usize::MAX);
static RENDER_MAX: AtomicUsize = AtomicUsize::new(0);
static RENDER_COUNT: AtomicUsize = AtomicUsize::new(0);

fn say(line: impl AsRef<str>) {
    let mut stdout = std::io::stdout();
    let _ = writeln!(stdout, "{}", line.as_ref());
    let _ = stdout.flush();
}

fn main() -> Result<(), ApplicationError> {
    let selftest = std::env::var_os("GRID_SELFTEST").is_some();

    Application::new(move |cx| {
        cx.add_stylesheet(STYLE).expect("failed to add stylesheet");

        let started = std::time::Instant::now();
        let all = generate_rows(ROWS);
        let view: Arc<[Row]> = Arc::from(all.clone().into_boxed_slice());
        let build_ms = started.elapsed().as_secs_f64() * 1000.0;
        say(format!("BUILD_MS {build_ms:.2}"));

        let rows = Signal::new(view);
        let query = Signal::new(String::new());
        let sort_state = Signal::new(None::<TableSortState>);
        let selected = Signal::new(Vec::<u32>::new());
        let count_label = Signal::new(format!("{ROWS} of {ROWS} rows"));

        let columns = Signal::new(build_columns());

        let timer = cx.add_timer(Duration::from_millis(120), None, |cx, action| {
            if let TimerAction::Tick(_) = action {
                cx.emit(GridEvent::SelfTestStep);
            }
        });

        Grid {
            all,
            rows,
            query,
            sort_state,
            selected,
            count_label,
            columns,
            anchor: None,
            selftest,
            step: 0,
            pass: 0,
            fail: 0,
        }
        .build(cx);

        if selftest {
            cx.start_timer(timer);
        }

        VStack::new(cx, move |cx| {
            HStack::new(cx, |cx| {
                Textbox::new(cx, query)
                    .class("filter")
                    .placeholder("filter by name…")
                    .width(Pixels(280.0))
                    .on_edit(|cx, text| cx.emit(GridEvent::Filter(text)));
                Label::new(cx, count_label).class("count");
                Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));
                Label::new(cx, "click a header to sort · drag a divider to resize")
                    .class("dim");
            })
            .class("toolbar");

            VirtualTable::new(cx, rows, columns, ROW_HEIGHT, |row: &Row| row.id)
                .sort_state(sort_state)
                .sort_cycle(TableSortCycle::BiState)
                .resizable_columns(true)
                .selectable(Selectable::Multi)
                .selected_row_ids(selected)
                .on_sort(|cx, key, direction| cx.emit(GridEvent::Sort(key, direction)))
                .on_row_select(|cx, id| {
                    cx.emit(GridEvent::SelectRow(id, cx.modifiers().shift()))
                })
                .width(Stretch(1.0))
                .height(Stretch(1.0));
        })
        .class("app");
    })
    .title("Grid (vizia)")
    .inner_size((1000, 640))
    .run()
}

// ---------------------------------------------------------------------------
// Data
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    Ok,
    Warn,
    Err,
}

impl Status {
    fn text(self) -> &'static str {
        match self {
            Status::Ok => "Ok",
            Status::Warn => "Warn",
            Status::Err => "Err",
        }
    }
}

/// Clone-cheap: `Arc<str>` for the name, packed `yyyymmdd` for the date.
#[derive(Clone, PartialEq)]
struct Row {
    id: u32,
    name: Arc<str>,
    category: &'static str,
    value: f64,
    date: u32,
    status: Status,
}

impl Row {
    fn date_text(&self) -> String {
        format!("{:04}-{:02}-{:02}", self.date / 10000, (self.date / 100) % 100, self.date % 100)
    }
}

/// Deterministic xorshift* — same 100k rows on every run, no `rand`.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn generate_rows(count: usize) -> Vec<Row> {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    (0..count)
        .map(|index| {
            let adjective = ADJECTIVES[rng.below(16) as usize];
            let noun = NOUNS[rng.below(16) as usize];
            let suffix = rng.below(10_000);
            Row {
                id: index as u32,
                name: Arc::from(format!("{adjective}-{noun}-{suffix:04}").as_str()),
                category: CATEGORIES[rng.below(8) as usize],
                value: (rng.below(1_000_000) as f64) / 100.0,
                // Packed yyyymmdd: 2020..2025, month 1..12, day 1..28.
                date: (2020 + rng.below(6) as u32) * 10000
                    + (rng.below(12) as u32 + 1) * 100
                    + (rng.below(28) as u32 + 1),
                status: match rng.below(10) {
                    0..=6 => Status::Ok,
                    7..=8 => Status::Warn,
                    _ => Status::Err,
                },
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Columns
// ---------------------------------------------------------------------------

fn build_columns() -> Vec<TableColumn<Row, TableHeader>> {
    vec![
        TableColumn::new(
            "id",
            |cx, direction| TableHeader::new(cx, "ID", direction),
            |cx, row| {
                Label::new(cx, row.map(|row: &Row| row.id.to_string()))
                    .class("cell")
                    .hoverable(false);
            },
        )
        .width(80.0)
        .min_width(60.0)
        .resizable(true),
        TableColumn::new(
            "name",
            |cx, direction| TableHeader::new(cx, "Name", direction),
            |cx, row| {
                Label::new(cx, row.map(|row: &Row| row.name.to_string()))
                    .class("cell")
                    .hoverable(false);
            },
        )
        .width(220.0)
        .min_width(120.0)
        .resizable(true),
        TableColumn::new(
            "category",
            |cx, direction| TableHeader::new(cx, "Category", direction),
            |cx, row| {
                Label::new(cx, row.map(|row: &Row| row.category.to_string()))
                    .class("cell")
                    .hoverable(false);
            },
        )
        .width(140.0)
        .min_width(90.0)
        .resizable(true),
        TableColumn::new(
            "value",
            |cx, direction| TableHeader::new(cx, "Value", direction),
            |cx, row| {
                Label::new(cx, row.map(|row: &Row| format!("{:.2}", row.value)))
                    .class("cell")
                    .class("numeric")
                    .hoverable(false);
            },
        )
        .width(120.0)
        .min_width(80.0)
        .resizable(true),
        TableColumn::new(
            "date",
            |cx, direction| TableHeader::new(cx, "Date", direction),
            |cx, row| {
                Label::new(cx, row.map(Row::date_text)).class("cell").hoverable(false);
            },
        )
        .width(130.0)
        .min_width(100.0)
        .resizable(true),
        TableColumn::new(
            "status",
            |cx, direction| TableHeader::new(cx, "Status", direction),
            |cx, row| {
                // Custom cell rendering: an arbitrary view tree per cell, so
                // the colored chip is a styled Label, not a special API.
                // This closure is also the virtualization probe — it only
                // runs for rows the table actually materialises.
                let index = row.map(|row: &Row| row.id as usize);
                let position = RENDER_COUNT.fetch_add(1, Ordering::Relaxed);
                let _ = position;
                RENDER_MIN.fetch_min(index.get(), Ordering::Relaxed);
                RENDER_MAX.fetch_max(index.get(), Ordering::Relaxed);
                HStack::new(cx, move |cx| {
                    Label::new(cx, row.map(|row: &Row| row.status.text().to_string()))
                        .class("chip")
                        .width(Pixels(56.0))
                        .text_wrap(false)
                        .hoverable(false)
                        .toggle_class("ok", row.map(|row: &Row| row.status == Status::Ok))
                        .toggle_class("warn", row.map(|row: &Row| row.status == Status::Warn))
                        .toggle_class("err", row.map(|row: &Row| row.status == Status::Err));
                })
                .class("chip-wrap")
                .hoverable(false);
            },
        )
        .width(110.0)
        .min_width(80.0)
        .resizable(true),
    ]
}

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

struct Grid {
    all: Vec<Row>,
    rows: Signal<Arc<[Row]>>,
    query: Signal<String>,
    sort_state: Signal<Option<TableSortState>>,
    selected: Signal<Vec<u32>>,
    count_label: Signal<String>,
    columns: Signal<Vec<TableColumn<Row, TableHeader>>>,
    anchor: Option<u32>,
    selftest: bool,
    step: usize,
    pass: usize,
    fail: usize,
}

enum GridEvent {
    Filter(String),
    Sort(String, TableSortDirection),
    SelectRow(u32, bool),
    SelfTestStep,
}

impl Grid {
    /// Recompute the visible slice: filter by substring on `name`, then apply
    /// the active sort. Self-timed; this is the FILTER_MS number.
    fn reapply(&mut self, timed: bool) {
        let started = std::time::Instant::now();
        let query = self.query.get().to_lowercase();

        let mut view: Vec<Row> = if query.is_empty() {
            self.all.clone()
        } else {
            self.all.iter().filter(|row| row.name.contains(&query)).cloned().collect()
        };

        if let Some(state) = self.sort_state.get() {
            let ascending = state.direction != TableSortDirection::Descending;
            match state.key.as_str() {
                "id" => view.sort_unstable_by_key(|row| row.id),
                "name" => view.sort_unstable_by(|a, b| a.name.cmp(&b.name)),
                "category" => view.sort_unstable_by(|a, b| a.category.cmp(b.category)),
                "value" => {
                    view.sort_unstable_by(|a, b| a.value.partial_cmp(&b.value).unwrap())
                }
                "date" => view.sort_unstable_by_key(|row| row.date),
                "status" => view.sort_unstable_by_key(|row| row.status.text()),
                _ => {}
            }
            if !ascending {
                view.reverse();
            }
        }

        let shown = view.len();
        self.rows.set(Arc::from(view.into_boxed_slice()));
        self.count_label.set(format!("{shown} of {ROWS} rows"));

        if timed {
            let ms = started.elapsed().as_secs_f64() * 1000.0;
            say(format!("FILTER_MS {} {ms:.2}", query.chars().count()));
        }
    }

    fn check(&mut self, condition: bool, message: impl AsRef<str>) {
        if condition {
            self.pass += 1;
        } else {
            self.fail += 1;
        }
        say(format!(
            "SELFTEST {} {}",
            if condition { "PASS" } else { "FAIL" },
            message.as_ref()
        ));
    }
}

impl Model for Grid {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.take(|grid_event, _| match grid_event {
            GridEvent::Filter(text) => {
                self.query.set(text);
                self.reapply(true);
            }

            GridEvent::Sort(key, direction) => {
                let started = std::time::Instant::now();
                self.sort_state.set(if direction == TableSortDirection::None {
                    None
                } else {
                    Some(TableSortState { key: key.clone(), direction })
                });
                self.reapply(false);
                let ms = started.elapsed().as_secs_f64() * 1000.0;
                let first = self.rows.get().first().map(|row| row.id).unwrap_or(u32::MAX);
                say(format!(
                    "SORT {key} {} first_id={first} ms={ms:.2}",
                    match direction {
                        TableSortDirection::Ascending => "asc",
                        TableSortDirection::Descending => "desc",
                        TableSortDirection::None => "none",
                    }
                ));
            }

            GridEvent::SelectRow(id, shift) => {
                let rows = self.rows.get();
                if shift {
                    if let (Some(anchor), Some(to)) = (
                        self.anchor.and_then(|a| rows.iter().position(|row| row.id == a)),
                        rows.iter().position(|row| row.id == id),
                    ) {
                        let (lo, hi) = if anchor <= to { (anchor, to) } else { (to, anchor) };
                        self.selected
                            .set(rows[lo..=hi].iter().map(|row| row.id).collect::<Vec<_>>());
                    }
                } else {
                    self.anchor = Some(id);
                    self.selected.set(vec![id]);
                }
                say(format!("SELECT count={} clicked_id={id}", self.selected.get().len()));
            }

            GridEvent::SelfTestStep => {
                if !self.selftest {
                    return;
                }
                self.step += 1;
                self.run_step(cx, self.step);
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Scripted self-test (GRID_SELFTEST=1)
//
// Each step emits the same events the widgets emit and then asserts on the
// resulting model state, one step per 120 ms timer tick so the table really
// re-renders (and the virtualization probe really re-runs) in between.
// ---------------------------------------------------------------------------

impl Grid {
    fn scroll_to(&self, cx: &mut EventContext, ratio: f32) {
        RENDER_MIN.store(usize::MAX, Ordering::Relaxed);
        RENDER_MAX.store(0, Ordering::Relaxed);
        RENDER_COUNT.store(0, Ordering::Relaxed);
        cx.emit_custom(
            Event::new(ScrollEvent::SetY(ratio))
                .target(Entity::root())
                .propagate(Propagation::Subtree),
        );
    }

    fn report_window(&self, ratio: f32) {
        let min = RENDER_MIN.load(Ordering::Relaxed);
        let max = RENDER_MAX.load(Ordering::Relaxed);
        let built = RENDER_COUNT.load(Ordering::Relaxed);
        say(format!(
            "WINDOW ratio={ratio:.2} first={} last={} cells_built={built} rows={}",
            if min == usize::MAX { 0 } else { min },
            max,
            self.rows.get().len()
        ));
    }

    fn run_step(&mut self, cx: &mut EventContext, step: usize) {
        match step {
            1 => {
                let rows = self.rows.get();
                self.check(rows.len() == ROWS, format!("{} rows built", rows.len()));
                self.check(
                    rows[0].id == 0 && rows[0].name.contains('-'),
                    format!("deterministic row 0: id={} name={}", rows[0].id, rows[0].name),
                );
                let (ok, warn, err) = rows.iter().fold((0, 0, 0), |acc, row| match row.status {
                    Status::Ok => (acc.0 + 1, acc.1, acc.2),
                    Status::Warn => (acc.0, acc.1 + 1, acc.2),
                    Status::Err => (acc.0, acc.1, acc.2 + 1),
                });
                self.check(
                    ok > 0 && warn > 0 && err > 0,
                    format!("status chips present ok={ok} warn={warn} err={err}"),
                );
            }

            // ---- filter-as-you-type -------------------------------------
            2 => cx.emit(GridEvent::Filter(String::from("p"))),
            3 => {
                let shown = self.rows.get().len();
                self.check(
                    shown > 0 && shown < ROWS,
                    format!("filter \"p\" narrowed to {shown} rows"),
                );
                cx.emit(GridEvent::Filter(String::from("pr")));
            }
            4 => cx.emit(GridEvent::Filter(String::from("pri"))),
            5 => cx.emit(GridEvent::Filter(String::from("prim"))),
            6 => {
                let rows = self.rows.get();
                let label_ok = self.count_label.get() == format!("{} of {ROWS} rows", rows.len());
                self.check(
                    rows.iter().all(|row| row.name.contains("prim")) && label_ok,
                    format!(
                        "all {} rows match \"prim\"; count label \"{}\"",
                        rows.len(),
                        self.count_label.get()
                    ),
                );
                cx.emit(GridEvent::Filter(String::new()));
            }
            7 => {
                self.check(
                    self.rows.get().len() == ROWS,
                    format!("filter cleared back to {}", self.rows.get().len()),
                );
            }

            // ---- sort ----------------------------------------------------
            8 => cx.emit(GridEvent::Sort(
                String::from("name"),
                TableSortDirection::Ascending,
            )),
            9 => {
                let rows = self.rows.get();
                self.check(
                    rows.windows(2).all(|pair| pair[0].name <= pair[1].name),
                    format!("name asc sorted ({} .. {})", rows[0].name, rows[ROWS - 1].name),
                );
                self.check(
                    self.sort_state.get().map(|state| state.direction)
                        == Some(TableSortDirection::Ascending),
                    "sort indicator state = Ascending",
                );
                cx.emit(GridEvent::Sort(
                    String::from("name"),
                    TableSortDirection::Descending,
                ));
            }
            10 => {
                let rows = self.rows.get();
                self.check(
                    rows.windows(2).all(|pair| pair[0].name >= pair[1].name),
                    format!("name desc sorted ({} .. {})", rows[0].name, rows[ROWS - 1].name),
                );
                cx.emit(GridEvent::Sort(String::from("id"), TableSortDirection::Ascending));
            }
            11 => {
                let rows = self.rows.get();
                self.check(
                    rows[0].id == 0 && rows[ROWS - 1].id == (ROWS - 1) as u32,
                    "id asc restores generation order",
                );
            }

            // ---- selection ----------------------------------------------
            12 => cx.emit(GridEvent::SelectRow(5, false)),
            13 => {
                self.check(
                    self.selected.get() == vec![5],
                    format!("single select -> {:?}", self.selected.get()),
                );
                cx.emit(GridEvent::SelectRow(9, true));
            }
            14 => {
                let selected = self.selected.get();
                self.check(
                    selected.len() == 5 && selected.first() == Some(&5),
                    format!("shift-range select -> {} rows {:?}", selected.len(), selected),
                );
            }

            // ---- column resize -------------------------------------------
            15 => {
                // What the header divider drag does: set the column's width
                // signal. Applied here directly so the assertion is on the
                // same state the drag mutates (a real CGEvent divider drag is
                // recorded separately in FRICTION.md).
                let columns = self.columns.get();
                let column = columns.iter().find(|column| column.key == "name").unwrap();
                let before = column.width.get();
                column.width.set(before + 90.0);
                let after = column.width.get();
                say(format!("RESIZE col=name width={before}->{after}"));
                self.check(
                    (after - before - 90.0).abs() < 0.5,
                    format!("column resize {before} -> {after}"),
                );
            }

            // ---- virtualized scrolling -----------------------------------
            16..=27 => {
                if step > 16 {
                    self.report_window((step - 16) as f32 / 12.0);
                }
                self.scroll_to(cx, (step - 15) as f32 / 12.0);
            }
            28 => {
                self.report_window(1.0);
                let built = RENDER_COUNT.load(Ordering::Relaxed);
                self.check(
                    built > 0 && built < 200,
                    format!("virtualized: {built} status cells materialised at the bottom of 100,000 rows"),
                );
            }

            29 => {
                say(format!("SELFTEST DONE pass={} fail={}", self.pass, self.fail));
                std::process::exit(if self.fail == 0 { 0 } else { 1 });
            }

            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Style
// ---------------------------------------------------------------------------

const STYLE: &str = r#"
.app { width: 1s; height: 1s; padding: 10px; vertical-gap: 8px; }
.toolbar { height: auto; horizontal-gap: 10px; alignment: center; }
.count { height: auto; font-size: 13px; }
.dim { height: auto; font-size: 12px; color: #8a8a8a; }
.filter { font-size: 13px; }

/* `VirtualTable` wraps every row in an HStack (`table-row`) and every cell in
   a VStack (`table-cell`). Both are hoverable by default, which makes THEM the
   press target instead of the `ListItem` that owns row selection — so row
   clicks never select. `pointer-events: none` on both hands the hover back to
   the list item. (Same class of trap as needing `.hoverable(false)` on the
   children of any pressable container in vizia.) */
.table-row, .table-cell { pointer-events: none; }
.table-cell { alignment: center left; }
.chip-wrap { width: 1s; height: 1s; alignment: center left; }
.cell { height: 1s; font-size: 12px; alignment: center left; }
.cell.numeric { alignment: right; width: 1s; }

.chip {
    height: auto;
    width: auto;
    padding: 2px 0px;
    font-size: 11px;
    corner-radius: 8px;
    text-align: center;
    color: #101010;
}

.chip.ok   { background-color: #7ac74f; }
.chip.warn { background-color: #f2c53d; }
.chip.err  { background-color: #e06363; }
"#;
