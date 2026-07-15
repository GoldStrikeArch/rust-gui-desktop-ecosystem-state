//! "Grid" — 100k-row data table per apps/SPEC-7.md, built with xilem 0.4.0.
//!
//! * Virtualization: masonry 0.4's `VirtualScroll` widget through the stock
//!   `virtual_scroll` view — only the on-screen rows exist as widgets.
//! * Sort / filter / selection are plain model operations on index vectors;
//!   the visible rows are recomputed and the view diffed on each change.
//! * Row click (with shift/cmd modifiers) and column-resize drag handles are
//!   hand-rolled masonry widgets (see widgets.rs) — no stock view reports
//!   modifier state or drag deltas.
//! * Prints `BUILD_MS <ms>` once at startup and `FILTER_MS <query_len> <ms>`
//!   per filter application (model recompute time; the subsequent view
//!   rebuild/paint is not included).

mod widgets;

use std::collections::HashSet;
use std::time::Instant;

use xilem::core::one_of::Either;
use xilem::masonry::properties::types::{CrossAxisAlignment, Length};
use xilem::masonry::properties::{LineBreaking, Padding};
use xilem::style::Style as _;
use xilem::view::{
    flex_col, flex_row, label, sized_box, text_input, virtual_scroll, FlexExt as _, FlexSpacer,
};
use xilem::winit::error::EventLoopError;
use xilem::{Color, EventLoop, FontWeight, TextAlign, WidgetView, WindowOptions, Xilem};

use widgets::{
    drag_handle, row_frame, HandleEvent, RowEvent, ACCENT, BG_EVEN, BG_ODD, CHIP_ERR, CHIP_OK,
    CHIP_WARN, HEADER_BG, TEXT_DIM,
};

const N_ROWS: usize = 100_000;
const HANDLE_W: f64 = 9.0;
const CELL_HPAD: f64 = 8.0;
const TEXT_MAIN: Color = Color::from_rgb8(0xe6, 0xe9, 0xef);

const COL_NAMES: [&str; 6] = ["ID", "Name", "Category", "Value", "Date", "Status"];
const DEFAULT_WIDTHS: [f64; 6] = [80.0, 250.0, 130.0, 110.0, 120.0, 100.0];

const CATEGORIES: [&str; 8] = [
    "Alpha", "Bravo", "Charlie", "Delta", "Echo", "Foxtrot", "Golf", "Hotel",
];
const ADJECTIVES: [&str; 16] = [
    "amber", "brisk", "calm", "dusty", "eager", "fuzzy", "grand", "happy", "icy", "jolly", "keen",
    "lucid", "mellow", "noble", "odd", "prime",
];
const NOUNS: [&str; 16] = [
    "anchor", "beacon", "canyon", "delta", "ember", "falcon", "garnet", "harbor", "island",
    "jigsaw", "kestrel", "lantern", "meadow", "nebula", "orchid", "pylon",
];

/// xorshift64* — deterministic, seeded; avoids a `rand` dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

struct Row {
    id: u32,
    name: String,
    /// Cached lowercase name for the substring filter.
    name_lower: String,
    category: u8,
    value: f64,
    date: String,
    status: u8,
}

fn generate_rows() -> Vec<Row> {
    let mut rng = Rng(0x5EED_CAFE_F00D_0001);
    (0..N_ROWS as u32)
        .map(|id| {
            let name = format!(
                "{}-{}-{:04}",
                ADJECTIVES[rng.below(16) as usize],
                NOUNS[rng.below(16) as usize],
                rng.below(10_000)
            );
            let name_lower = name.to_lowercase();
            let category = rng.below(8) as u8;
            let value = (rng.below(1_000_000) as f64) / 100.0;
            let date = format!(
                "{:04}-{:02}-{:02}",
                2020 + rng.below(6),
                1 + rng.below(12),
                1 + rng.below(28)
            );
            // Weighted status: ~70% Ok, ~20% Warn, ~10% Err.
            let status = match rng.below(10) {
                0..=6 => 0,
                7 | 8 => 1,
                _ => 2,
            };
            Row {
                id,
                name,
                name_lower,
                category,
                value,
                date,
                status,
            }
        })
        .collect()
}

struct AppState {
    rows: Vec<Row>,
    filter: String,
    sort_col: usize,
    sort_desc: bool,
    /// All row indices in the current sort order.
    sorted: Vec<u32>,
    /// `sorted` restricted to rows matching the filter — what the table shows.
    visible: Vec<u32>,
    /// Selected row ids.
    selected: HashSet<u32>,
    /// Row id of the last plain click — the shift-range anchor.
    anchor: Option<u32>,
    widths: [f64; 6],
    /// (column, width at press) while a resize handle is dragged.
    resize: Option<(usize, f64)>,
}

impl AppState {
    fn new() -> Self {
        let rows = generate_rows();
        let sorted: Vec<u32> = (0..N_ROWS as u32).collect();
        let visible = sorted.clone();
        Self {
            rows,
            filter: String::new(),
            sort_col: 0,
            sort_desc: false,
            sorted,
            visible,
            selected: HashSet::new(),
            anchor: None,
            widths: DEFAULT_WIDTHS,
            resize: None,
        }
    }

    /// Re-sort `sorted` under the current sort spec. Ties break by id so the
    /// order is deterministic for every column.
    fn apply_sort(&mut self) {
        let rows = &self.rows;
        let desc = self.sort_desc;
        let col = self.sort_col;
        self.sorted.sort_unstable_by(|&a, &b| {
            let (ra, rb) = (&rows[a as usize], &rows[b as usize]);
            let ord = match col {
                0 => ra.id.cmp(&rb.id),
                1 => ra.name.cmp(&rb.name),
                2 => CATEGORIES[ra.category as usize].cmp(CATEGORIES[rb.category as usize]),
                3 => ra
                    .value
                    .partial_cmp(&rb.value)
                    .unwrap_or(std::cmp::Ordering::Equal),
                4 => ra.date.cmp(&rb.date),
                _ => ra.status.cmp(&rb.status),
            };
            let ord = ord.then(ra.id.cmp(&rb.id));
            if desc {
                ord.reverse()
            } else {
                ord
            }
        });
    }

    /// Recompute `visible` from `sorted` + `filter`; returns elapsed ms.
    fn recompute_visible(&mut self) -> f64 {
        let t0 = Instant::now();
        let needle = self.filter.trim().to_lowercase();
        self.visible = if needle.is_empty() {
            self.sorted.clone()
        } else {
            self.sorted
                .iter()
                .copied()
                .filter(|&i| self.rows[i as usize].name_lower.contains(needle.as_str()))
                .collect()
        };
        t0.elapsed().as_secs_f64() * 1000.0
    }

    fn set_filter(&mut self, value: String) {
        self.filter = value;
        let ms = self.recompute_visible();
        println!("FILTER_MS {} {:.3}", self.filter.trim().chars().count(), ms);
    }

    fn toggle_sort(&mut self, col: usize) {
        if self.sort_col == col {
            self.sort_desc = !self.sort_desc;
        } else {
            self.sort_col = col;
            self.sort_desc = false;
        }
        let t0 = Instant::now();
        self.apply_sort();
        let sort_ms = t0.elapsed().as_secs_f64() * 1000.0;
        self.recompute_visible();
        println!(
            "SORT_MS {} {} {:.3}",
            COL_NAMES[col],
            if self.sort_desc { "desc" } else { "asc" },
            sort_ms
        );
    }

    fn row_clicked(&mut self, pos: usize, shift: bool, cmd: bool) {
        if pos >= self.visible.len() {
            return;
        }
        let id = self.visible[pos];
        if shift {
            if let Some(anchor) = self.anchor {
                if let Some(apos) = self.visible.iter().position(|&x| x == anchor) {
                    let (lo, hi) = (apos.min(pos), apos.max(pos));
                    self.selected = self.visible[lo..=hi].iter().copied().collect();
                    return;
                }
            }
            self.selected.clear();
            self.selected.insert(id);
            self.anchor = Some(id);
        } else if cmd {
            if !self.selected.remove(&id) {
                self.selected.insert(id);
            }
            self.anchor = Some(id);
        } else {
            self.selected.clear();
            self.selected.insert(id);
            self.anchor = Some(id);
        }
    }

    fn handle_event(&mut self, col: usize, event: HandleEvent) {
        match event {
            HandleEvent::Pressed => self.resize = Some((col, self.widths[col])),
            HandleEvent::Dragged { dx } => {
                if let Some((c, base)) = self.resize {
                    if c == col {
                        self.widths[col] = (base + dx).clamp(60.0, 480.0);
                    }
                }
            }
            HandleEvent::Released => self.resize = None,
        }
    }
}

fn thousands(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// One fixed-width text cell.
fn cell(
    text: String,
    width: f64,
    align: TextAlign,
    color: Color,
) -> impl WidgetView<AppState> + use<> {
    sized_box(
        label(text)
            .text_size(13.0)
            .text_alignment(align)
            .prop(LineBreaking::Clip)
            .color(color),
    )
    .width(Length::px(width))
    .padding(Padding::horizontal(CELL_HPAD))
}

/// Status column cell: colored chip hugging its text.
fn chip_cell(text: &'static str, bg: Color, width: f64) -> impl WidgetView<AppState> + use<> {
    let chip = sized_box(
        label(text)
            .text_size(11.0)
            .weight(FontWeight::SEMI_BOLD)
            .color(TEXT_MAIN),
    )
    .background_color(bg)
    .corner_radius(8.0)
    .padding(Padding::from_vh(1.0, 9.0));
    sized_box(flex_row((chip,)).gap(Length::px(0.0)))
        .width(Length::px(width))
        .padding(Padding::horizontal(CELL_HPAD))
}

/// Child component for the `virtual_scroll` view: one row of the table.
/// `idx` can transiently be outside `visible` while the valid range shrinks
/// (documented VirtualScroll caveat), hence the placeholder branch.
fn row_view(state: &mut AppState, idx: i64) -> impl WidgetView<AppState> + use<> {
    let pos = idx.max(0) as usize;
    let row = if idx >= 0 && pos < state.visible.len() {
        Some(&state.rows[state.visible[pos] as usize])
    } else {
        None
    };
    let id_s = row.map(|r| r.id.to_string()).unwrap_or_default();
    let name_s = row.map(|r| r.name.clone()).unwrap_or_default();
    let cat_s = row
        .map(|r| CATEGORIES[r.category as usize].to_string())
        .unwrap_or_default();
    let val_s = row.map(|r| format!("{:.2}", r.value)).unwrap_or_default();
    let date_s = row.map(|r| r.date.clone()).unwrap_or_default();
    let (chip_txt, chip_bg) = match row.map(|r| r.status) {
        Some(0) => ("Ok", CHIP_OK),
        Some(1) => ("Warn", CHIP_WARN),
        Some(_) => ("Err", CHIP_ERR),
        None => ("", Color::TRANSPARENT),
    };
    let selected = row.is_some_and(|r| state.selected.contains(&r.id));
    let w = state.widths;

    let cells = flex_row((
        cell(id_s, w[0], TextAlign::Start, TEXT_DIM),
        cell(name_s, w[1], TextAlign::Start, TEXT_MAIN),
        cell(cat_s, w[2], TextAlign::Start, TEXT_DIM),
        cell(val_s, w[3], TextAlign::End, TEXT_MAIN),
        cell(date_s, w[4], TextAlign::Start, TEXT_DIM),
        chip_cell(chip_txt, chip_bg, w[5]),
    ))
    .gap(Length::px(0.0));

    row_frame(cells, move |state: &mut AppState, event| {
        let RowEvent::Clicked { shift, cmd } = event;
        state.row_clicked(pos, shift, cmd);
    })
    .bg(if pos % 2 == 0 { BG_EVEN } else { BG_ODD })
    .selected(selected)
    .vpad(3.0)
}

fn header_cell(state: &AppState, col: usize) -> impl WidgetView<AppState> + use<> {
    let name = COL_NAMES[col];
    // ASCII sort indicator: ▲/▼ (U+25B2/U+25BC) render as tofu with
    // masonry's default font setup (same issue as ✕ in xilem-board).
    let indicator = if state.sort_col == col {
        if state.sort_desc {
            " v"
        } else {
            " ^"
        }
    } else {
        ""
    };
    // The drag handle to this cell's right takes HANDLE_W out of the column.
    let w = state.widths[col] - if col < 5 { HANDLE_W } else { 0.0 };
    sized_box(
        row_frame(
            flex_row((label(format!("{name}{indicator}"))
                .text_size(13.0)
                .weight(FontWeight::BOLD)
                .prop(LineBreaking::Clip),))
            .gap(Length::px(0.0))
            .padding(Padding::horizontal(CELL_HPAD)),
            move |state: &mut AppState, event| {
                if matches!(event, RowEvent::Clicked { .. }) {
                    state.toggle_sort(col);
                }
            },
        )
        .header()
        .bg(HEADER_BG)
        .vpad(6.0),
    )
    .width(Length::px(w))
}

fn resize_handle(col: usize) -> impl WidgetView<AppState> + use<> {
    drag_handle(HANDLE_W, 30.0, move |state: &mut AppState, event| {
        state.handle_event(col, event);
    })
}

fn app_logic(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    let toolbar = flex_row((
        sized_box(
            text_input(state.filter.clone(), |state: &mut AppState, value| {
                state.set_filter(value);
            })
            .placeholder("Filter by name…"),
        )
        .width(Length::px(280.0)),
        label(format!(
            "{} of {} rows",
            thousands(state.visible.len()),
            thousands(N_ROWS)
        ))
        .text_size(13.0)
        .color(TEXT_MAIN),
        label(format!("{} selected", thousands(state.selected.len())))
            .text_size(13.0)
            .color(if state.selected.is_empty() {
                TEXT_DIM
            } else {
                ACCENT
            }),
        FlexSpacer::Flex(1.0),
        label("click select · shift range · cmd toggle · drag divider resizes")
            .text_size(11.0)
            .color(TEXT_DIM),
    ))
    .gap(Length::px(14.0))
    .must_fill_major_axis(true);

    let header = flex_row((
        header_cell(state, 0),
        resize_handle(0),
        header_cell(state, 1),
        resize_handle(1),
        header_cell(state, 2),
        resize_handle(2),
        header_cell(state, 3),
        resize_handle(3),
        header_cell(state, 4),
        resize_handle(4),
        header_cell(state, 5),
        // Non-interactive filler so the header bar spans the full width
        // like the row stripes do.
        row_frame(label(""), |_: &mut AppState, _| {})
            .header()
            .bg(HEADER_BG)
            .vpad(6.0)
            .flex(1.0),
    ))
    .gap(Length::px(0.0));

    let n = state.visible.len();
    let body = if n == 0 {
        // An empty valid range is documented VirtualScroll jank; show a
        // plain placeholder instead.
        Either::B(sized_box(label("No matching rows").color(TEXT_DIM)).padding(24.0))
    } else {
        Either::A(virtual_scroll(0..n as i64, row_view))
    };

    flex_col((toolbar, header, body.flex(1.0)))
        .gap(Length::px(6.0))
        .cross_axis_alignment(CrossAxisAlignment::Fill)
        .must_fill_major_axis(true)
        .padding(10.0)
}

fn main() -> Result<(), EventLoopError> {
    let t0 = Instant::now();
    let state = AppState::new();
    println!("BUILD_MS {:.3}", t0.elapsed().as_secs_f64() * 1000.0);

    let mut window = WindowOptions::new("Grid (xilem)")
        .with_initial_inner_size(xilem::dpi::LogicalSize::new(1000.0, 640.0))
        .with_resizable(true);
    // Verification aid on a shared desktop: GRID_TOPMOST=1 keeps the window
    // above other apps so scripted synthetic input cannot land elsewhere.
    if std::env::var("GRID_TOPMOST").as_deref() == Ok("1") {
        window = window.with_window_level(xilem::winit::window::WindowLevel::AlwaysOnTop);
    }
    Xilem::new_simple(state, app_logic, window).run_in(EventLoop::with_user_event())
}
