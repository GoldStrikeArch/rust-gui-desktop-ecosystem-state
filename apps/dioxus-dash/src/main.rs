//! "Pulse" — live metrics dashboard. Dioxus 0.7 desktop (wry/tao webview).
//!
//! REPAINT STRATEGY: all state lives in Rust `Signal`s. The tick loop is one
//! `use_future` that sleeps 1/hz (tokio::time::sleep) and writes the `metrics`
//! signal. Every write marks the (single) App component dirty; Dioxus re-runs
//! the RSX, diffs the VDOM, and ships *minimal DOM edits* to the webview over
//! its IPC channel — per tick that is essentially six `points` attribute swaps
//! (sparklines), one big-polyline swap, and a handful of text nodes. The
//! webview then repaints on its own compositor. At 10 Hz this is trivial; at
//! 60 Hz the whole App closure re-runs 60x/s (string-building ~2 KB of SVG
//! points each frame) — still cheap for this tree, and scoping could be
//! tightened by splitting cards into child components that each subscribe to
//! their own signal slice. While paused (or when no signal changes) nothing
//! re-renders at all: repaint is purely write-driven, there is no frame loop.
//!
//! Charts are SVG generated in RSX (idiomatic path) rather than Canvas2D via
//! `document::eval`: SVG keeps the chart inside the declarative signal->diff
//! model (no JS strings, no imperative escape hatch) and the diff cost is one
//! attribute per polyline. Canvas would need hand-written JS ticked from Rust,
//! which is exactly the escape hatch this experiment tries to avoid.
//!
//! Drag & drop is hand-rolled on mouse events with Rust-side drag state
//! (onmousedown/onmousemove/onmouseup + a 6px threshold); HTML5 `draggable`
//! also exists in RSX but gives less control (snapshot ghost, dragover
//! prevent_default choreography) — see FRICTION.md.

use std::collections::VecDeque;
use std::time::Duration;

use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
use dioxus::prelude::*;

const HISTORY: usize = 300; // main chart window (samples)
const SPARK: usize = 60; // sparkline window (samples)
const CHART_H: f64 = 250.0;

fn main() {
    dioxus::LaunchBuilder::desktop()
        .with_cfg(
            Config::new().with_window(
                WindowBuilder::new()
                    .with_title("Pulse (dioxus)")
                    .with_inner_size(LogicalSize::new(900.0, 640.0))
                    .with_resizable(true),
            ),
        )
        .launch(App);
}

/// Tiny xorshift64* PRNG so the synthetic feed needs no `rand` dependency.
struct Rng(u64);
impl Rng {
    fn next_f64(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
    }
}

struct Metric {
    name: &'static str,
    unit: &'static str,
    scale: f64, // display = raw(0..100) * scale
    color: &'static str,
    value: f64,
    vel: f64,
    history: VecDeque<f64>,
}

impl Metric {
    fn new(name: &'static str, unit: &'static str, scale: f64, color: &'static str, rng: &mut Rng) -> Self {
        let mut m = Metric {
            name,
            unit,
            scale,
            color,
            value: 20.0 + 60.0 * rng.next_f64(),
            vel: 0.0,
            history: VecDeque::with_capacity(HISTORY),
        };
        for _ in 0..HISTORY {
            m.tick(rng); // pre-fill so every chart starts full
        }
        m
    }

    /// Smooth random walk: damped velocity + noise, clamped to 0..100.
    fn tick(&mut self, rng: &mut Rng) {
        self.vel = (self.vel + (rng.next_f64() - 0.5) * 6.0) * 0.72;
        self.value = (self.value + self.vel).clamp(0.0, 100.0);
        if self.history.len() == HISTORY {
            self.history.pop_front();
        }
        self.history.push_back(self.value);
    }
}

/// SVG polyline points for the newest `take` samples, right-aligned in w x h.
fn polyline_points(hist: &VecDeque<f64>, take: usize, w: f64, h: f64, pad: f64) -> String {
    let n = hist.len().min(take);
    if n < 2 {
        return String::new();
    }
    let step = w / (take - 1) as f64;
    let x0 = w - (n - 1) as f64 * step;
    let skip = hist.len() - n;
    let mut s = String::with_capacity(n * 12);
    for (i, v) in hist.iter().skip(skip).enumerate() {
        let x = x0 + i as f64 * step;
        let y = pad + (h - 2.0 * pad) * (1.0 - v / 100.0);
        s.push_str(&format!("{x:.1},{y:.1} "));
    }
    s
}

#[derive(Clone, Copy, PartialEq)]
struct Drag {
    src: usize,             // grid slot the drag started from
    started: bool,          // true once cursor moved > threshold
    x: f64,                 // current cursor (client coords, for the ghost)
    y: f64,
    ox: f64,                // press origin (for the threshold)
    oy: f64,
    target: Option<usize>,  // slot the card would land in
}

struct CardVm {
    slot: usize,
    mid: usize,
    cls: String,
    name: &'static str,
    unit: &'static str,
    color: &'static str,
    value: String,
    points: String,
}

#[component]
fn App() -> Element {
    let mut metrics = use_signal(|| {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        vec![
            Metric::new("CPU", "%", 1.0, "#7aa2ff", &mut rng),
            Metric::new("Memory", "GB", 0.24, "#5ad1b3", &mut rng),
            Metric::new("Net In", "Mb/s", 9.4, "#ffd166", &mut rng),
            Metric::new("Net Out", "Mb/s", 4.2, "#f4845f", &mut rng),
            Metric::new("Disk", "MB/s", 5.5, "#c792ea", &mut rng),
            Metric::new("Requests", "r/s", 31.0, "#ef6b9d", &mut rng),
        ]
    });
    let mut order = use_signal(|| (0..6).collect::<Vec<usize>>());
    let mut selected = use_signal(|| 0usize);
    let mut rate = use_signal(|| 10u32);
    let mut paused = use_signal(|| false);
    let mut drag = use_signal(|| Option::<Drag>::None);
    let mut hover = use_signal(|| Option::<f64>::None); // cursor x inside the chart svg
    let mut chart_w = use_signal(|| 860.0f64); // kept in sync by onresize
    let mut ticks = use_signal(|| 0u64);

    // The generator: one long-lived task; reads rate/paused fresh every lap,
    // so slider changes take effect on the next tick without restarting it.
    use_future(move || async move {
        let mut rng = Rng(0xC0FF_EE12_3456_789B);
        loop {
            let hz = (*rate.read()).clamp(1, 60) as f64;
            tokio::time::sleep(Duration::from_secs_f64(1.0 / hz)).await;
            if !*paused.read() {
                for m in metrics.write().iter_mut() {
                    m.tick(&mut rng);
                }
                ticks += 1;
            }
        }
    });

    // ---- view-model precomputation (keeps rsx! free of borrow gymnastics) --
    let drag_v = *drag.read();
    let dragging = drag_v.map_or(false, |d| d.started);
    let sel = *selected.read();

    let cards: Vec<CardVm> = {
        let ms = metrics.read();
        order
            .read()
            .iter()
            .copied()
            .enumerate()
            .map(|(slot, mid)| {
                let m = &ms[mid];
                let mut cls = String::from("card");
                if sel == mid {
                    cls.push_str(" selected");
                }
                if let Some(d) = drag_v.filter(|d| d.started) {
                    if d.src == slot {
                        cls.push_str(" drag-src");
                    } else if d.target == Some(slot) {
                        cls.push_str(" drop-target");
                    }
                }
                CardVm {
                    slot,
                    mid,
                    cls,
                    name: m.name,
                    unit: m.unit,
                    color: m.color,
                    value: format!("{:.1}", m.value * m.scale),
                    points: polyline_points(&m.history, SPARK, 120.0, 36.0, 3.0),
                }
            })
            .collect()
    };

    let w = *chart_w.read();
    let (sel_name, sel_color, main_pts, crosshair, tooltip) = {
        let ms = metrics.read();
        let m = &ms[sel];
        let pts = polyline_points(&m.history, HISTORY, w, CHART_H, 10.0);
        // Crosshair + tooltip: map cursor x back to a sample index.
        let hv = (*hover.read()).map(|hx| {
            let n = m.history.len();
            let step = w / (HISTORY - 1) as f64;
            let x0 = w - (n - 1) as f64 * step;
            let i = (((hx - x0) / step).round().max(0.0) as usize).min(n - 1);
            let v = m.history[i];
            let x = x0 + i as f64 * step;
            let y = 10.0 + (CHART_H - 20.0) * (1.0 - v / 100.0);
            let tip = format!("{:.1} {}  ·  t\u{2212}{}", v * m.scale, m.unit, n - 1 - i);
            let tip_x = if x > w - 170.0 { x - 165.0 } else { x + 14.0 };
            let tip_y = (y - 44.0).max(6.0);
            ((x, y), (tip, tip_x, tip_y))
        });
        let (crosshair, tooltip) = match hv {
            Some((c, t)) => (Some(c), Some(t)),
            None => (None, None),
        };
        (m.name, m.color, pts, crosshair, tooltip)
    };
    // Gridlines precomputed: rsx format strings stay simple interpolations.
    let gridlines: Vec<(f64, f64, u32)> = [0u32, 25, 50, 75, 100]
        .iter()
        .map(|&gv| {
            let y = 10.0 + (CHART_H - 20.0) * (1.0 - gv as f64 / 100.0);
            (y, y - 4.0, gv)
        })
        .collect();

    let ghost = drag_v.filter(|d| d.started).map(|d| {
        let m = &metrics.read()[order.read()[d.src]];
        (d.x + 12.0, d.y + 10.0, m.name, format!("{:.1} {}", m.value * m.scale, m.unit), m.color)
    });

    rsx! {
        style { {CSS} }
        div {
            class: if dragging { "root dragging" } else { "root" },
            // Root-level move/up implement the drag state machine; card-level
            // move (below) picks the drop target. Events bubble, so both fire.
            onmousemove: move |evt| {
                if drag.peek().is_some() {
                    let p = evt.client_coordinates();
                    let mut dw = drag.write();
                    let d = dw.as_mut().unwrap();
                    d.x = p.x;
                    d.y = p.y;
                    if !d.started && (p.x - d.ox).hypot(p.y - d.oy) > 6.0 {
                        d.started = true;
                    }
                }
            },
            onmouseup: move |_| {
                let d = *drag.peek();
                if d.is_some() {
                    drag.set(None);
                }
                if let Some(d) = d {
                    if !d.started {
                        // A press without movement is a click: select the card.
                        selected.set(order.read()[d.src]);
                    } else if let Some(t) = d.target {
                        if t != d.src {
                            let mut o = order.write();
                            let m = o.remove(d.src);
                            o.insert(t, m);
                        }
                    }
                }
            },
            onmouseleave: move |_| {
                if drag.peek().is_some() {
                    drag.set(None); // cursor left the window: cancel the drag
                }
            },

            div { class: "toprow",
                h1 { "Pulse" }
                span { class: "hint", "drag cards to reorder · click to select" }
                div { class: "controls",
                    button {
                        class: "btn",
                        onclick: move |_| {
                            let p = !*paused.peek();
                            paused.set(p);
                        },
                        if *paused.read() { "Resume" } else { "Pause" }
                    }
                    input {
                        r#type: "range",
                        min: "1",
                        max: "60",
                        value: "{rate}",
                        oninput: move |evt| {
                            if let Ok(v) = evt.value().parse::<u32>() {
                                rate.set(v.clamp(1, 60));
                            }
                        },
                    }
                    span { class: "ratelbl", "{rate} Hz" }
                    span { class: "ticks", "{ticks} samples" }
                }
            }

            div { class: "grid",
                for c in cards {
                    div {
                        key: "{c.mid}",
                        class: "{c.cls}",
                        onmousedown: {
                            let slot = c.slot;
                            move |evt: MouseEvent| {
                                let p = evt.client_coordinates();
                                drag.set(Some(Drag {
                                    src: slot,
                                    started: false,
                                    x: p.x,
                                    y: p.y,
                                    ox: p.x,
                                    oy: p.y,
                                    target: None,
                                }));
                            }
                        },
                        onmousemove: {
                            let slot = c.slot;
                            move |_| {
                                // Only touch the signal when the target actually
                                // changes — a bare .write() would re-render on
                                // every mousemove.
                                let needs = matches!(
                                    &*drag.peek(),
                                    Some(d) if d.started && d.target != Some(slot)
                                );
                                if needs {
                                    drag.write().as_mut().unwrap().target = Some(slot);
                                }
                            }
                        },
                        div { class: "mname", "{c.name}" }
                        div { class: "mvalue", style: "color: {c.color};",
                            "{c.value}"
                            span { class: "munit", " {c.unit}" }
                        }
                        svg {
                            class: "spark",
                            view_box: "0 0 120 36",
                            preserve_aspect_ratio: "none",
                            polyline {
                                points: "{c.points}",
                                fill: "none",
                                stroke: "{c.color}",
                                stroke_width: "1.5",
                            }
                        }
                    }
                }
            }

            div { class: "chartsec",
                div { class: "charthead",
                    span { class: "chartname", style: "color: {sel_color};", "{sel_name}" }
                    span { class: "chartsub", " — last {HISTORY} samples" }
                }
                div {
                    class: "chartwrap",
                    // ResizeObserver-backed: fires once on mount and on every
                    // window resize, so the pixel-space chart stays exact.
                    onresize: move |evt| {
                        if let Ok(sz) = evt.data().get_content_box_size() {
                            chart_w.set(sz.width);
                        }
                    },
                    svg {
                        class: "mainchart",
                        width: "{w}",
                        height: "{CHART_H}",
                        onmousemove: move |evt| {
                            hover.set(Some(evt.element_coordinates().x));
                        },
                        onmouseleave: move |_| hover.set(None),
                        for (gy, gly, gv) in gridlines {
                            line {
                                x1: "0",
                                x2: "{w}",
                                y1: "{gy}",
                                y2: "{gy}",
                                stroke: "#202b45",
                                stroke_width: "1",
                            }
                            text {
                                x: "6",
                                y: "{gly}",
                                fill: "#566a94",
                                style: "font-size: 10px;",
                                "{gv}"
                            }
                        }
                        polyline {
                            points: "{main_pts}",
                            fill: "none",
                            stroke: "{sel_color}",
                            stroke_width: "2",
                            stroke_linejoin: "round",
                            stroke_linecap: "round",
                        }
                        if let Some((hx, hy)) = crosshair {
                            line {
                                x1: "{hx}",
                                x2: "{hx}",
                                y1: "0",
                                y2: "{CHART_H}",
                                stroke: "#4a5a80",
                                stroke_width: "1",
                            }
                            circle {
                                cx: "{hx}",
                                cy: "{hy}",
                                r: "3.5",
                                fill: "{sel_color}",
                                stroke: "#0e1320",
                                stroke_width: "1.5",
                            }
                        }
                    }
                    if let Some((tip, tx, ty)) = tooltip {
                        div { class: "tooltip", style: "left: {tx}px; top: {ty}px;", "{tip}" }
                    }
                }
            }

            if let Some((gx, gy, gname, gval, gcolor)) = ghost {
                div { class: "ghost", style: "left: {gx}px; top: {gy}px;",
                    div { class: "mname", "{gname}" }
                    div { class: "mvalue", style: "color: {gcolor}; font-size: 18px;", "{gval}" }
                }
            }
        }
    }
}

const CSS: &str = r#"
* { margin: 0; padding: 0; box-sizing: border-box; }
html, body { height: 100%; }
body {
  background: #0e1320; color: #e8ecf4; overflow: hidden;
  font-family: system-ui, -apple-system, sans-serif;
  -webkit-user-select: none; user-select: none;
}
.root { display: flex; flex-direction: column; gap: 12px; padding: 14px 16px; height: 100vh; }
.dragging, .dragging .card { cursor: grabbing !important; }
.toprow { display: flex; align-items: center; gap: 12px; }
h1 { font-size: 18px; letter-spacing: .4px; }
.hint { font-size: 11px; color: #5e6f92; }
.controls { display: flex; align-items: center; gap: 10px; margin-left: auto; }
.btn {
  background: #22304d; color: #e8ecf4; border: 1px solid #33456b; border-radius: 6px;
  padding: 5px 14px; font-size: 13px; cursor: pointer; transition: background .15s ease;
}
.btn:hover { background: #2e4066; }
input[type=range] { width: 150px; accent-color: #7aa2ff; }
.ratelbl { font-size: 13px; color: #9db0d0; min-width: 44px; font-variant-numeric: tabular-nums; }
.ticks { font-size: 11px; color: #5e6f92; font-variant-numeric: tabular-nums; }
.grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 10px; }
.card {
  background: #171f31; border: 1px solid #263354; border-radius: 10px;
  padding: 10px 12px 8px; cursor: grab;
  transition: transform .15s ease, box-shadow .15s ease, border-color .15s ease, opacity .15s ease;
}
/* Children never take pointer events, so mouse events (and their offset
   coordinates) always target the card element itself. */
.card * { pointer-events: none; }
.card:hover { transform: translateY(-2px); box-shadow: 0 8px 18px rgba(0,0,0,.4); }
.card.selected { border-color: #7aa2ff; box-shadow: 0 0 0 1px #7aa2ff inset; }
.card.drag-src { opacity: .35; }
.card.drop-target {
  border-color: #ffd166; box-shadow: 0 0 0 2px #ffd166 inset; transform: scale(1.02);
}
.mname { font-size: 11px; color: #9db0d0; text-transform: uppercase; letter-spacing: .8px; }
.mvalue { font-size: 24px; font-weight: 600; margin: 2px 0 4px; font-variant-numeric: tabular-nums; }
.munit { font-size: 12px; color: #7688ad; font-weight: 400; }
.spark { width: 100%; height: 36px; display: block; }
.chartsec { display: flex; flex-direction: column; gap: 6px; flex: 1; min-height: 0; }
.charthead { font-size: 13px; }
.chartname { font-weight: 600; }
.chartsub { color: #5e6f92; }
.chartwrap { position: relative; }
.mainchart {
  display: block; background: #121a2b; border: 1px solid #263354; border-radius: 10px;
}
.mainchart * { pointer-events: none; }
.tooltip {
  position: absolute; pointer-events: none; z-index: 20;
  background: rgba(10, 14, 24, .94); border: 1px solid #33456b; border-radius: 6px;
  padding: 5px 9px; font-size: 12px; white-space: nowrap; color: #dfe7f5;
}
.ghost {
  position: fixed; z-index: 99; pointer-events: none;
  background: #1d2740; border: 1px solid #3a4c75; border-radius: 10px;
  padding: 8px 14px; box-shadow: 0 14px 30px rgba(0,0,0,.55);
  transform: rotate(-2deg) scale(1.03); animation: ghost-pop .12s ease;
}
@keyframes ghost-pop { from { transform: rotate(0deg) scale(.9); opacity: .4; } }
"#;
