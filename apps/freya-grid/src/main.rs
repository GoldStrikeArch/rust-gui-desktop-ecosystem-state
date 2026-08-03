//! "Grid" — 100k-row data table (SPEC-7), Freya 0.4.
//!
//! Architecture notes (research-relevant):
//! - Freya ships **real virtualization**: `VirtualScrollView` takes a builder
//!   closure plus `length` + `item_size` and only calls the builder for rows in
//!   the viewport, so the 100k-row body is a single stock component (the iced
//!   port hand-rolled a spacer window for exactly this). Freya's `Table`
//!   component was read and rejected: it is a layout helper that wants one
//!   element per cell, with no virtualization.
//! - Sorting/filtering operate on `visible: State<Vec<u32>>` (indices into the
//!   immutable leaked `rows`), recomputed synchronously and self-timed
//!   (`FILTER_MS <query_len> <ms>` on stdout, per the spec).
//! - Column resize is a hand-rolled drag on a 7 px divider strip using
//!   `on_pointer_down` + `on_global_pointer_move` + `on_global_pointer_press`.
//! - Row-click modifiers: `PressEventData` carries no modifier state, so a
//!   root-level `on_global_key_down`/`on_global_key_up` pair mirrors the live
//!   `Modifiers` into a signal (iced needed the same workaround).
//! - `GRID_SELFTEST=1` runs a scripted pass through the SAME functions the UI
//!   events call (filter, sort, click, resize math, programmatic scroll) and
//!   prints `SORT`/`SELECT`/`RESIZE`/`WINDOW` evidence lines plus a final
//!   `SELFTEST DONE pass=N fail=M`, then exits.

use std::{
    cell::Cell,
    collections::HashSet,
    rc::Rc,
    time::{
        Duration,
        Instant,
    },
};

use async_io::Timer;
use freya::prelude::*;

const N_ROWS: usize = 100_000;
const ROW_H: f32 = 26.0;
const MIN_COL_W: f32 = 40.0;
const MAX_COL_W: f32 = 480.0;
const DIVIDER_W: f32 = 7.0;

const COLUMNS: [&str; 6] = ["id", "name", "category", "value", "date", "status"];
const DEFAULT_WIDTHS: [f32; 6] = [70.0, 220.0, 110.0, 130.0, 130.0, 90.0];

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

const BG: Color = Color::from_argb(255, 252, 252, 253);
const HEADER_BG: Color = Color::from_argb(255, 240, 241, 244);
const ROW_ALT: Color = Color::from_argb(255, 247, 248, 250);
const SELECTED_BG: Color = Color::from_argb(255, 210, 226, 255);
const TEXT: Color = Color::from_argb(255, 26, 28, 33);
const MUTED: Color = Color::from_argb(255, 108, 115, 128);
const GRID_LINE: Color = Color::from_argb(255, 223, 226, 231);

// ---------------------------------------------------------------- data

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
            Status::Ok => (
                Color::from_argb(255, 219, 244, 226),
                Color::from_argb(255, 22, 105, 54),
            ),
            Status::Warn => (
                Color::from_argb(255, 253, 240, 205),
                Color::from_argb(255, 133, 90, 6),
            ),
            Status::Err => (
                Color::from_argb(255, 253, 224, 224),
                Color::from_argb(255, 152, 32, 32),
            ),
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
/// Identical generator and draw order to the other ports of SPEC-7, so the
/// dataset (and therefore every `first_id=` in the self-test) matches.
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum SortDir {
    Asc,
    Desc,
}

// ---------------------------------------------------------------- model

#[derive(Clone, Copy)]
struct Grid {
    rows: &'static [Row],
    visible: State<Vec<u32>>,
    query: State<String>,
    sort: State<Option<(usize, SortDir)>>,
    widths: State<[f32; 6]>,
    selected: State<HashSet<u32>>,
    anchor: State<Option<usize>>,
    resize: State<Option<(usize, f32, f32)>>,
    modifiers: State<Modifiers>,
    selftest: bool,
}

impl Grid {
    fn sort_indices(&self, indices: &mut [u32]) {
        let Some((column, direction)) = *self.sort.peek() else {
            return;
        };
        let rows = self.rows;
        match column {
            0 => indices.sort_unstable_by_key(|&i| rows[i as usize].id),
            1 => indices
                .sort_unstable_by(|&a, &b| rows[a as usize].name.cmp(&rows[b as usize].name)),
            2 => indices.sort_unstable_by(|&a, &b| {
                rows[a as usize]
                    .category
                    .cmp(rows[b as usize].category)
                    .then(rows[a as usize].id.cmp(&rows[b as usize].id))
            }),
            3 => indices.sort_unstable_by(|&a, &b| {
                rows[a as usize].value.total_cmp(&rows[b as usize].value)
            }),
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

    /// Recompute `visible` from query + sort. Self-timed per the spec.
    fn refresh(&self) {
        let mut this = *self;
        let start = Instant::now();
        let query = this.query.peek().to_lowercase();

        let mut visible: Vec<u32> = (0..this.rows.len() as u32)
            .filter(|&i| query.is_empty() || this.rows[i as usize].name.contains(&query))
            .collect();
        this.sort_indices(&mut visible);
        this.visible.set(visible);

        println!(
            "FILTER_MS {} {:.2}",
            query.chars().count(),
            start.elapsed().as_secs_f64() * 1000.0
        );

        this.anchor.set(None);
    }

    fn toggle_sort(&self, column: usize) {
        let mut this = *self;
        let direction = match *this.sort.peek() {
            Some((c, SortDir::Asc)) if c == column => SortDir::Desc,
            _ => SortDir::Asc,
        };
        this.sort.set(Some((column, direction)));

        let start = Instant::now();
        let mut taken = std::mem::take(&mut *this.visible.write());
        this.sort_indices(&mut taken);
        let first_id = taken.first().map_or(0, |&i| this.rows[i as usize].id);
        this.visible.set(taken);
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        this.anchor.set(None);

        if this.selftest {
            println!(
                "SORT {} {} first_id={first_id} ms={elapsed_ms:.2}",
                COLUMNS[column],
                if direction == SortDir::Asc { "asc" } else { "desc" },
            );
        }
    }

    /// Click selection: plain = single, shift = range from anchor,
    /// cmd/ctrl = toggle. `position` is the index in `visible`.
    fn row_clicked(&self, position: usize, shift: bool, command: bool) {
        let mut this = *self;
        let Some(index) = this.visible.peek().get(position).copied() else {
            return;
        };
        let anchor = *this.anchor.peek();

        if shift && anchor.is_some() {
            let anchor = anchor.unwrap();
            let (from, to) = if anchor <= position {
                (anchor, position)
            } else {
                (position, anchor)
            };
            let range: HashSet<u32> = this.visible.peek()[from..=to].iter().copied().collect();
            this.selected.set(range);
        } else if command {
            let mut set = this.selected.peek().clone();
            if !set.remove(&index) {
                set.insert(index);
            }
            this.selected.set(set);
            this.anchor.set(Some(position));
        } else {
            this.selected.set(HashSet::from([index]));
            this.anchor.set(Some(position));
        }

        if this.selftest {
            println!(
                "SELECT count={} clicked_id={}",
                this.selected.peek().len(),
                this.rows[index as usize].id
            );
        }
    }

    fn resize_moved(&self, cursor_x: f32) {
        let mut this = *self;
        let Some((column, start_width, origin)) = *this.resize.peek() else {
            return;
        };
        let width = (start_width + (cursor_x - origin)).clamp(MIN_COL_W, MAX_COL_W);
        let mut widths = *this.widths.peek();
        widths[column] = width;
        this.widths.set(widths);
    }

    fn resize_released(&self) {
        let mut this = *self;
        let Some((column, ..)) = *this.resize.peek() else {
            return;
        };
        let width = this.widths.peek()[column];
        this.resize.set(None);
        if this.selftest {
            println!("RESIZE col={} width={width:.0}", COLUMNS[column]);
        }
    }
}

// ---------------------------------------------------------------- main

fn main() {
    launch(
        LaunchConfig::new().with_window(
            WindowConfig::new(app)
                .with_title("Grid (freya)")
                .with_size(1000.0, 640.0)
                .with_background(BG),
        ),
    )
}

fn app() -> impl IntoElement {
    let grid = use_hook(|| {
        let start = Instant::now();
        let rows: &'static [Row] = Box::leak(generate_rows().into_boxed_slice());
        let visible: Vec<u32> = (0..rows.len() as u32).collect();
        println!("BUILD_MS {:.2}", start.elapsed().as_secs_f64() * 1000.0);

        Grid {
            rows,
            visible: State::create(visible),
            query: State::create(String::new()),
            sort: State::create(None),
            widths: State::create(DEFAULT_WIDTHS),
            selected: State::create(HashSet::new()),
            anchor: State::create(None),
            resize: State::create(None),
            modifiers: State::create(Modifiers::empty()),
            selftest: std::env::var_os("GRID_SELFTEST").is_some(),
        }
    });

    let scroll = use_scroll_controller(ScrollConfig::default);
    // The lowest row index the virtual list actually built in the last frame.
    let traced_first = use_hook(|| Rc::new(Cell::new(usize::MAX)));

    // Filter-as-you-type: the `Input` writes `query`, this effect re-derives
    // `visible` and prints FILTER_MS. `first` skips the initial run so the
    // startup build is only reported once, by BUILD_MS.
    let first_run = use_hook(|| Rc::new(Cell::new(true)));
    use_side_effect({
        let first_run = first_run.clone();
        move || {
            let _subscribe = grid.query.read().len();
            if first_run.replace(false) {
                return;
            }
            grid.refresh();
        }
    });

    if grid.selftest {
        let traced = traced_first.clone();
        use_hook(move || {
            spawn(async move {
                selftest(grid, scroll, traced).await;
            });
        });
    }

    let widths = *grid.widths.read();
    let sort = *grid.sort.read();
    let visible_len = grid.visible.read().len();
    let selected_len = grid.selected.read().len();
    let total_w: f32 = widths.iter().sum::<f32>() + DIVIDER_W * 6.0;

    let rows = grid.rows;
    let visible = grid.visible;
    let selected = grid.selected;
    let traced = traced_first.clone();

    rect()
        .expanded()
        .content(Content::flex())
        .background(BG)
        .color(TEXT)
        .a11y_focusable(true)
        .a11y_auto_focus(true)
        // `PressEventData` carries no modifier state, so mirror the live
        // modifiers from global key events (same workaround the iced port needs).
        .on_global_key_down(move |e: Event<KeyboardEventData>| {
            let mut grid = grid;
            grid.modifiers.set(e.modifiers);
        })
        .on_global_key_up(move |e: Event<KeyboardEventData>| {
            let mut grid = grid;
            grid.modifiers.set(e.modifiers);
        })
        .on_global_pointer_move(move |e: Event<PointerEventData>| {
            if grid.resize.peek().is_some() {
                grid.resize_moved(e.global_location().x as f32);
            }
        })
        .on_global_pointer_press(move |_: Event<PointerEventData>| {
            if grid.resize.peek().is_some() {
                grid.resize_released();
            }
        })
        // ------------------------------------------------------- toolbar
        .child(
            rect()
                .horizontal()
                .height(Size::px(40.))
                .cross_align(Alignment::Center)
                .spacing(10.)
                .padding(Gaps::new_symmetric(0., 10.))
                .child(
                    Input::new(grid.query)
                        .compact()
                        .placeholder("filter by name…")
                        .width(Size::px(240.)),
                )
                .child(
                    label()
                        .text(format!("{visible_len} of {N_ROWS} rows"))
                        .font_size(12.)
                        .color(MUTED),
                )
                .child(
                    label()
                        .text(format!("{selected_len} selected"))
                        .font_size(12.)
                        .color(MUTED),
                )
                .child(
                    label()
                        .text("click header to sort · drag divider to resize · shift-click for a range")
                        .font_size(11.)
                        .color(MUTED),
                ),
        )
        // ------------------------------------------------------- header
        .child(
            rect()
                .horizontal()
                .width(Size::px(total_w))
                .height(Size::px(30.))
                .background(HEADER_BG)
                .children(
                    COLUMNS
                        .iter()
                        .enumerate()
                        .flat_map(|(index, name)| {
                            let indicator = match sort {
                                Some((c, SortDir::Asc)) if c == index => " ▲",
                                Some((c, SortDir::Desc)) if c == index => " ▼",
                                _ => "",
                            };
                            [
                                rect()
                                    .key(("h", index))
                                    .width(Size::px(widths[index]))
                                    .height(Size::fill())
                                    .cross_align(Alignment::Center)
                                    .main_align(Alignment::Center)
                                    .padding(Gaps::new_symmetric(0., 6.))
                                    .a11y_role(AccessibilityRole::ColumnHeader)
                                    .on_press(move |_| grid.toggle_sort(index))
                                    .child(
                                        label()
                                            .text(format!("{name}{indicator}"))
                                            .font_size(12.)
                                            .font_weight(FontWeight::BOLD)
                                            .max_lines(1)
                                            .color(TEXT),
                                    )
                                    .into(),
                                rect()
                                    .key(("d", index))
                                    .width(Size::px(DIVIDER_W))
                                    .height(Size::fill())
                                    .center()
                                    // No per-element cursor property on `rect`;
                                    // `Cursor::set` is the imperative escape hatch.
                                    .on_pointer_enter(move |_| Cursor::set(CursorIcon::ColResize))
                                    .on_pointer_leave(move |_| Cursor::set(CursorIcon::default()))
                                    .on_pointer_down(move |e: Event<PointerEventData>| {
                                        e.stop_propagation();
                                        let mut grid = grid;
                                        let start = grid.widths.peek()[index];
                                        grid.resize.set(Some((
                                            index,
                                            start,
                                            e.global_location().x as f32,
                                        )));
                                    })
                                    .child(
                                        rect()
                                            .width(Size::px(1.))
                                            .height(Size::fill())
                                            .background(GRID_LINE),
                                    )
                                    .into(),
                            ]
                        })
                        .collect::<Vec<Element>>(),
                ),
        )
        // ------------------------------------------------------- body
        .child(
            VirtualScrollView::new_controlled(
                move |position: usize, _: &()| {
                    traced.set(traced.get().min(position));
                    let Some(&index) = visible.peek().get(position) else {
                        return rect().height(Size::px(ROW_H)).into();
                    };
                    let row = &rows[index as usize];
                    let is_selected = selected.peek().contains(&index);
                    let widths = *grid.widths.peek();
                    let (chip_bg, chip_fg) = row.status.colors();

                    rect()
                        .key(index)
                        .horizontal()
                        .width(Size::px(total_w))
                        .height(Size::px(ROW_H))
                        .cross_align(Alignment::Center)
                        .background(if is_selected {
                            SELECTED_BG
                        } else if position % 2 == 1 {
                            ROW_ALT
                        } else {
                            Color::TRANSPARENT
                        })
                        .a11y_role(AccessibilityRole::Row)
                        .on_press(move |_| {
                            let modifiers = *grid.modifiers.peek();
                            grid.row_clicked(
                                position,
                                modifiers.contains(Modifiers::SHIFT),
                                modifiers.contains(Modifiers::META)
                                    || modifiers.contains(Modifiers::CONTROL),
                            );
                        })
                        .child(cell(widths[0], row.id.to_string(), TextAlign::Left))
                        .child(cell(widths[1], row.name.clone(), TextAlign::Left))
                        .child(cell(widths[2], row.category.to_string(), TextAlign::Left))
                        .child(cell(widths[3], format!("{:.2}", row.value), TextAlign::Right))
                        .child(cell(widths[4], row.date.clone(), TextAlign::Left))
                        // Custom cell rendering: a coloured status chip.
                        .child(
                            rect()
                                .width(Size::px(widths[5]))
                                .height(Size::fill())
                                .cross_align(Alignment::Center)
                                .main_align(Alignment::Center)
                                .child(
                                    rect()
                                        .background(chip_bg)
                                        .rounded_full()
                                        .padding(Gaps::new_symmetric(1., 8.))
                                        .child(
                                            label()
                                                .text(row.status.label())
                                                .font_size(11.)
                                                .color(chip_fg),
                                        ),
                                ),
                        )
                        .into()
                },
                scroll,
            )
            .length(visible_len)
            .item_size(ROW_H)
            .width(Size::fill())
            .height(Size::flex(1.)),
        )
}

fn cell(width: f32, text: String, align: TextAlign) -> Element {
    rect()
        .width(Size::px(width))
        .height(Size::fill())
        .main_align(Alignment::Center)
        .padding(Gaps::new_symmetric(0., 6.))
        .child(
            label()
                .text(text)
                .width(Size::fill())
                .font_size(12.)
                .max_lines(1)
                .text_align(align)
                .color(TEXT),
        )
        .into()
}

// ---------------------------------------------------------------- self-test

/// Scripted pass through the same functions the UI events call.
async fn selftest(grid: Grid, mut scroll: ScrollController, traced: Rc<Cell<usize>>) {
    let passed = Cell::new(0usize);
    let failed = Cell::new(0usize);
    let check = |name: &str, ok: bool| {
        if ok {
            passed.set(passed.get() + 1);
        } else {
            failed.set(failed.get() + 1);
            println!("SELFTEST FAIL {name}");
        }
    };
    let mut grid_mut = grid;

    Timer::after(Duration::from_millis(1200)).await;

    check("build 100k rows", grid.rows.len() == N_ROWS);

    // Filter typing: b → br → bri → bris (one FILTER_MS per keystroke).
    for query in ["b", "br", "bri", "bris"] {
        grid_mut.query.set(query.to_string());
        Timer::after(Duration::from_millis(120)).await;
    }
    Timer::after(Duration::from_millis(300)).await;

    let expected = grid.rows.iter().filter(|r| r.name.contains("bris")).count();
    let count = grid.visible.peek().len();
    let all_match = grid
        .visible
        .peek()
        .iter()
        .all(|&i| grid.rows[i as usize].name.contains("bris"));
    check("filter 'bris' matches only", all_match && count > 0);
    check("filter 'bris' count exact", count == expected);

    grid_mut.query.set(String::new());
    Timer::after(Duration::from_millis(300)).await;
    check(
        "clear filter restores 100k",
        grid.visible.peek().len() == N_ROWS,
    );

    // Sorts: name asc, name desc, id asc.
    grid.toggle_sort(1);
    check(
        "sort name asc ordered",
        grid.visible
            .peek()
            .windows(2)
            .all(|w| grid.rows[w[0] as usize].name <= grid.rows[w[1] as usize].name),
    );
    grid.toggle_sort(1);
    check(
        "sort name desc ordered",
        grid.visible
            .peek()
            .windows(2)
            .all(|w| grid.rows[w[0] as usize].name >= grid.rows[w[1] as usize].name),
    );
    grid.toggle_sort(0);
    check(
        "sort id asc first row id 0",
        grid.visible.peek().first() == Some(&0),
    );

    // Selection: plain, shift-range, plain again.
    grid.row_clicked(5, false, false);
    check("plain click selects 1", grid.selected.peek().len() == 1);
    grid.row_clicked(8, true, false);
    check(
        "shift-click range selects 4",
        grid.selected.peek().len() == 4,
    );
    grid.row_clicked(2, false, false);
    check("plain click resets to 1", grid.selected.peek().len() == 1);

    // Column resize through the same math as the divider drag:
    // press at x=3, move to x=58 → +55 px on the id column.
    grid_mut
        .resize
        .set(Some((0, grid.widths.peek()[0], 3.0)));
    grid.resize_moved(58.0);
    grid.resize_released();
    check("resize id col to 125", grid.widths.peek()[0] == 125.0);

    // Programmatic scrolls. `traced` is written by the virtual list's item
    // builder, so the WINDOW lines report rows that were really built.
    for (y, expect) in [(800i32, 30usize), (260_000, 10_000), (0, 0)] {
        traced.set(usize::MAX);
        scroll.scroll_to_y(-y);
        Timer::after(Duration::from_millis(450)).await;
        let first = traced.get();
        println!("WINDOW first={first} y={y}");
        check(&format!("scroll to y={y} windows"), first == expect);
    }

    println!("SELFTEST DONE pass={} fail={}", passed.get(), failed.get());
    std::process::exit(if failed.get() == 0 { 0 } else { 1 });
}
