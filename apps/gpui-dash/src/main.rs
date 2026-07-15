//! "Pulse" — live metrics dashboard in gpui 0.2.2 (SPEC-2).
//!
//! gpui ships no widgets, but it has real interaction primitives:
//! - `on_drag` / `on_drop` / `drag_over` for the card reorder (native DnD),
//! - `canvas` + `PathBuilder` + `paint_path`/`paint_quad` for the charts,
//! - `cx.spawn` + `BackgroundExecutor::timer` for the 10 Hz tick,
//! - `with_animation` (per-frame re-style callback + easing fns) for animation.
//!
//! Everything else (slider, tooltip, buttons) is composed from styled `div()`s.
//!
//! Repaint strategy: `cx.notify()` per tick marks the window dirty; gpui then
//! re-renders the element tree and repaints the whole window scene once per
//! dirty frame (no partial damage). Hover/slider interactions notify per mouse
//! event; the oneshot drop-settle animation requests frames only while active.

use std::{cell::Cell, collections::VecDeque, rc::Rc, time::Duration};

use gpui::{
    Animation, AnimationExt as _, App, Application, Bounds, Context, DragMoveEvent, MouseButton,
    MouseDownEvent, MouseMoveEvent, Path, PathBuilder, Pixels, Point, SharedString,
    TitlebarOptions, Window, WindowBounds, WindowOptions, canvas, div, ease_in_out, fill, point,
    prelude::*, px, rgb, rgba, size,
};

/// `Pixels`' inner f32 is private in 0.2.2; go through `From<Pixels> for f32`.
fn fpx(p: Pixels) -> f32 {
    p.into()
}

const SPARK_WINDOW: usize = 60; // samples in the card sparkline
const CHART_WINDOW: usize = 300; // samples in the main chart (ring buffer cap)
const Y_MIN: f32 = 0.0;
const Y_MAX: f32 = 100.0;

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

struct Metric {
    name: &'static str,
    unit: &'static str,
    color: u32,
    vel: f32,
    samples: VecDeque<f32>,
}

impl Metric {
    fn new(name: &'static str, unit: &'static str, color: u32, start: f32) -> Self {
        let mut samples = VecDeque::with_capacity(CHART_WINDOW);
        samples.push_back(start);
        Self { name, unit, color, vel: 0.0, samples }
    }

    fn value(&self) -> f32 {
        *self.samples.back().unwrap_or(&0.0)
    }
}

/// Drag payload for card reordering (gpui dispatches drags by payload type).
#[derive(Clone)]
struct CardDrag {
    metric: usize,
}

/// Drag payload for the tick-rate slider thumb.
#[derive(Clone)]
struct HzDrag;

/// The floating drag preview gpui paints under the cursor during a card drag.
struct CardGhost {
    name: SharedString,
    value: String,
    color: u32,
}

impl Render for CardGhost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(220.))
            .p_3()
            .rounded_lg()
            .bg(rgba(0x1e293bee))
            .border_2()
            .border_color(rgb(self.color))
            .shadow_lg()
            .flex()
            .justify_between()
            .items_center()
            .text_sm()
            .text_color(rgb(0xe2e8f0))
            .child(self.name.clone())
            .child(div().text_color(rgb(self.color)).child(self.value.clone()))
    }
}

/// Invisible drag preview for the slider (we only want `on_drag_move` events).
struct NoGhost;

impl Render for NoGhost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

// ---------------------------------------------------------------------------
// App entity
// ---------------------------------------------------------------------------

struct PulseApp {
    metrics: Vec<Metric>,
    /// Grid slot -> metric index (drag-and-drop reorders this).
    order: Vec<usize>,
    selected: usize,
    paused: bool,
    hz: f32,
    tick_count: u64,
    rng: u64,
    /// Cursor position (window coords) while hovering the main chart.
    hover: Option<Point<Pixels>>,
    /// Main-chart canvas bounds, recorded during paint (for tooltip math).
    chart_bounds: Rc<Cell<Bounds<Pixels>>>,
    /// Slider track bounds, recorded by a canvas "bounds probe".
    slider_bounds: Rc<Cell<Bounds<Pixels>>>,
    /// Metric that just landed from a drag (drives the drop-settle animation).
    drop_anim: Option<usize>,
    drop_epoch: u64,
}

impl PulseApp {
    fn new(cx: &mut Context<Self>) -> Self {
        // The tick loop: an entity-owned task on the foreground executor that
        // sleeps on a background timer. Re-reads `hz`/`paused` every lap, so
        // the slider takes effect on the next tick without restarting the task.
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            loop {
                let Ok(hz) = this.update(cx, |t: &mut PulseApp, _| t.hz) else {
                    break; // entity dropped -> stop ticking
                };
                executor
                    .timer(Duration::from_secs_f64(1.0 / hz.clamp(1.0, 60.0) as f64))
                    .await;
                let alive = this.update(cx, |t, cx| {
                    if !t.paused {
                        t.tick();
                        cx.notify();
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();

        let mut app = Self {
            metrics: vec![
                Metric::new("CPU", "%", 0x38bdf8, 42.0),
                Metric::new("Memory", "%", 0xa78bfa, 63.0),
                Metric::new("Network In", "MB/s", 0x34d399, 25.0),
                Metric::new("Network Out", "MB/s", 0xfbbf24, 18.0),
                Metric::new("Disk", "%", 0xf472b6, 71.0),
                Metric::new("Requests", "/s", 0xf87171, 55.0),
            ],
            order: (0..6).collect(),
            selected: 0,
            paused: false,
            hz: 10.0,
            tick_count: 0,
            rng: 0x9e3779b97f4a7c15,
            hover: None,
            chart_bounds: Rc::new(Cell::new(Bounds::default())),
            slider_bounds: Rc::new(Cell::new(Bounds::default())),
            drop_anim: None,
            drop_epoch: 0,
        };
        // Pre-fill a few samples so the charts aren't empty at launch.
        for _ in 0..24 {
            app.tick();
        }
        app
    }

    /// xorshift64 — good enough for a synthetic random walk, no rand crate.
    fn next_rand(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        ((x >> 40) as f32) / ((1u64 << 24) as f32)
    }

    /// One smooth-random-walk step for every metric.
    fn tick(&mut self) {
        self.tick_count += 1;
        for i in 0..self.metrics.len() {
            let r = self.next_rand();
            let m = &mut self.metrics[i];
            m.vel = m.vel * 0.85 + (r - 0.5) * 2.4;
            let mut v = m.value() + m.vel;
            if !(1.0..=99.0).contains(&v) {
                v = v.clamp(1.0, 99.0);
                m.vel = -m.vel * 0.5;
            }
            if m.samples.len() >= CHART_WINDOW {
                m.samples.pop_front();
            }
            m.samples.push_back(v);
        }
    }

    /// Move the dragged metric into `target_slot` ("take that card's slot").
    fn reorder(&mut self, dragged: usize, target_slot: usize, cx: &mut Context<Self>) {
        let Some(from) = self.order.iter().position(|&m| m == dragged) else {
            return;
        };
        if from == target_slot {
            return;
        }
        let m = self.order.remove(from);
        self.order.insert(target_slot.min(self.order.len()), m);
        self.drop_epoch += 1;
        self.drop_anim = Some(dragged);
        cx.notify();
    }

    fn set_hz_from_x(&mut self, x: f32) {
        let b = self.slider_bounds.get();
        let w = fpx(b.size.width);
        if w <= 0.0 {
            return;
        }
        let frac = ((x - fpx(b.origin.x)) / w).clamp(0.0, 1.0);
        self.hz = (1.0 + frac * 59.0).round();
    }
}

// ---------------------------------------------------------------------------
// Chart geometry helpers (pure functions -> canvas paint closures)
// ---------------------------------------------------------------------------

/// Right-aligned polyline over a fixed sample window: the newest sample sits
/// at the right edge and the line scrolls left as data arrives.
fn polyline_path(
    samples: &[f32],
    bounds: Bounds<Pixels>,
    window_cap: usize,
    stroke: Pixels,
) -> Option<Path<Pixels>> {
    if samples.len() < 2 {
        return None;
    }
    let n = samples.len();
    let w = fpx(bounds.size.width);
    let h = fpx(bounds.size.height);
    let step = w / (window_cap.max(2) - 1) as f32;
    let x0 = fpx(bounds.origin.x) + w - step * (n - 1) as f32;
    let y = |v: f32| fpx(bounds.origin.y) + h - ((v - Y_MIN) / (Y_MAX - Y_MIN)).clamp(0.0, 1.0) * h;

    let mut b = PathBuilder::stroke(stroke);
    b.move_to(point(px(x0), px(y(samples[0]))));
    for (i, v) in samples.iter().enumerate().skip(1) {
        b.line_to(point(px(x0 + step * i as f32), px(y(*v))));
    }
    b.build().ok()
}

/// Nearest sample to cursor x. Returns (index, sample_x, sample_y) in window px.
fn hover_sample(
    samples: &[f32],
    bounds: Bounds<Pixels>,
    pos: Point<Pixels>,
) -> Option<(usize, f32, f32)> {
    if samples.len() < 2 {
        return None;
    }
    let n = samples.len();
    let w = fpx(bounds.size.width);
    let h = fpx(bounds.size.height);
    let step = w / (CHART_WINDOW - 1) as f32;
    let right = fpx(bounds.origin.x) + w;
    let back = (((right - fpx(pos.x)) / step).round().max(0.0) as usize).min(n - 1);
    let ix = n - 1 - back;
    let sx = right - step * back as f32;
    let sy =
        fpx(bounds.origin.y) + h - ((samples[ix] - Y_MIN) / (Y_MAX - Y_MIN)).clamp(0.0, 1.0) * h;
    Some((ix, sx, sy))
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

impl PulseApp {
    fn render_card(&self, slot: usize, metric_ix: usize, cx: &mut Context<Self>) -> gpui::AnyElement {
        let m = &self.metrics[metric_ix];
        let selected = self.selected == metric_ix;
        let color = m.color;
        let value = m.value();
        let n = m.samples.len();
        let spark: Vec<f32> = m
            .samples
            .iter()
            .skip(n.saturating_sub(SPARK_WINDOW))
            .copied()
            .collect();

        let ghost_name: SharedString = m.name.into();
        let ghost_value = format!("{value:.1}{}", m.unit);

        let card = div()
            .id(("card", metric_ix))
            .flex_1()
            .flex()
            .flex_col()
            .gap_1()
            .p_3()
            .rounded_lg()
            .bg(rgb(0x1e293b))
            .border_2()
            .border_color(if selected { rgb(color) } else { rgb(0x334155) })
            .cursor_pointer()
            .hover(|s| s.bg(rgb(0x243247)))
            // Slot indicator: highlight the card whose slot the drop will take.
            .drag_over::<CardDrag>(move |style, _, _, _| {
                style.border_color(rgb(0xe2e8f0)).bg(rgb(0x0c4a6e))
            })
            .on_drop(cx.listener(move |this, d: &CardDrag, _, cx| {
                this.reorder(d.metric, slot, cx);
            }))
            // Drag ghost: gpui paints this entity anchored at the grab offset.
            .on_drag(CardDrag { metric: metric_ix }, move |d, _grab, _, cx| {
                let (name, value, color) = (ghost_name.clone(), ghost_value.clone(), color);
                let _ = d;
                cx.new(|_| CardGhost { name, value, color })
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.selected = metric_ix;
                cx.notify();
            }))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(div().text_sm().text_color(rgb(0x94a3b8)).child(m.name))
                    .when(selected, |d| {
                        d.child(div().text_xs().text_color(rgb(color)).child("selected"))
                    }),
            )
            .child(
                div()
                    .text_2xl()
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(rgb(0xf1f5f9))
                    .child(format!("{value:.1} {}", m.unit)),
            )
            .child(div().h(px(36.)).w_full().child(canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    if let Some(path) = polyline_path(&spark, bounds, SPARK_WINDOW, px(1.5)) {
                        window.paint_path(path, rgb(color));
                    }
                },
            )
            .size_full()));

        // Drop-settle animation: when a card lands from a drag, fade it back in.
        // Keying the element id with `drop_epoch` restarts the oneshot animation.
        if self.drop_anim == Some(metric_ix) {
            card.with_animation(
                ("card-settle", self.drop_epoch as usize),
                Animation::new(Duration::from_millis(320)).with_easing(ease_in_out),
                |el, delta| el.opacity(0.3 + 0.7 * delta),
            )
            .into_any_element()
        } else {
            card.into_any_element()
        }
    }

    fn render_controls(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let paused = self.paused;
        let frac = ((self.hz - 1.0) / 59.0).clamp(0.0, 1.0);
        let track_w = 180.0_f32;
        let probe = self.slider_bounds.clone();

        div()
            .flex()
            .items_center()
            .gap_4()
            .child(
                div()
                    .id("pause")
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .bg(if paused { rgb(0x16a34a) } else { rgb(0x334155) })
                    .hover(|s| s.bg(rgb(0x475569)))
                    .cursor_pointer()
                    .text_color(rgb(0xf1f5f9))
                    .child(if paused { "Resume" } else { "Pause" })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.paused = !this.paused;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().text_sm().text_color(rgb(0x94a3b8)).child("Tick rate"))
                    .child(
                        div()
                            .id("hz-track")
                            .w(px(track_w))
                            .h(px(18.))
                            .relative()
                            .cursor_pointer()
                            // Bounds probe: a zero-cost canvas that records the
                            // track's window bounds for click-to-set math.
                            .child(
                                canvas(move |bounds, _, _| probe.set(bounds), |_, _, _, _| ())
                                    .absolute()
                                    .size_full(),
                            )
                            // rail
                            .child(
                                div()
                                    .absolute()
                                    .left_0()
                                    .top(px(7.))
                                    .w_full()
                                    .h(px(4.))
                                    .rounded_full()
                                    .bg(rgb(0x334155)),
                            )
                            // fill
                            .child(
                                div()
                                    .absolute()
                                    .left_0()
                                    .top(px(7.))
                                    .w(px(track_w * frac))
                                    .h(px(4.))
                                    .rounded_full()
                                    .bg(rgb(0x38bdf8)),
                            )
                            // thumb
                            .child(
                                div()
                                    .absolute()
                                    .left(px((track_w * frac - 7.0).max(0.0)))
                                    .top(px(2.))
                                    .size(px(14.))
                                    .rounded_full()
                                    .bg(rgb(0xe2e8f0))
                                    .shadow_sm(),
                            )
                            // Click anywhere on the track to jump the value.
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, ev: &MouseDownEvent, _, cx| {
                                    this.set_hz_from_x(fpx(ev.position.x));
                                    cx.notify();
                                }),
                            )
                            // Slider dragging = a gpui drag with an invisible
                            // ghost; `on_drag_move` streams cursor x + bounds.
                            .on_drag(HzDrag, |_, _, _, cx| cx.new(|_| NoGhost))
                            .on_drag_move::<HzDrag>(cx.listener(
                                |this, ev: &DragMoveEvent<HzDrag>, _, cx| {
                                    this.slider_bounds.set(ev.bounds);
                                    this.set_hz_from_x(fpx(ev.event.position.x));
                                    cx.notify();
                                },
                            )),
                    )
                    .child(
                        div()
                            .w(px(48.))
                            .text_sm()
                            .text_color(rgb(0xf1f5f9))
                            .child(format!("{:.0} Hz", self.hz)),
                    ),
            )
    }

    fn render_main_chart(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let sel = &self.metrics[self.selected];
        let color = sel.color;
        let unit = sel.unit;
        let data: Vec<f32> = sel.samples.iter().copied().collect();
        let hover = self.hover;
        let bounds_cell = self.chart_bounds.clone();

        // Tooltip contents are computed in render from last frame's bounds.
        let tooltip = hover.and_then(|pos| {
            let cb = self.chart_bounds.get();
            if !cb.contains(&pos) {
                return None;
            }
            let (ix, sx, sy) = hover_sample(&data, cb, pos)?;
            let t = self.tick_count - (data.len() - 1 - ix) as u64;
            let local_x = sx - fpx(cb.origin.x);
            let local_y = sy - fpx(cb.origin.y);
            let flip = local_x > fpx(cb.size.width) - 130.0;
            Some(
                div()
                    .absolute()
                    .left(px(if flip { local_x - 128.0 } else { local_x + 12.0 }))
                    .top(px((local_y - 34.0).max(2.0)))
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .bg(rgb(0x0f172a))
                    .border_1()
                    .border_color(rgb(0x475569))
                    .shadow_md()
                    .text_xs()
                    .text_color(rgb(0xe2e8f0))
                    .child(format!("t={t} · {:.1} {unit}", data[ix])),
            )
        });

        let chart_data = data.clone();
        div()
            .flex_1()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .rounded_lg()
            .bg(rgb(0x1e293b))
            .border_1()
            .border_color(rgb(0x334155))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(div().size(px(10.)).rounded_full().bg(rgb(color)))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(0x94a3b8))
                                    .child(format!("{} — last {} samples", sel.name, data.len())),
                            ),
                    )
                    .child(
                        div()
                            .text_lg()
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(rgb(color))
                            .child(format!("{:.1} {unit}", sel.value())),
                    ),
            )
            .child(
                div()
                    .id("chart")
                    .relative()
                    .flex_1()
                    .w_full()
                    .child(
                        canvas(
                            |_, _, _| (),
                            move |bounds, _, window, _| {
                                bounds_cell.set(bounds);
                                // horizontal grid lines at 0/25/50/75/100
                                for k in 0..=4 {
                                    let yy = bounds.origin.y
                                        + bounds.size.height * (k as f32 / 4.0)
                                        - px(if k == 4 { 1.0 } else { 0.0 });
                                    window.paint_quad(fill(
                                        Bounds {
                                            origin: point(bounds.origin.x, yy),
                                            size: size(bounds.size.width, px(1.)),
                                        },
                                        rgba(0x33415588),
                                    ));
                                }
                                if let Some(path) =
                                    polyline_path(&chart_data, bounds, CHART_WINDOW, px(2.0))
                                {
                                    window.paint_path(path, rgb(color));
                                }
                                // crosshair + marker under the cursor
                                if let Some(pos) = hover {
                                    if bounds.contains(&pos) {
                                        if let Some((_, sx, sy)) =
                                            hover_sample(&chart_data, bounds, pos)
                                        {
                                            window.paint_quad(fill(
                                                Bounds {
                                                    origin: point(px(sx), bounds.origin.y),
                                                    size: size(px(1.), bounds.size.height),
                                                },
                                                rgba(0x94a3b8aa),
                                            ));
                                            window.paint_quad(fill(
                                                Bounds {
                                                    origin: point(bounds.origin.x, px(sy)),
                                                    size: size(bounds.size.width, px(1.)),
                                                },
                                                rgba(0x94a3b8aa),
                                            ));
                                            window.paint_quad(fill(
                                                Bounds {
                                                    origin: point(px(sx - 3.0), px(sy - 3.0)),
                                                    size: size(px(6.), px(6.)),
                                                },
                                                rgb(0xf1f5f9),
                                            ));
                                        }
                                    }
                                }
                            },
                        )
                        .size_full(),
                    )
                    .children(tooltip)
                    .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _, cx| {
                        this.hover = Some(ev.position);
                        cx.notify();
                    }))
                    .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                        if !*hovered {
                            this.hover = None;
                            cx.notify();
                        }
                    })),
            )
    }
}

impl Render for PulseApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Live indicator: pulse driven by the data tick itself (no repeating
        // `with_animation`, which would force a repaint every display frame).
        let live_alpha = if self.paused {
            0.25
        } else {
            let phase = (self.tick_count % 16) as f32 / 16.0;
            0.35 + 0.65 * (phase * std::f32::consts::TAU).sin().abs()
        };

        let mut rows: Vec<gpui::AnyElement> = Vec::new();
        let order = self.order.clone();
        for (r, chunk) in order.chunks(3).enumerate() {
            let mut cards: Vec<gpui::AnyElement> = Vec::new();
            for (c, &metric_ix) in chunk.iter().enumerate() {
                cards.push(self.render_card(r * 3 + c, metric_ix, cx));
            }
            rows.push(div().flex().gap_3().children(cards).into_any_element());
        }

        div()
            .size_full()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .bg(rgb(0x0f172a))
            .text_color(rgb(0xe2e8f0))
            .text_sm()
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_xl()
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .child("Pulse"),
                            )
                            .child(
                                div()
                                    .size(px(10.))
                                    .rounded_full()
                                    .bg(rgb(0x34d399))
                                    .opacity(live_alpha),
                            )
                            .child(div().text_xs().text_color(rgb(0x64748b)).child(
                                if self.paused { "paused" } else { "live" },
                            )),
                    )
                    .child(self.render_controls(cx)),
            )
            .children(rows)
            .child(self.render_main_chart(cx))
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(900.), px(640.)), cx);

        // gpui apps do not quit when the last window closes unless told to.
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        cx.open_window(
            WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some(SharedString::from("Pulse (gpui)")),
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(PulseApp::new),
        )
        .unwrap();
        cx.activate(true);
    });
}
