//! "Grid" — 100k-row data table in gpui 0.2.2 (SPEC-7).
//!
//! gpui ships no table widget. The table here is hand-assembled:
//! - **virtualization**: `uniform_list` (the same element Zed uses for its
//!   big lists) — only the visible rows are laid out and painted, so 100k
//!   rows cost the same per frame as 30,
//! - **header/sort/selection**: styled `div()`s + `on_click` (ClickEvent
//!   exposes modifiers, so Shift/Cmd-click range/toggle selection is native),
//! - **column resize**: a real drag on a 7 px divider using gpui's typed DnD
//!   (`on_drag` with an invisible ghost + `on_drag_move` on the header row,
//!   which hands the listener its own bounds — the same trick as the
//!   gpui-dash slider),
//! - **filter input**: the minimal hand-rolled `on_key_down` input from
//!   gpui-app/gpui-board (gpui has no text-input widget; no IME/selection).
//!
//! Model: `rows` (immutable after generation) + `visible` (a Vec<u32> of row
//! indices — filter and sort permute indices, never the data). Selection is
//! a HashSet of row *ids* so it survives re-sort/re-filter.
//!
//! Instrumentation (SPEC-7): prints `BUILD_MS <ms>` once, `FILTER_MS
//! <query_len> <ms>` per filter application, and `SORT_MS <col> <ms>` per
//! sort. `GRID_SELFTEST=1` runs a scripted pass through the same code paths
//! the UI events call (filter typing, sort, long scroll via the scroll
//! handle) so timings/RSS can be captured without OS-level input injection
//! (blocked on this machine, see apps/gpui-app/GAPS.md).

use std::collections::HashSet;
use std::time::{Duration, Instant};

use gpui::{
    App, Application, Bounds, ClickEvent, Context, DragMoveEvent, Entity, FocusHandle,
    KeyDownEvent, Modifiers, ScrollStrategy, SharedString, TitlebarOptions,
    UniformListScrollHandle, Window, WindowBounds, WindowOptions, div, prelude::*, px, rgb, rgba,
    size, uniform_list,
};

const ROW_COUNT: usize = 100_000;
const ROW_H: f32 = 28.0;
const ACCENT: u32 = 0x3b82f6;

// ---------------------------------------------------------------------------
// Data model (deterministic — same 100k rows every run)
// ---------------------------------------------------------------------------

const ADJECTIVES: [&str; 16] = [
    "amber", "brisk", "coral", "dusty", "eager", "fuzzy", "glossy", "hazel", "icy", "jolly",
    "keen", "lunar", "mossy", "noble", "opal", "prime",
];
const NOUNS: [&str; 16] = [
    "anchor", "beacon", "cobalt", "delta", "ember", "falcon", "garnet", "harbor", "island",
    "jasper", "kernel", "lagoon", "marble", "nectar", "orbit", "prism",
];
const CATEGORIES: [&str; 8] = [
    "Alpha", "Bravo", "Charlie", "Delta", "Echo", "Foxtrot", "Golf", "Hotel",
];

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
}

struct Row {
    id: u32,
    name: SharedString,
    /// Precomputed lowercase name for substring filtering.
    name_lc: String,
    category: &'static str,
    value: f64,
    date: SharedString,
    status: Status,
}

/// xorshift64 — seeded PRNG, no rand crate (same approach as gpui-dash).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

/// Civil-from-days (Howard Hinnant's algorithm) — ISO date without a chrono dep.
fn iso_date(days_since_epoch: i64) -> String {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

fn generate_rows(n: usize) -> Vec<Row> {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut rows = Vec::with_capacity(n);
    for id in 0..n as u32 {
        let r = rng.next();
        let adj = ADJECTIVES[(r & 0xf) as usize];
        let noun = NOUNS[((r >> 4) & 0xf) as usize];
        let num = (r >> 8) % 10_000;
        let name = format!("{adj}-{noun}-{num:04}");
        let category = CATEGORIES[((r >> 24) & 0x7) as usize];
        let value = ((rng.next() % 1_000_000) as f64) / 100.0; // 0.00 .. 9999.99
        // 2020-01-01 (epoch day 18262) + 0..2191 days => 2020..2025
        let date = iso_date(18_262 + (rng.next() % 2192) as i64);
        let status = match rng.next() % 10 {
            0..=6 => Status::Ok,
            7..=8 => Status::Warn,
            _ => Status::Err,
        };
        rows.push(Row {
            id,
            name_lc: name.to_lowercase(),
            name: name.into(),
            category,
            value,
            date: date.into(),
            status,
        });
    }
    rows
}

// ---------------------------------------------------------------------------
// Columns
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Col {
    Id,
    Name,
    Category,
    Value,
    Date,
    Status,
}

const COLS: [(Col, &str); 6] = [
    (Col::Id, "ID"),
    (Col::Name, "Name"),
    (Col::Category, "Category"),
    (Col::Value, "Value"),
    (Col::Date, "Date"),
    (Col::Status, "Status"),
];

const DEFAULT_WIDTHS: [f32; 6] = [70.0, 250.0, 120.0, 110.0, 120.0, 90.0];

/// Typed drag payload for the column-resize divider (gpui routes DnD by type).
#[derive(Clone)]
struct ColResize {
    col: usize,
}

/// Invisible drag ghost — we only want the `on_drag_move` stream.
struct NoGhost;

impl Render for NoGhost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

// ---------------------------------------------------------------------------
// App entity
// ---------------------------------------------------------------------------

struct GridApp {
    rows: Vec<Row>,
    /// Row indices after filter + sort — the only thing render looks at.
    visible: Vec<u32>,
    filter: String,
    sort: Option<(Col, bool /* desc */)>,
    /// Selected row ids (ids, not indices — survives re-sort/re-filter).
    selected: HashSet<u32>,
    /// Visible-index of the last plain click — the Shift-click range anchor.
    anchor: Option<usize>,
    widths: [f32; 6],
    scroll: UniformListScrollHandle,
    filter_focus: FocusHandle,
}

impl GridApp {
    fn new(rows: Vec<Row>, visible: Vec<u32>, cx: &mut Context<Self>) -> Self {
        if std::env::var("GRID_SELFTEST").is_ok() {
            Self::spawn_selftest(cx);
        }
        Self {
            rows,
            visible,
            filter: String::new(),
            sort: None,
            selected: HashSet::new(),
            anchor: None,
            widths: DEFAULT_WIDTHS,
            scroll: UniformListScrollHandle::new(),
            filter_focus: cx.focus_handle(),
        }
    }

    // -- filter / sort ------------------------------------------------------

    /// Rebuild `visible` from the filter, re-applying the active sort.
    /// Timed per SPEC-7: `FILTER_MS <query_len> <ms>` (the full cost of one
    /// filter keystroke, including the re-sort if a sort is active).
    fn apply_filter(&mut self, cx: &mut Context<Self>) {
        let t0 = Instant::now();
        let q = self.filter.to_lowercase();
        self.visible = if q.is_empty() {
            (0..self.rows.len() as u32).collect()
        } else {
            self.rows
                .iter()
                .enumerate()
                .filter(|(_, r)| r.name_lc.contains(&q))
                .map(|(i, _)| i as u32)
                .collect()
        };
        if let Some((col, desc)) = self.sort {
            self.sort_visible(col, desc);
        }
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        println!("FILTER_MS {} {ms:.2}", self.filter.chars().count());
        self.anchor = None;
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        cx.notify();
    }

    fn toggle_sort(&mut self, col: Col, cx: &mut Context<Self>) {
        let desc = match self.sort {
            Some((c, d)) if c == col => !d, // second click: flip direction
            _ => false,                     // first click: ascending
        };
        self.sort = Some((col, desc));
        let t0 = Instant::now();
        self.sort_visible(col, desc);
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        let name = COLS.iter().find(|(c, _)| *c == col).unwrap().1;
        println!("SORT_MS {name} {ms:.2}");
        self.anchor = None;
        cx.notify();
    }

    fn sort_visible(&mut self, col: Col, desc: bool) {
        let rows = &self.rows;
        self.visible.sort_unstable_by(|&a, &b| {
            let (ra, rb) = (&rows[a as usize], &rows[b as usize]);
            let ord = match col {
                Col::Id => ra.id.cmp(&rb.id),
                Col::Name => ra.name.cmp(&rb.name),
                Col::Category => ra.category.cmp(rb.category),
                Col::Value => ra.value.total_cmp(&rb.value),
                Col::Date => ra.date.cmp(&rb.date),
                Col::Status => ra.status.rank().cmp(&rb.status.rank()),
            };
            if desc { ord.reverse() } else { ord }
        });
    }

    // -- selection ----------------------------------------------------------

    fn row_clicked(&mut self, vis_ix: usize, modifiers: Modifiers, cx: &mut Context<Self>) {
        if vis_ix >= self.visible.len() {
            return;
        }
        let id = self.rows[self.visible[vis_ix] as usize].id;
        if modifiers.shift {
            // Range selection from the anchor (or the top if none).
            let a = self.anchor.unwrap_or(0).min(self.visible.len() - 1);
            let (lo, hi) = if a <= vis_ix { (a, vis_ix) } else { (vis_ix, a) };
            self.selected = self.visible[lo..=hi]
                .iter()
                .map(|&i| self.rows[i as usize].id)
                .collect();
        } else if modifiers.platform {
            // Cmd-click toggles membership.
            if !self.selected.remove(&id) {
                self.selected.insert(id);
            }
            self.anchor = Some(vis_ix);
        } else {
            self.selected = HashSet::from([id]);
            self.anchor = Some(vis_ix);
        }
        cx.notify();
    }

    // -- filter input (hand-rolled; gpui has no text-input widget) ----------

    fn on_filter_key(&mut self, ev: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let ks = &ev.keystroke;
        if ks.modifiers.platform || ks.modifiers.control || ks.modifiers.function {
            return;
        }
        match ks.key.as_str() {
            "backspace" => {
                if self.filter.pop().is_some() {
                    self.apply_filter(cx);
                }
            }
            "escape" => {
                if !self.filter.is_empty() {
                    self.filter.clear();
                    self.apply_filter(cx);
                }
            }
            _ => {
                if let Some(c) = &ks.key_char {
                    self.filter.push_str(c);
                    self.apply_filter(cx);
                }
            }
        }
    }

    fn set_filter_text(&mut self, text: String, cx: &mut Context<Self>) {
        self.filter = text;
        self.apply_filter(cx);
    }

    // -- column resize ------------------------------------------------------

    /// `on_drag_move` on the header row: bounds are the header row's own, so
    /// column i's left edge is `bounds.origin.x + sum(widths[..i])`.
    fn on_resize_drag(&mut self, ev: &DragMoveEvent<ColResize>, _: &mut Window, cx: &mut Context<Self>) {
        let col = ev.drag(cx).col;
        let start_x: f32 = f32::from(ev.bounds.origin.x) + self.widths[..col].iter().sum::<f32>();
        let w = (f32::from(ev.event.position.x) - start_x).clamp(56.0, 640.0);
        if (w - self.widths[col]).abs() > 0.5 {
            self.widths[col] = w;
            cx.notify();
        }
    }

    // -- self-test (verification only; same code paths as the UI events) ----

    fn spawn_selftest(cx: &mut Context<Self>) {
        let exec = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            exec.timer(Duration::from_secs(3)).await; // settle + RSS "after load"
            println!("SELFTEST filter pass");
            for q in ["a", "am", "amb", "ambe"] {
                let _ = this.update(cx, |t, cx| t.set_filter_text(q.to_string(), cx));
                exec.timer(Duration::from_millis(400)).await;
            }
            let _ = this.update(cx, |t, cx| t.set_filter_text(String::new(), cx));
            exec.timer(Duration::from_millis(400)).await;
            println!("SELFTEST sort pass");
            let _ = this.update(cx, |t, cx| t.toggle_sort(Col::Name, cx)); // asc
            exec.timer(Duration::from_millis(400)).await;
            let _ = this.update(cx, |t, cx| t.toggle_sort(Col::Name, cx)); // desc
            exec.timer(Duration::from_millis(400)).await;
            let _ = this.update(cx, |t, cx| t.toggle_sort(Col::Value, cx));
            exec.timer(Duration::from_millis(400)).await;
            println!("SELFTEST long scroll");
            for i in 0..=120 {
                let _ = this.update(cx, |t, cx| {
                    let last = t.visible.len().saturating_sub(1);
                    t.scroll.scroll_to_item(last * i / 120, ScrollStrategy::Top);
                    cx.notify();
                });
                exec.timer(Duration::from_millis(50)).await;
            }
            println!("SELFTEST_DONE");
        })
        .detach();
    }
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

fn status_chip(status: Status) -> impl IntoElement {
    let (bg, fg) = match status {
        Status::Ok => (0xdcfce7, 0x166534),
        Status::Warn => (0xfef3c7, 0x92400e),
        Status::Err => (0xfee2e2, 0x991b1b),
    };
    div()
        .px_2()
        .rounded_full()
        .bg(rgb(bg))
        .text_color(rgb(fg))
        .text_xs()
        .child(status.label())
}

/// One data row for the uniform_list (plain function: the list's render
/// closure has `&mut App`, not `Context<Self>`, so callbacks capture the
/// entity handle and call `entity.update`).
fn render_data_row(
    app: &GridApp,
    vis_ix: usize,
    entity: Entity<GridApp>,
) -> gpui::AnyElement {
    let row = &app.rows[app.visible[vis_ix] as usize];
    let selected = app.selected.contains(&row.id);
    let w = app.widths;

    let cell = |width: f32| div().w(px(width)).h_full().px_2().flex().items_center().flex_none();

    div()
        .id(("row", vis_ix))
        .h(px(ROW_H))
        .w_full()
        .flex()
        .items_center()
        .text_sm()
        .text_color(rgb(0x111827))
        .bg(if selected {
            rgb(0xdbeafe)
        } else if vis_ix % 2 == 1 {
            rgb(0xf8fafc)
        } else {
            rgb(0xffffff)
        })
        .border_b_1()
        .border_color(rgb(0xf1f5f9))
        .when(selected, |d| d.border_color(rgb(0xbfdbfe)))
        .hover(|s| s.bg(if selected { rgb(0xbfdbfe) } else { rgb(0xeff6ff) }))
        .on_click(move |ev: &ClickEvent, _, cx| {
            let modifiers = ev.modifiers();
            entity.update(cx, |this, cx| this.row_clicked(vis_ix, modifiers, cx));
        })
        .child(cell(w[0]).text_color(rgb(0x6b7280)).child(format!("{}", row.id)))
        .child(cell(w[1]).child(div().truncate().child(row.name.clone())))
        .child(cell(w[2]).child(row.category))
        .child(cell(w[3]).justify_end().child(format!("{:.2}", row.value))) // right-aligned
        .child(cell(w[4]).child(row.date.clone()))
        .child(cell(w[5]).child(status_chip(row.status)))
        .into_any_element()
}

impl GridApp {
    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut cells: Vec<gpui::AnyElement> = Vec::new();
        for (i, (col, label)) in COLS.iter().enumerate() {
            let col = *col;
            let indicator = match self.sort {
                Some((c, false)) if c == col => " ▲",
                Some((c, true)) if c == col => " ▼",
                _ => "",
            };
            cells.push(
                div()
                    .id(("header", i))
                    .relative()
                    .flex_none()
                    .w(px(self.widths[i]))
                    .h_full()
                    .px_2()
                    .flex()
                    .items_center()
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(0xe2e8f0)))
                    .when(i == 3, |d| d.justify_end()) // Value header right-aligned too
                    .on_click(cx.listener(move |this, _, _, cx| this.toggle_sort(col, cx)))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(0x334155))
                            .child(format!("{label}{indicator}")),
                    )
                    // Resize divider: a real drag (invisible ghost); the header
                    // row's on_drag_move does the width math.
                    .child(
                        div()
                            .id(("divider", i))
                            .absolute()
                            .right(px(-3.))
                            .top_0()
                            .bottom_0()
                            .w(px(7.))
                            .cursor_col_resize()
                            .child(div().absolute().right(px(3.)).top_0().bottom_0().w(px(1.)).bg(rgb(0xcbd5e1)))
                            .hover(|s| s.bg(rgba(0x3b82f640)))
                            .on_click(|_, _, cx| cx.stop_propagation()) // don't sort on divider click
                            .on_drag(ColResize { col: i }, |_, _, _, cx| cx.new(|_| NoGhost)),
                    )
                    .into_any_element(),
            );
        }
        div()
            .id("header-row")
            .flex_none()
            .h(px(32.))
            .w_full()
            .flex()
            .bg(rgb(0xf1f5f9))
            .border_b_1()
            .border_color(rgb(0xcbd5e1))
            .on_drag_move(cx.listener(Self::on_resize_drag))
            .children(cells)
    }

    fn render_toolbar(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.filter_focus.is_focused(window);
        let input = div()
            .id("filter")
            .track_focus(&self.filter_focus)
            .on_key_down(cx.listener(Self::on_filter_key))
            .w(px(280.))
            .h(px(30.))
            .px_2()
            .flex()
            .items_center()
            .bg(gpui::white())
            .border_1()
            .border_color(if focused { rgb(ACCENT) } else { rgb(0xd1d5db) })
            .rounded_md()
            .cursor_text()
            .text_sm()
            .child(if self.filter.is_empty() {
                div().text_color(rgb(0x9ca3af)).child("Filter by name…")
            } else {
                div().text_color(rgb(0x111827)).child(self.filter.clone())
            })
            .when(focused, |d| d.child(div().w(px(1.5)).h(px(16.)).bg(rgb(0x111827))));

        div()
            .flex_none()
            .flex()
            .items_center()
            .gap_3()
            .px_3()
            .py_2()
            .child(input)
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0x475569))
                    .child(format!("{} of {} rows", self.visible.len(), self.rows.len())),
            )
            .child(div().flex_1())
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0x64748b))
                    .child(format!("{} selected", self.selected.len())),
            )
    }
}

impl Render for GridApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let table = uniform_list(
            "grid-rows",
            self.visible.len(),
            move |range, _window, cx: &mut App| {
                let mut out = Vec::with_capacity(range.len());
                for vis_ix in range {
                    let el = render_data_row(entity.read(cx), vis_ix, entity.clone());
                    out.push(el);
                }
                out
            },
        )
        .track_scroll(self.scroll.clone())
        .flex_1();

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0xf8fafc))
            .child(self.render_toolbar(window, cx))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .mx_3()
                    .mb_3()
                    .min_h(px(0.))
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(0xcbd5e1))
                    .bg(gpui::white())
                    .overflow_hidden()
                    .child(self.render_header(cx))
                    .child(table),
            )
    }
}

// ---------------------------------------------------------------------------

fn main() {
    // BUILD_MS: data generation + initial model (index vector) build.
    let t0 = Instant::now();
    let rows = generate_rows(ROW_COUNT);
    let visible: Vec<u32> = (0..rows.len() as u32).collect();
    println!("BUILD_MS {}", t0.elapsed().as_millis());

    Application::new().run(move |cx: &mut App| {
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        let bounds = Bounds::centered(None, size(px(1000.), px(640.)), cx);
        cx.open_window(
            WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some(SharedString::from("Grid (gpui)")),
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| {
                cx.new(|cx| {
                    let app = GridApp::new(rows, visible, cx);
                    app.filter_focus.focus(window);
                    app
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
