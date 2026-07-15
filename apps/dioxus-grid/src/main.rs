//! "Grid" — 100k-row data table. Dioxus 0.7 desktop (wry/tao webview).
//!
//! ARCHITECTURE — hand-rolled DOM windowing (there is no table/virtual-list
//! widget for Dioxus desktop):
//!   * The 100k `Row`s are generated once (seeded xorshift64*) into a
//!     process-global `OnceLock` — immutable, `'static`, so row text reaches
//!     RSX as `&'static str` with zero per-frame cloning.
//!   * The *view* is a `Signal<Vec<u32>>` of row indices = filter ∘ sort over
//!     the full Vec, rebuilt eagerly (and self-timed → `FILTER_MS`) on every
//!     filter keystroke / header click.
//!   * Virtualization: one scroll container holds a sticky header plus a
//!     `total_rows * ROW_H`-tall spacer div; only the rows intersecting the
//!     viewport (± OVERSCAN) are rendered, absolutely positioned at
//!     `view_index * ROW_H`. `onscroll` gives `scroll_top()` (ScrollData in
//!     0.7 carries scroll_top/client_height — new since the iteration-2 apps)
//!     and writes the signal **only when the row bucket changes**, so native
//!     compositor scrolling stays smooth and Rust re-renders ~36 rows only
//!     when the window slides. `onresize` (ResizeObserver-backed, fires on
//!     mount) supplies the viewport height.
//!   * Column resize: mouse-event state machine (header divider onmousedown →
//!     root onmousemove applies clientX delta → root onmouseup ends), widths
//!     in a `Signal<[f64; 6]>` rendered as `grid-template-columns`. Events
//!     carry no element geometry (iteration-2/3 lesson) but resize only needs
//!     deltas, so no overlay trick is required here.
//!   * Selection: click → single select; Shift-click → contiguous range from
//!     the last plain-clicked view index (`evt.modifiers()`), ids in a
//!     `Signal<HashSet<u32>>`.
//!
//! Prints `BUILD_MS <ms>` once and `FILTER_MS <query_len> <ms>` per filter
//! application (also `SORT_MS <col> <ms>` as extra evidence). Run with
//! GRID_SELFTEST=1 to exercise filter/sort/selection/resize through the same
//! callbacks the UI uses plus a scripted long scroll through real DOM scroll
//! events (document::eval sets scrollTop → real onscroll → windowing).

use std::collections::HashSet;
use std::sync::OnceLock;
use std::time::Instant;

use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
use dioxus::html::Modifiers;
use dioxus::prelude::*;

const N_ROWS: usize = 100_000;
const ROW_H: f64 = 28.0;
const OVERSCAN: usize = 8; // also absorbs the 36px sticky header offset

const CATEGORIES: [&str; 8] = [
    "Logistics", "Retail", "Energy", "Finance", "Health", "Transit", "Media", "Agri",
];
const ADJ: [&str; 24] = [
    "amber", "brisk", "coral", "dusty", "eager", "fuzzy", "glossy", "hazel", "icy", "jolly",
    "keen", "lunar", "mossy", "noble", "opal", "prime", "quiet", "rusty", "sunny", "tidal",
    "umber", "vivid", "wispy", "zesty",
];
const NOUN: [&str; 24] = [
    "anchor", "beacon", "cobalt", "delta", "ember", "falcon", "garnet", "harbor", "island",
    "jasper", "kernel", "lagoon", "marble", "nectar", "orbit", "prism", "quartz", "ridge",
    "summit", "thicket", "umbra", "vault", "willow", "zenith",
];
const STATUS_LABEL: [&str; 3] = ["Ok", "Warn", "Err"];
const STATUS_CLASS: [&str; 3] = ["ok", "warn", "err"];

/// xorshift64* — deterministic, no `rand` dependency.
struct Rng(u64);
impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

struct Row {
    id: u32,
    name: String,
    category: u8,
    value: f64,
    date: String, // ISO yyyy-mm-dd: lexicographic order == chronological
    status: u8,   // 0 Ok / 1 Warn / 2 Err
}

/// Immutable dataset, generated once. `'static` means RSX borrows row text
/// directly instead of cloning Strings into the render tree every frame.
fn rows() -> &'static Vec<Row> {
    static CELL: OnceLock<Vec<Row>> = OnceLock::new();
    CELL.get_or_init(|| {
        let mut rng = Rng(0x5EED_CAFE_F00D_D00D);
        (0..N_ROWS as u32)
            .map(|id| {
                let name = format!(
                    "{}-{}-{:04}",
                    ADJ[(rng.next_u64() % 24) as usize],
                    NOUN[(rng.next_u64() % 24) as usize],
                    rng.next_u64() % 10_000
                );
                let category = (rng.next_u64() % 8) as u8;
                let value = (rng.next_f64() * 10_000.0 * 100.0).round() / 100.0;
                let date = format!(
                    "{}-{:02}-{:02}",
                    2020 + rng.next_u64() % 6,
                    1 + rng.next_u64() % 12,
                    1 + rng.next_u64() % 28
                );
                let r = rng.next_f64();
                let status = if r < 0.70 { 0 } else if r < 0.90 { 1 } else { 2 };
                Row { id, name, category, value, date, status }
            })
            .collect()
    })
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Col {
    Id,
    Name,
    Category,
    Value,
    Date,
    Status,
}
impl Col {
    const ALL: [Col; 6] = [Col::Id, Col::Name, Col::Category, Col::Value, Col::Date, Col::Status];
    fn label(self) -> &'static str {
        match self {
            Col::Id => "ID",
            Col::Name => "Name",
            Col::Category => "Category",
            Col::Value => "Value",
            Col::Date => "Date",
            Col::Status => "Status",
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
struct Sort {
    col: Col,
    asc: bool,
}

/// filter ∘ sort over the *full* Vec → view of row indices.
fn rebuild_view(rows: &[Row], filter: &str, sort: Sort) -> Vec<u32> {
    let q = filter.trim().to_lowercase(); // names are all-lowercase
    let mut v: Vec<u32> = if q.is_empty() {
        (0..rows.len() as u32).collect()
    } else {
        rows.iter()
            .enumerate()
            .filter(|(_, r)| r.name.contains(&q))
            .map(|(i, _)| i as u32)
            .collect()
    };
    let key = |i: u32| &rows[i as usize];
    match sort.col {
        Col::Id => {} // filtered indices are already id-ascending
        Col::Name => v.sort_unstable_by(|&a, &b| key(a).name.cmp(&key(b).name).then(a.cmp(&b))),
        Col::Category => v.sort_unstable_by(|&a, &b| {
            key(a).category.cmp(&key(b).category).then(a.cmp(&b))
        }),
        Col::Value => v.sort_unstable_by(|&a, &b| {
            key(a).value.partial_cmp(&key(b).value).unwrap().then(a.cmp(&b))
        }),
        Col::Date => v.sort_unstable_by(|&a, &b| key(a).date.cmp(&key(b).date).then(a.cmp(&b))),
        Col::Status => v.sort_unstable_by(|&a, &b| {
            key(a).status.cmp(&key(b).status).then(a.cmp(&b))
        }),
    }
    if !sort.asc {
        v.reverse();
    }
    v
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

#[derive(Clone, Copy, PartialEq)]
struct ColResize {
    col: usize,
    start_x: f64,
    start_w: f64,
}

/// Per-row view model, precomputed before rsx! (borrow discipline: no `.read()`
/// guards held inside nested closures).
struct RowVm {
    vi: usize, // index in the current view
    id: u32,
    y: f64,
    cls: &'static str,
    name: &'static str,
    category: &'static str,
    value: String,
    date: &'static str,
    chip_cls: &'static str,
    chip: &'static str,
}

fn main() {
    dioxus::LaunchBuilder::desktop()
        .with_cfg(
            Config::new().with_window(
                WindowBuilder::new()
                    .with_title("Grid (dioxus)")
                    .with_inner_size(LogicalSize::new(1000.0, 640.0))
                    .with_resizable(true),
            ),
        )
        .launch(App);
}

#[component]
fn App() -> Element {
    // BUILD_MS covers data generation + the initial (identity) view build.
    let mut view = use_signal(|| {
        let t0 = Instant::now();
        let n = rows().len();
        let v: Vec<u32> = (0..n as u32).collect();
        println!("BUILD_MS {:.1}", t0.elapsed().as_secs_f64() * 1000.0);
        v
    });
    let mut filter = use_signal(String::new);
    let mut sort = use_signal(|| Sort { col: Col::Id, asc: true });
    let mut selected = use_signal(HashSet::<u32>::new);
    let mut anchor = use_signal(|| Option::<usize>::None); // view idx of last plain click
    let mut scroll_top = use_signal(|| 0.0f64);
    let mut viewport_h = use_signal(|| 560.0f64);
    let mut widths = use_signal(|| [70.0f64, 230.0, 120.0, 110.0, 120.0, 90.0]);
    let mut resize = use_signal(|| Option::<ColResize>::None);

    // Shared callbacks: the UI handlers and the self-test drive the SAME code.
    let apply_filter = use_callback(move |q: String| {
        let t0 = Instant::now();
        let v = rebuild_view(rows(), &q, *sort.peek());
        println!("FILTER_MS {} {:.2}", q.chars().count(), t0.elapsed().as_secs_f64() * 1000.0);
        filter.set(q);
        view.set(v);
        anchor.set(None); // view indices shifted; ids in `selected` stay valid
    });
    let apply_sort = use_callback(move |col: Col| {
        let s = *sort.peek();
        let ns = Sort { col, asc: if s.col == col { !s.asc } else { true } };
        let t0 = Instant::now();
        let v = rebuild_view(rows(), &filter.peek(), ns);
        println!(
            "SORT_MS {} {} {:.2}",
            col.label(),
            if ns.asc { "asc" } else { "desc" },
            t0.elapsed().as_secs_f64() * 1000.0
        );
        sort.set(ns);
        view.set(v);
        anchor.set(None);
    });
    // Click / Shift-click selection over the current view.
    let do_select = use_callback(move |(vi, id, shift): (usize, u32, bool)| {
        let a = *anchor.peek();
        if shift && a.is_some() {
            let a = a.unwrap();
            let v = view.peek();
            let (lo, hi) = (a.min(vi), a.max(vi));
            selected.set(v[lo..=hi].iter().copied().collect());
        } else {
            selected.set(HashSet::from([id]));
            anchor.set(Some(vi));
        }
    });
    // Column resize step (root onmousemove and the self-test both call this).
    let apply_resize = use_callback(move |x: f64| {
        if let Some(r) = *resize.peek() {
            let w = (r.start_w + (x - r.start_x)).clamp(50.0, 600.0);
            if (widths.peek()[r.col] - w).abs() >= 0.5 {
                widths.write()[r.col] = w;
            }
        }
    });

    // ---- view-model precomputation --------------------------------------
    let v = view.read();
    let total = v.len();
    let st = *scroll_top.read();
    let vh = *viewport_h.read();
    let first = ((st / ROW_H) as usize).saturating_sub(OVERSCAN);
    let count = (vh / ROW_H).ceil() as usize + 2 * OVERSCAN;
    let end = (first + count).min(total);
    let first = first.min(end);
    let sel = selected.read();
    let visible: Vec<RowVm> = v[first..end]
        .iter()
        .enumerate()
        .map(|(k, &ri)| {
            let vi = first + k;
            let r = &rows()[ri as usize];
            RowVm {
                vi,
                id: r.id,
                y: vi as f64 * ROW_H,
                cls: if sel.contains(&r.id) { "row sel" } else { "row" },
                name: &r.name,
                category: CATEGORIES[r.category as usize],
                value: format!("{:.2}", r.value),
                date: &r.date,
                chip_cls: STATUS_CLASS[r.status as usize],
                chip: STATUS_LABEL[r.status as usize],
            }
        })
        .collect();
    drop(sel);
    drop(v);

    let ws = *widths.read();
    let grid_cols = format!(
        "{}px {}px {}px {}px {}px {}px",
        ws[0], ws[1], ws[2], ws[3], ws[4], ws[5]
    );
    let content_w: f64 = ws.iter().sum();
    let total_h = total as f64 * ROW_H;
    let s = *sort.read();
    let resizing = resize.read().is_some();
    let n_sel = selected.read().len();

    rsx! {
        style { {CSS} }
        div {
            class: if resizing { "root resizing" } else { "root" },
            onmousemove: move |evt| {
                if resize.peek().is_some() {
                    apply_resize.call(evt.client_coordinates().x);
                }
            },
            onmouseup: move |_| {
                if resize.peek().is_some() {
                    resize.set(None);
                }
            },
            onmouseleave: move |_| {
                if resize.peek().is_some() {
                    resize.set(None);
                }
            },

            div { class: "toolbar",
                h1 { "Grid" }
                input {
                    r#type: "text",
                    class: "filter",
                    placeholder: "filter by name\u{2026}",
                    value: "{filter}",
                    oninput: move |evt| apply_filter.call(evt.value()),
                }
                span { class: "count", "{thousands(total)} of {thousands(N_ROWS)} rows" }
                if n_sel > 0 {
                    span { class: "selcount", "{thousands(n_sel)} selected" }
                }
                span { class: "hint", "click header: sort · drag divider: resize · shift-click: range" }
            }

            div {
                id: "vp",
                class: "viewport",
                onscroll: move |evt| {
                    // Write only when the row bucket changes: native scrolling
                    // stays on the compositor; Rust re-renders on window slide.
                    let t = evt.scroll_top();
                    if (t / ROW_H) as i64 != (*scroll_top.peek() / ROW_H) as i64 {
                        scroll_top.set(t);
                    }
                },
                onresize: move |evt| {
                    // ResizeObserver-backed; fires once on mount too.
                    if let Ok(sz) = evt.data().get_content_box_size() {
                        viewport_h.set(sz.height);
                    }
                },

                div { class: "header", style: "grid-template-columns: {grid_cols}; width: {content_w}px;",
                    for (i, col) in Col::ALL.iter().copied().enumerate() {
                        div { class: "hcell",
                            div {
                                class: "hlabel",
                                class: if col == Col::Value { "hlabel right" },
                                onclick: move |_| apply_sort.call(col),
                                "{col.label()}"
                                if s.col == col {
                                    span { class: "arrow", if s.asc { " \u{25B2}" } else { " \u{25BC}" } }
                                }
                            }
                            div {
                                class: "divider",
                                onmousedown: move |evt: MouseEvent| {
                                    resize.set(Some(ColResize {
                                        col: i,
                                        start_x: evt.client_coordinates().x,
                                        start_w: widths.peek()[i],
                                    }));
                                },
                            }
                        }
                    }
                }

                div { class: "spacer", style: "height: {total_h}px; width: {content_w}px;",
                    for r in visible {
                        div {
                            key: "{r.id}",
                            class: "{r.cls}",
                            style: "top: {r.y}px; grid-template-columns: {grid_cols};",
                            onclick: {
                                let (vi, id) = (r.vi, r.id);
                                move |evt: MouseEvent| {
                                    do_select.call((vi, id, evt.modifiers().contains(Modifiers::SHIFT)));
                                }
                            },
                            div { class: "cell dim", "{r.id}" }
                            div { class: "cell", "{r.name}" }
                            div { class: "cell dim", "{r.category}" }
                            div { class: "cell num", "{r.value}" }
                            div { class: "cell dim", "{r.date}" }
                            div { class: "cell",
                                span { class: "chip {r.chip_cls}", "{r.chip}" }
                            }
                        }
                    }
                }
            }
        }

        // ================= VERIFICATION (GRID_SELFTEST=1) =================
        // Drives the same callbacks the UI handlers call (filter/sort/select/
        // resize) plus a real-DOM long scroll via document::eval → onscroll.
        {
            let _ = use_future(move || async move {
                if std::env::var("GRID_SELFTEST").is_err() {
                    return;
                }
                let ms = |n: u64| tokio::time::sleep(std::time::Duration::from_millis(n));
                ms(1500).await;
                // Simulated typing: a → am → amb → ambe (each prints FILTER_MS).
                for q in ["a", "am", "amb", "ambe"] {
                    apply_filter.call(q.to_string());
                    ms(250).await;
                }
                println!("SELFTEST_FILTERED_ROWS {}", view.peek().len());
                apply_filter.call(String::new());
                ms(250).await;
                apply_sort.call(Col::Value); // asc
                ms(250).await;
                apply_sort.call(Col::Value); // desc
                ms(250).await;
                // Selection: plain click on view row 5, shift-click on 25.
                let id5 = view.peek()[5];
                let id25 = view.peek()[25];
                do_select.call((5, id5, false));
                do_select.call((25, id25, true));
                println!("SELFTEST_SELECTED {}", selected.peek().len());
                // Column resize through the production drag path.
                resize.set(Some(ColResize { col: 1, start_x: 300.0, start_w: widths.peek()[1] }));
                apply_resize.call(370.0); // drag +70px
                resize.set(None);
                println!("SELFTEST_NAME_COL_W {}", widths.peek()[1]);
                // Long scroll: real DOM scrollTop writes → real onscroll events.
                let h = view.peek().len() as f64 * ROW_H;
                for i in 0..=40u32 {
                    let y = f64::from(i) / 40.0 * h;
                    let js = format!("return (document.getElementById('vp').scrollTop = {y});");
                    let _ = dioxus::document::eval(&js).await;
                    ms(60).await;
                }
                ms(300).await;
                println!("SELFTEST_SCROLL_TOP {:.0}", scroll_top.peek());
                println!("SELFTEST_DONE");
            });
        }
    }
}

const CSS: &str = r#"
* { margin: 0; padding: 0; box-sizing: border-box; }
html, body { height: 100%; }
body {
  background: #0e1320; color: #e8ecf4; overflow: hidden;
  font-family: system-ui, -apple-system, sans-serif; font-size: 13px;
  -webkit-user-select: none; user-select: none;
}
.root { display: flex; flex-direction: column; gap: 10px; padding: 12px 14px; height: 100vh; }
.resizing, .resizing * { cursor: col-resize !important; }
.toolbar { display: flex; align-items: center; gap: 12px; }
h1 { font-size: 17px; letter-spacing: .4px; }
.filter {
  background: #171f31; color: #e8ecf4; border: 1px solid #33456b; border-radius: 6px;
  padding: 5px 10px; font-size: 13px; width: 220px; outline: none;
  -webkit-user-select: text; user-select: text;
}
.filter:focus { border-color: #7aa2ff; }
.count { color: #9db0d0; font-variant-numeric: tabular-nums; }
.selcount { color: #ffd166; font-variant-numeric: tabular-nums; }
.hint { margin-left: auto; font-size: 11px; color: #5e6f92; }
.viewport {
  flex: 1; overflow: auto; position: relative;
  background: #121a2b; border: 1px solid #263354; border-radius: 10px;
}
.header {
  display: grid; height: 36px; position: sticky; top: 0; z-index: 5;
  background: #1a2337; border-bottom: 1px solid #33456b; min-width: 100%;
}
.hcell { position: relative; display: flex; align-items: stretch; overflow: hidden; }
.hlabel {
  flex: 1; display: flex; align-items: center; padding: 0 10px; cursor: pointer;
  font-weight: 600; color: #b8c6e0; white-space: nowrap; overflow: hidden;
}
.hlabel:hover { color: #ffffff; background: #22304d; }
.hlabel.right { justify-content: flex-end; }
.arrow { color: #7aa2ff; font-size: 10px; }
.divider {
  width: 7px; margin-left: -7px; cursor: col-resize; z-index: 6;
  border-right: 1px solid #33456b; align-self: stretch; margin-left: auto;
}
.divider:hover { border-right: 3px solid #7aa2ff; }
.spacer { position: relative; }
.row {
  position: absolute; left: 0; display: grid; height: 28px; width: 100%;
  align-items: center; border-bottom: 1px solid #1c2740; cursor: default;
}
.row:hover { background: #1a2337; }
.row.sel { background: #24365c; }
.row.sel:hover { background: #2a3f6b; }
.cell {
  padding: 0 10px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  font-variant-numeric: tabular-nums;
}
.cell.dim { color: #9db0d0; }
.cell.num { text-align: right; color: #cfe0ff; }
.chip {
  display: inline-block; padding: 1px 9px; border-radius: 999px;
  font-size: 11px; font-weight: 600;
}
.chip.ok   { background: #10331f; color: #5ad18b; border: 1px solid #1e5c38; }
.chip.warn { background: #3a2f10; color: #ffd166; border: 1px solid #6b5518; }
.chip.err  { background: #3a1519; color: #ff7a85; border: 1px solid #6b2730; }
"#;
