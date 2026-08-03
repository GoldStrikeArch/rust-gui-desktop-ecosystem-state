//! "Grid" — 100k-row data table (SPEC-7), floem git @ 778bb5f2.
//!
//! Architecture notes (research-relevant):
//! - floem has NO table widget; the grid is composed from taffy flex rows.
//!   It DOES have real virtualization: `VirtualStack` (understory_virtual_list)
//!   materializes only the on-screen row views, so the 100k-row body is a
//!   single built-in view — no hand-rolled spacer-window machinery (the iced
//!   port hand-rolled exactly that).
//! - Sorting/filtering operate on `visible: RwSignal<Vec<u32>>` (indices into
//!   the immutable leaked `rows`), recomputed synchronously and self-timed
//!   (`FILTER_MS <query_len> <ms>` on stdout, per the spec).
//! - Column resize is a hand-rolled pointer-capture drag on a 7 px divider
//!   strip: `cx.request_pointer_capture` keeps `PointerMove` flowing to the
//!   divider view outside its bounds; widths live in a signal read by every
//!   cell's (reactive) style closure.
//! - Row click selection reads `PointerDown`'s modifier state directly
//!   (shift = range from anchor, cmd/ctrl = toggle) — no global modifier
//!   subscription needed (iced needed one).
//! - `GRID_SELFTEST=1` runs a scripted pass through the SAME closures the UI
//!   events call (filter, sort, click, resize math, programmatic scroll) and
//!   prints `SORT`/`SELECT`/`RESIZE`/`WINDOW` evidence lines + a final
//!   `SELFTEST DONE pass=N fail=M`, then exits.

use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use floem::Application;
use floem::kurbo::{Point, Size};
use floem::prelude::*;
use floem::style::CursorStyle;
use floem::views::VirtualVector;
use floem::views::scroll::ScrollChanged;
use floem::window::WindowConfig;

const N_ROWS: usize = 100_000;
const ROW_H: f64 = 26.0;
const HEADER_H: f64 = 30.0;
const MIN_COL_W: f64 = 40.0;
const MAX_COL_W: f64 = 480.0;
const DIVIDER_W: f64 = 7.0;

const COLUMNS: [&str; 6] = ["id", "name", "category", "value", "date", "status"];

const CATEGORIES: [&str; 8] = [
    "alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel",
];
const ADJECTIVES: [&str; 16] = [
    "brisk", "calm", "dusty", "eager", "fuzzy", "grand", "happy", "icy", "jolly", "keen", "lucid",
    "misty", "noble", "odd", "prime", "quiet",
];
const NOUNS: [&str; 16] = [
    "falcon", "otter", "maple", "comet", "river", "ember", "stone", "cloud", "harbor", "meadow",
    "signal", "circuit", "lantern", "orchid", "summit", "walnut",
];

const BG_HEADER: Color = Color::from_rgb8(0xe4, 0xe4, 0xea);
const BG_DIVIDER: Color = Color::from_rgb8(0xc9, 0xc9, 0xd2);
const BG_ZEBRA: Color = Color::from_rgb8(0xf4, 0xf4, 0xf6);
const BG_SELECTED: Color = Color::from_rgb8(0xbf, 0xd3, 0xf5);
const TEXT_DIM: Color = Color::from_rgb8(0x70, 0x70, 0x7a);

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
    fn label(self) -> &'static str {
        match self {
            Status::Ok => "Ok",
            Status::Warn => "Warn",
            Status::Err => "Err",
        }
    }

    fn rank(self) -> u8 {
        match self {
            Status::Ok => 0,
            Status::Warn => 1,
            Status::Err => 2,
        }
    }

    fn colors(self) -> (Color, Color) {
        match self {
            Status::Ok => (Color::from_rgb8(0x1d, 0x7a, 0x33), Color::WHITE),
            Status::Warn => (Color::from_rgb8(0xc9, 0x8a, 0x0b), Color::BLACK),
            Status::Err => (Color::from_rgb8(0xc2, 0x33, 0x2e), Color::WHITE),
        }
    }
}

struct Row {
    id: u32,
    name: String,
    category: &'static str,
    value: f64,
    date: String,
    status: Status,
}

/// Tiny xorshift* PRNG — deterministic data without pulling in `rand`.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

fn generate_rows() -> Vec<Row> {
    let mut rng = Rng(0xDEAD_BEEF_CAFE_F00D);

    (0..N_ROWS as u32)
        .map(|id| {
            let r = rng.next();
            let adjective = ADJECTIVES[(r >> 8) as usize % ADJECTIVES.len()];
            let noun = NOUNS[(r >> 16) as usize % NOUNS.len()];
            let number = (r >> 24) % 10_000;

            let value = (rng.next() % 100_000_000) as f64 / 100.0;

            let d = rng.next();
            let (year, month, day) = (2019 + d % 7, 1 + (d >> 16) % 12, 1 + (d >> 32) % 28);

            let status = match rng.next() % 10 {
                0..=6 => Status::Ok,
                7..=8 => Status::Warn,
                _ => Status::Err,
            };

            Row {
                id,
                name: format!("{adjective}-{noun}-{number:04}"),
                category: CATEGORIES[(r >> 40) as usize % CATEGORIES.len()],
                value,
                date: format!("{year:04}-{month:02}-{day:02}"),
                status,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Model (Copy — signals + a leaked &'static row store)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum SortDir {
    Asc,
    Desc,
}

#[derive(Clone, Copy)]
struct Grid {
    rows: &'static [Row],
    visible: RwSignal<Vec<u32>>,
    query: RwSignal<String>,
    sort: RwSignal<Option<(usize, SortDir)>>,
    widths: RwSignal<[f64; 6]>,
    selected: RwSignal<HashSet<u32>>,
    anchor: RwSignal<Option<usize>>,
    /// (column, start width, pointer-capture start x) of a divider drag.
    resize: RwSignal<Option<(usize, f64, f64)>>,
    scroll_y: RwSignal<f64>,
    traced_first: RwSignal<usize>,
    scroll_target: RwSignal<Option<Point>>,
    selftest: bool,
}

impl Grid {
    fn new() -> Self {
        let start = Instant::now();
        let rows: &'static [Row] = Box::leak(generate_rows().into_boxed_slice());
        let visible: Vec<u32> = (0..rows.len() as u32).collect();
        println!("BUILD_MS {:.2}", start.elapsed().as_secs_f64() * 1000.0);

        Self {
            rows,
            visible: RwSignal::new(visible),
            query: RwSignal::new(String::new()),
            sort: RwSignal::new(None),
            widths: RwSignal::new([70.0, 220.0, 110.0, 130.0, 130.0, 90.0]),
            selected: RwSignal::new(HashSet::new()),
            anchor: RwSignal::new(None),
            resize: RwSignal::new(None),
            scroll_y: RwSignal::new(0.0),
            traced_first: RwSignal::new(0),
            scroll_target: RwSignal::new(None),
            selftest: std::env::var_os("GRID_SELFTEST").is_some(),
        }
    }

    /// Recompute `visible` from query + sort. Self-timed per the spec.
    fn refresh(&self) {
        let start = Instant::now();
        let query = self.query.get_untracked().to_lowercase();

        let mut visible: Vec<u32> = (0..self.rows.len() as u32)
            .filter(|&i| query.is_empty() || self.rows[i as usize].name.contains(&query))
            .collect();
        self.sort_indices(&mut visible);
        self.visible.set(visible);

        println!(
            "FILTER_MS {} {:.2}",
            query.chars().count(),
            start.elapsed().as_secs_f64() * 1000.0
        );

        self.anchor.set(None);
        self.scroll_target.set(Some(Point::ZERO));
    }

    fn sort_indices(&self, indices: &mut [u32]) {
        let Some((column, direction)) = self.sort.get_untracked() else {
            return;
        };
        let rows = self.rows;
        match column {
            0 => indices.sort_unstable_by_key(|&i| rows[i as usize].id),
            1 => indices.sort_unstable_by(|&a, &b| rows[a as usize].name.cmp(&rows[b as usize].name)),
            2 => indices.sort_unstable_by(|&a, &b| {
                rows[a as usize]
                    .category
                    .cmp(rows[b as usize].category)
                    .then(rows[a as usize].id.cmp(&rows[b as usize].id))
            }),
            3 => indices
                .sort_unstable_by(|&a, &b| rows[a as usize].value.total_cmp(&rows[b as usize].value)),
            4 => indices.sort_unstable_by(|&a, &b| {
                rows[a as usize]
                    .date
                    .cmp(&rows[b as usize].date)
                    .then(rows[a as usize].id.cmp(&rows[b as usize].id))
            }),
            _ => indices.sort_unstable_by(|&a, &b| {
                rows[a as usize]
                    .status
                    .rank()
                    .cmp(&rows[b as usize].status.rank())
                    .then(rows[a as usize].id.cmp(&rows[b as usize].id))
            }),
        }
        if direction == SortDir::Desc {
            indices.reverse();
        }
    }

    fn toggle_sort(&self, column: usize) {
        let direction = match self.sort.get_untracked() {
            Some((c, SortDir::Asc)) if c == column => SortDir::Desc,
            _ => SortDir::Asc,
        };
        self.sort.set(Some((column, direction)));

        let start = Instant::now();
        self.visible.update(|v| {
            // update() gives &mut in place; sort_indices reads self.sort.
            let mut taken = std::mem::take(v);
            self.sort_indices(&mut taken);
            *v = taken;
        });
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        self.anchor.set(None);

        if self.selftest {
            println!(
                "SORT {} {} first_id={} ms={elapsed_ms:.2}",
                COLUMNS[column],
                if direction == SortDir::Asc { "asc" } else { "desc" },
                self.visible
                    .with_untracked(|v| v.first().map_or(0, |&i| self.rows[i as usize].id)),
            );
        }
    }

    /// Click selection: plain = single, shift = range from anchor,
    /// cmd/ctrl = toggle. `position` is the index in `visible`.
    fn row_clicked(&self, position: usize, shift: bool, command: bool) {
        let Some(index) = self.visible.with_untracked(|v| v.get(position).copied()) else {
            return;
        };

        if shift && let Some(anchor) = self.anchor.get_untracked() {
            let (from, to) = if anchor <= position { (anchor, position) } else { (position, anchor) };
            let range: HashSet<u32> =
                self.visible.with_untracked(|v| v[from..=to].iter().copied().collect());
            self.selected.set(range);
        } else if command {
            self.selected.update(|s| {
                if !s.remove(&index) {
                    s.insert(index);
                }
            });
            self.anchor.set(Some(position));
        } else {
            self.selected.set(HashSet::from([index]));
            self.anchor.set(Some(position));
        }

        if self.selftest {
            println!(
                "SELECT count={} clicked_id={}",
                self.selected.with_untracked(|s| s.len()),
                self.rows[index as usize].id
            );
        }
    }

    fn resize_moved(&self, x: f64) {
        if let Some((column, start_width, origin)) = self.resize.get_untracked() {
            self.widths.update(|w| {
                w[column] = (start_width + x - origin).clamp(MIN_COL_W, MAX_COL_W);
            });
        }
    }

    fn resize_released(&self) {
        if let Some((column, _, _)) = self.resize.get_untracked() {
            self.resize.set(None);
            if self.selftest {
                println!(
                    "RESIZE col={} width={:.0}",
                    COLUMNS[column],
                    self.widths.get_untracked()[column]
                );
            }
        }
    }

    fn scrolled(&self, y: f64) {
        self.scroll_y.set(y);
        if self.selftest {
            let first = (y / ROW_H) as usize;
            if first != self.traced_first.get_untracked() {
                self.traced_first.set(first);
                println!("WINDOW first={first} y={y:.0}");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Views
// ---------------------------------------------------------------------------

fn main() {
    Application::new()
        .window(
            |_| app_view(),
            Some(
                WindowConfig::default()
                    .title("Grid (floem)")
                    .size(Size::new(1000.0, 640.0)),
            ),
        )
        .run();
}

fn app_view() -> impl IntoView {
    let grid = Grid::new();

    // Filter-as-you-type: the TextInput writes `query`; this effect re-runs
    // on every change (skipping the initial run).
    floem::reactive::Effect::new(move |prev| {
        grid.query.track();
        if prev.is_some() {
            grid.refresh();
        }
    });

    if grid.selftest {
        selftest(grid);
    }

    let controls = Stack::horizontal((
        TextInput::new(grid.query)
            .placeholder("filter by name…")
            .style(|s| s.width(300.0).padding(8.0)),
        Label::derived(move || format!("{} of 100,000 rows", grid.visible.with(|v| v.len())))
            .style(|s| s.font_size(14.0)),
        Empty::new().style(|s| s.flex_grow(1.0)),
        Label::new("click header: sort · drag divider: resize · shift-click: range")
            .style(|s| s.font_size(12.0).color(TEXT_DIM)),
    ))
    .style(|s| s.gap(14.0).items_center().width_full());

    let body = VirtualStack::full(
        move || grid.visible.enumerate(),
        // Key includes the row index so re-sorts rebuild the (few) live rows.
        |(position, index)| (*position, *index),
        move |(position, index)| row_view(grid, position, index),
    )
    .item_size_fixed(|| ROW_H)
    .style(|s| s.flex_col().width_full())
    .scroll()
    .on_event_cont(ScrollChanged::listener(), move |_, changed| {
        grid.scrolled(changed.offset.y);
    })
    .scroll_to(move || grid.scroll_target.get())
    // min_height(0) is LOAD-BEARING: without it taffy sizes the scroll to
    // its (2.6M px) min-content height, the clip never applies, and the
    // VirtualStack sees "viewport == content" — materializing ALL 100k rows
    // (16 GiB RSS, minutes of shaping). See FRICTION.md.
    .style(|s| s.width_full().flex_grow(1.0).min_height(0.0));

    let table = Stack::vertical((header_view(grid), body)).style(|s| {
        s.flex_col()
            .width_full()
            .flex_grow(1.0)
            .min_height(0.0)
            .border(1.0)
            .border_color(BG_DIVIDER)
            .border_radius(6.0)
    });

    Stack::vertical((controls, table))
        .style(|s| s.flex_col().gap(10.0).padding(10.0).size_full())
}

fn header_view(grid: Grid) -> impl IntoView {
    let cells = COLUMNS
        .iter()
        .enumerate()
        .map(|(i, &name)| {
            let header = Label::derived(move || {
                let indicator = match grid.sort.get() {
                    Some((c, SortDir::Asc)) if c == i => " ▲",
                    Some((c, SortDir::Desc)) if c == i => " ▼",
                    _ => "",
                };
                format!("{name}{indicator}")
            })
            .style(move |s| {
                s.font_size(13.0)
                    .width(grid.widths.get()[i])
                    .height(HEADER_H)
                    .padding(6.0)
                    .padding_horiz(8.0)
                    .background(BG_HEADER)
                    .cursor(CursorStyle::Pointer)
            })
            .on_event_stop(listener::Click, move |_, _| grid.toggle_sort(i));

            // Divider grab strip: pointer-capture drag resizes column `i`.
            let divider = Empty::new()
                .style(|s| {
                    s.width(DIVIDER_W)
                        .height(HEADER_H)
                        .background(BG_DIVIDER)
                        .cursor(CursorStyle::ColResize)
                })
                .on_event_stop(listener::PointerDown, move |cx, event| {
                    if let Some(pointer_id) = event.pointer.pointer_id {
                        cx.request_pointer_capture(pointer_id);
                    }
                    grid.resize.set(Some((
                        i,
                        grid.widths.get_untracked()[i],
                        event.state.logical_point().x,
                    )));
                })
                .on_event_stop(listener::PointerMove, move |_, update| {
                    grid.resize_moved(update.current.logical_point().x);
                })
                .on_event_stop(listener::PointerUp, move |_, _| grid.resize_released());

            Stack::horizontal((header, divider)).into_any()
        })
        .collect::<Vec<_>>();

    Stack::horizontal_from_iter(cells)
}

fn row_view(grid: Grid, position: usize, index: u32) -> impl IntoView {

    let data = &grid.rows[index as usize];
    let (chip_bg, chip_fg) = data.status.colors();

    let cell = move |content: floem::AnyView, col: usize| {
        content
            .style(move |s| {
                s.width(grid.widths.get()[col] + DIVIDER_W)
                    .height(ROW_H)
                    .padding_horiz(8.0)
                    .padding_vert(4.0)
                    .font_size(13.0)
            })
            .clip()
    };

    let value_cell = Label::new(format!("{:.2}", data.value))
        .style(|s| s.font_size(13.0))
        .container()
        .style(move |s| {
            s.width(grid.widths.get()[3] + DIVIDER_W)
                .height(ROW_H)
                .padding_horiz(8.0)
                .padding_vert(4.0)
                .justify_end()
        })
        .clip();

    let chip = Label::new(data.status.label())
        .style(move |s| {
            s.font_size(11.0)
                .color(chip_fg)
                .background(chip_bg)
                .padding_horiz(8.0)
                .padding_vert(1.0)
                .border_radius(9.0)
        })
        .container()
        .style(move |s| {
            s.width(grid.widths.get()[5] + DIVIDER_W)
                .height(ROW_H)
                .padding_horiz(8.0)
                .items_center()
        });

    Stack::horizontal((
        cell(Label::new(data.id.to_string()).into_any(), 0),
        cell(Label::new(data.name.clone()).into_any(), 1),
        cell(Label::new(data.category).into_any(), 2),
        value_cell,
        cell(Label::new(data.date.clone()).into_any(), 4),
        chip,
    ))
    .style(move |s| {
        let selected = grid.selected.with(|sel| sel.contains(&index));
        let zebra = position % 2 == 1;
        s.height(ROW_H).width_full().apply_if(selected, |s| s.background(BG_SELECTED)).apply_if(
            !selected && zebra,
            |s| s.background(BG_ZEBRA),
        )
    })
    .on_event_stop(listener::PointerDown, move |_, event| {
        let modifiers = event.state.modifiers;
        grid.row_clicked(position, modifiers.shift(), modifiers.meta() || modifiers.ctrl());
    })
}

// ---------------------------------------------------------------------------
// Scripted self-test (GRID_SELFTEST=1): 14 checks through the UI closures.
// ---------------------------------------------------------------------------

static PASS: AtomicUsize = AtomicUsize::new(0);
static FAIL: AtomicUsize = AtomicUsize::new(0);

fn check(name: &str, ok: bool) {
    if ok {
        PASS.fetch_add(1, Ordering::Relaxed);
    } else {
        FAIL.fetch_add(1, Ordering::Relaxed);
        println!("CHECK-FAIL {name}");
    }
}

fn selftest(grid: Grid) {
    fn after(ms: u64, f: impl FnOnce() + 'static) {
        floem::action::exec_after(Duration::from_millis(ms), move |_| f());
    }

    check("build 100k rows", grid.rows.len() == N_ROWS);

    after(2_000, move || {
        // Filter typing: b → br → bri → bris (FILTER_MS per keystroke).
        for (t, q) in ["b", "br", "bri", "bris"].iter().enumerate() {
            after(200 * t as u64, move || grid.query.set(q.to_string()));
        }
        after(1_000, move || {
            let expected =
                grid.rows.iter().filter(|r| r.name.contains("bris")).count();
            let all_match = grid
                .visible
                .with_untracked(|v| v.iter().all(|&i| grid.rows[i as usize].name.contains("bris")));
            let count = grid.visible.with_untracked(|v| v.len());
            check("filter 'bris' matches only", all_match && count > 0);
            check("filter 'bris' count exact", count == expected);

            grid.query.set(String::new());
            after(300, move || {
                check(
                    "clear filter restores 100k",
                    grid.visible.with_untracked(|v| v.len()) == N_ROWS,
                );

                // Sorts: name asc, name desc, id asc.
                grid.toggle_sort(1);
                let asc_sorted = grid.visible.with_untracked(|v| {
                    v.windows(2)
                        .all(|w| grid.rows[w[0] as usize].name <= grid.rows[w[1] as usize].name)
                });
                check("sort name asc ordered", asc_sorted);

                grid.toggle_sort(1);
                let desc_sorted = grid.visible.with_untracked(|v| {
                    v.windows(2)
                        .all(|w| grid.rows[w[0] as usize].name >= grid.rows[w[1] as usize].name)
                });
                check("sort name desc ordered", desc_sorted);

                grid.toggle_sort(0);
                check(
                    "sort id asc first row id 0",
                    grid.visible.with_untracked(|v| v.first() == Some(&0)),
                );

                // Selection: plain, shift-range, plain again.
                grid.row_clicked(5, false, false);
                check("plain click selects 1", grid.selected.with_untracked(|s| s.len()) == 1);
                grid.row_clicked(8, true, false);
                check("shift-click range selects 4", grid.selected.with_untracked(|s| s.len()) == 4);
                grid.row_clicked(2, false, false);
                check("plain click resets to 1", grid.selected.with_untracked(|s| s.len()) == 1);

                // Column resize through the same math as the divider drag:
                // press at x=3, move to x=58 → +55 px on the id column.
                grid.resize.set(Some((0, grid.widths.get_untracked()[0], 3.0)));
                grid.resize_moved(58.0);
                grid.resize_released();
                check("resize id col to 125", grid.widths.get_untracked()[0] == 125.0);

                // Programmatic scrolls → ScrollChanged → WINDOW lines.
                grid.scroll_target.set(Some(Point::new(0.0, 800.0)));
                after(400, move || {
                    check("scroll to y=800 windows", grid.traced_first.get_untracked() == 30);
                    grid.scroll_target.set(Some(Point::new(0.0, 260_000.0)));
                    after(400, move || {
                        check(
                            "scroll to y=260000 windows",
                            grid.traced_first.get_untracked() == 10_000,
                        );
                        grid.scroll_target.set(Some(Point::ZERO));
                        after(400, move || {
                            check(
                                "scroll back to top",
                                grid.traced_first.get_untracked() == 0,
                            );
                            println!(
                                "SELFTEST DONE pass={} fail={}",
                                PASS.load(Ordering::Relaxed),
                                FAIL.load(Ordering::Relaxed)
                            );
                            use std::io::Write;
                            let _ = std::io::stdout().flush();
                            std::process::exit(
                                if FAIL.load(Ordering::Relaxed) == 0 { 0 } else { 1 },
                            );
                        });
                    });
                });
            });
        });
    });
}
