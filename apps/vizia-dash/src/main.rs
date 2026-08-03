//! "Pulse" — live metrics dashboard (SPEC-2), vizia 0.4.
//!
//! Architecture notes (research-relevant):
//! - **Live data**: `cx.add_timer(interval, None, cb)` is a first-class core
//!   API — no executor feature, no subscription plumbing. The tick rate is
//!   changed in place with `cx.modify_timer(t, |s| s.set_interval(..))`, and
//!   pause/resume is `cx.stop_timer` / `cx.start_timer`.
//! - **Charts**: vizia renders with Skia, and `vizia::vg` re-exports
//!   `skia-safe`, so a custom `View::draw(&self, cx, canvas)` hands you the
//!   *same* `skia_safe::Canvas` the framework draws itself with. Sparklines
//!   and the main chart are `View` impls; text is left to real `Label`s
//!   overlaid absolutely rather than drawn on the canvas.
//! - **Drag & drop**: built into core. `.on_drag(|ex| ex.set_drop_data(..))`
//!   arms a drag once the mouse leaves a pressed DRAGGABLE view, and
//!   `.on_drop(|ex, data| ..)` fires on release over a target;
//!   `ex.has_drop_data()` drives the slot highlight. The only hand-rolled
//!   part is the ghost that follows the cursor.
//! - **Animation**: CSS. `transition: <prop> <ms>` in a stylesheet, driven by
//!   vizia's animation system on pseudo-class/`toggle_class` changes — no
//!   per-frame callbacks and no manual `Instant` threading.

use std::collections::VecDeque;

use vizia::prelude::*;
use vizia::vg;

const SPARK_LEN: usize = 60; // samples in a card sparkline
const CHART_LEN: usize = 300; // samples in the main chart
const CARDS: usize = 6;

fn main() -> Result<(), ApplicationError> {
    Application::new(|cx| {
        cx.add_stylesheet(STYLE).expect("failed to add stylesheet");

        let metrics: Vec<Metric> = METRIC_DEFS
            .iter()
            .enumerate()
            .map(|(index, (name, unit, color, seed, (floor, ceiling)))| {
                // Pre-roll the walk so the chart opens with real history
                // instead of a flat line.
                let mut walker = Walker { state: *seed, value: (floor + ceiling) / 2.0 };
                let history: VecDeque<f32> =
                    (0..CHART_LEN).map(|_| walker.next(*floor, *ceiling)).collect();
                let current = *history.back().unwrap();
                Metric {
                    index,
                    name,
                    unit,
                    color: *color,
                    samples: Signal::new(history),
                    current: Signal::new(current),
                    walker: Signal::new(walker),
                }
            })
            .collect();

        let order = Signal::new((0..CARDS).collect::<Vec<usize>>());
        let selected = Signal::new(0usize);
        let paused = Signal::new(false);
        let hz = Signal::new(10.0f32);
        let hover = Signal::new(None::<Hover>);
        let drag_from = Signal::new(None::<usize>);
        let drag_over = Signal::new(None::<usize>);
        let cursor = Signal::new((0.0f32, 0.0f32));

        // 10 Hz by default. `None` = repeat forever.
        let timer = cx.add_timer(Duration::from_millis(100), None, |cx, action| {
            if let TimerAction::Tick(_) = action {
                cx.emit(DashEvent::Tick);
            }
        });

        Dash {
            metrics: metrics.clone(),
            order,
            selected,
            paused,
            hz,
            hover,
            drag_from,
            drag_over,
            cursor,
            timer,
        }
        .build(cx);

        cx.start_timer(timer);

        let hz_label = Memo::new(move |_| format!("{:.0} Hz", hz.get()));
        let pause_label = Memo::new(move |_| {
            if paused.get() { "Resume".to_string() } else { "Pause".to_string() }
        });

        VStack::new(cx, |cx| {
            // ---------------- controls row ----------------
            HStack::new(cx, |cx| {
                Button::new(cx, |cx| Label::new(cx, pause_label))
                    .variant(ButtonVariant::Primary)
                    .class("pause")
                    .on_press(|cx| cx.emit(DashEvent::TogglePause));

                Label::new(cx, "tick rate").class("dim");

                // 1..60 Hz. `Slider` is normalised 0..1, so map both ways.
                Slider::new(cx, hz.map(|value| (*value - 1.0) / 59.0))
                    .on_change(|cx, value| {
                        cx.emit(DashEvent::SetRate((value * 59.0 + 1.0).round()))
                    })
                    .width(Pixels(220.0));

                Label::new(cx, hz_label).class("hz");

                Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));

                Label::new(cx, "drag a card to reorder · click to select").class("dim");
            })
            .class("controls");

            // ---------------- card grid ----------------
            // Two rows of three. Rebuilt whenever `order` changes, which is
            // what makes the grid "reflow" after a drop.
            Binding::new(cx, order, {
                let metrics = metrics.clone();
                move |cx| {
                    let order_now = order.get();
                    VStack::new(cx, |cx| {
                        for row in 0..2 {
                            HStack::new(cx, |cx| {
                                for column in 0..3 {
                                    let slot = row * 3 + column;
                                    card(cx, slot, metrics[order_now[slot]], selected, drag_over);
                                }
                            })
                            .class("card-row");
                        }
                    })
                    .class("card-grid");
                }
            });

            // ---------------- main chart ----------------
            let selected_name = {
                let metrics = metrics.clone();
                Memo::new(move |_| {
                    let metric = metrics[selected.get()];
                    format!("{} ({})", metric.name, metric.unit)
                })
            };

            VStack::new(cx, |cx| {
                Label::new(cx, selected_name).class("chart-title");

                ZStack::new(cx, |cx| {
                    MainChart::new(cx, metrics.clone(), selected, hover)
                        .width(Stretch(1.0))
                        .height(Stretch(1.0))
                        .on_mouse_move(move |cx, x, y| {
                            let bounds = cx.bounds();
                            cx.emit(DashEvent::ChartHover(x - bounds.x, y - bounds.y));
                        })
                        .on_hover_out(|cx| cx.emit(DashEvent::ChartLeave));

                    // Tooltip: a real view positioned over the canvas, so the
                    // text uses the same shaping/AA as the rest of the UI.
                    VStack::new(cx, |cx| {
                        Label::new(
                            cx,
                            hover.map(|h| {
                                h.map_or(String::new(), |h| format!("{:.1}", h.value))
                            }),
                        )
                        .class("tip-value");
                        Label::new(
                            cx,
                            hover.map(|h| {
                                h.map_or(String::new(), |h| format!("sample {}", h.index))
                            }),
                        )
                        .class("tip-index");
                    })
                    .class("tooltip")
                    .hoverable(false)
                    .position_type(PositionType::Absolute)
                    .left(hover.map(|h| Pixels(h.map_or(0.0, |h| h.tip_x))))
                    .top(hover.map(|h| Pixels(h.map_or(0.0, |h| h.tip_y))))
                    .display(hover.map(|h| {
                        if h.is_some() { Display::Flex } else { Display::None }
                    }));
                })
                .class("chart-area");
            })
            .class("chart-panel");

            // ---------------- drag ghost ----------------
            // Hand-rolled: vizia arms and routes the drag for us but does not
            // render a preview, so this is an absolutely positioned card-ish
            // element chased by the root's `on_mouse_move`.
            let ghost_label = {
                let metrics = metrics.clone();
                Memo::new(move |_| {
                    drag_from
                        .get()
                        .map(|slot| metrics[order.get()[slot]].name.to_string())
                        .unwrap_or_default()
                })
            };
            Label::new(cx, ghost_label)
                .class("ghost")
                .hoverable(false)
                .position_type(PositionType::Absolute)
                .left(cursor.map(|c| Pixels(c.0 + 12.0)))
                .top(cursor.map(|c| Pixels(c.1 + 12.0)))
                .display(drag_from.map(|d| {
                    if d.is_some() { Display::Flex } else { Display::None }
                }));
        })
        .class("dash")
        .on_mouse_move(|cx, x, y| cx.emit(DashEvent::Cursor(x, y)));
    })
    .title("Pulse (vizia)")
    .inner_size((900, 640))
    .run()
}

// ---------------------------------------------------------------------------
// Cards
// ---------------------------------------------------------------------------

fn card(
    cx: &mut Context,
    slot: usize,
    metric: Metric,
    selected: Signal<usize>,
    drag_over: Signal<Option<usize>>,
) {
    let value_text = Memo::new(move |_| format!("{:.1}", metric.current.get()));
    let index = metric.index;

    VStack::new(cx, |cx| {
        // Every child is `hoverable(false)` so the *card* stays the hover
        // target: vizia's `on_press` only fires when `cx.current ==
        // meta.target`, i.e. when the pressed view is itself the hovered
        // entity — a click landing on a child label would otherwise never
        // reach the card's press/drag handlers.
        Label::new(cx, metric.name).class("card-name").hoverable(false);
        HStack::new(cx, |cx| {
            Label::new(cx, value_text).class("card-value").hoverable(false);
            Label::new(cx, metric.unit).class("card-unit").hoverable(false);
        })
        .class("card-value-row")
        .hoverable(false);
        Sparkline::new(cx, metric).class("spark").hoverable(false);
    })
    .class("card")
    .toggle_class("selected", selected.map(move |s| *s == index))
    .toggle_class("drop-target", drag_over.map(move |d| *d == Some(slot)))
    .on_press(move |cx| cx.emit(DashEvent::Select(index)))
    // Built-in DnD: marks the view DRAGGABLE and fires once the pointer
    // leaves the pressed card. The payload is this card's entity id, but the
    // app only needs "a drag is in flight from slot N", so it also stashes
    // the slot in the model.
    .on_drag(move |cx| {
        cx.set_drop_data(cx.current());
        cx.emit(DashEvent::DragStart(slot));
    })
    // Drop indicator: fires continuously while the pointer is over this card.
    .on_over(move |cx| {
        if cx.has_drop_data() {
            cx.emit(DashEvent::DragOver(slot));
        }
    })
    .on_drop(move |cx, _data| cx.emit(DashEvent::Drop(slot)));
}

// ---------------------------------------------------------------------------
// Custom views (Skia)
// ---------------------------------------------------------------------------

/// Sparkline of the last `SPARK_LEN` samples of one metric.
struct Sparkline {
    metric: Metric,
}

impl Sparkline {
    fn new(cx: &mut Context, metric: Metric) -> Handle<'_, Self> {
        Self { metric }
            .build(cx, |_| {})
            // Redraw only when this metric's own buffer changes.
            .bind(metric.samples, |mut handle| handle.needs_redraw())
    }
}

impl View for Sparkline {
    fn element(&self) -> Option<&'static str> {
        Some("sparkline")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        let samples = self.metric.samples.get();
        let start = samples.len().saturating_sub(SPARK_LEN);
        let window: Vec<f32> = samples.iter().skip(start).copied().collect();
        if window.len() < 2 {
            return;
        }

        let (lo, hi) = extent(&window);
        // skia-safe 0.93 `Path` is immutable: geometry is accumulated in a
        // `PathBuilder` and snapshotted.
        let mut builder = vg::PathBuilder::new();
        for (i, value) in window.iter().enumerate() {
            let x = bounds.x + bounds.w * (i as f32 / (window.len() - 1) as f32);
            let y = bounds.y + bounds.h * (1.0 - (value - lo) / (hi - lo));
            if i == 0 {
                builder.move_to((x, y));
            } else {
                builder.line_to((x, y));
            }
        }
        let path = builder.snapshot();

        // Filled area under the line, then the line itself.
        builder.line_to((bounds.x + bounds.w, bounds.y + bounds.h));
        builder.line_to((bounds.x, bounds.y + bounds.h));
        builder.close();
        let area = builder.detach();

        let (r, g, b) = self.metric.color;
        let mut fill = vg::Paint::default();
        fill.set_anti_alias(true);
        fill.set_color(vg::Color::from_argb(46, r, g, b));
        canvas.draw_path(&area, &fill);

        let mut stroke = vg::Paint::default();
        stroke.set_anti_alias(true);
        stroke.set_style(vg::paint::Style::Stroke);
        stroke.set_stroke_width(1.5);
        stroke.set_color(vg::Color::from_argb(255, r, g, b));
        canvas.draw_path(&path, &stroke);
    }
}

/// The big chart: last `CHART_LEN` samples of the selected metric plus a
/// crosshair + snapped marker at the hovered sample.
struct MainChart {
    metrics: Vec<Metric>,
    selected: Signal<usize>,
    hover: Signal<Option<Hover>>,
}

impl MainChart {
    fn new(
        cx: &mut Context,
        metrics: Vec<Metric>,
        selected: Signal<usize>,
        hover: Signal<Option<Hover>>,
    ) -> Handle<'_, Self> {
        let sample_signals: Vec<Signal<VecDeque<f32>>> =
            metrics.iter().map(|m| m.samples).collect();
        let mut handle = Self { metrics, selected, hover }.build(cx, |_| {});
        for signal in sample_signals {
            handle = handle.bind(signal, |mut handle| handle.needs_redraw());
        }
        handle
            .bind(selected, |mut handle| handle.needs_redraw())
            .bind(hover, |mut handle| handle.needs_redraw())
    }
}

impl View for MainChart {
    fn element(&self) -> Option<&'static str> {
        Some("mainchart")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        let metric = self.metrics[self.selected.get()];
        let samples: Vec<f32> = metric.samples.get().iter().copied().collect();
        if samples.len() < 2 {
            return;
        }
        let (lo, hi) = extent(&samples);

        // Gridlines (5 horizontal bands).
        let mut grid = vg::Paint::default();
        grid.set_anti_alias(false);
        grid.set_style(vg::paint::Style::Stroke);
        grid.set_stroke_width(1.0);
        grid.set_color(vg::Color::from_argb(40, 128, 128, 128));
        for i in 0..=4 {
            let y = bounds.y + bounds.h * (i as f32 / 4.0);
            canvas.draw_line((bounds.x, y), (bounds.x + bounds.w, y), &grid);
        }

        let x_at = |i: usize| bounds.x + bounds.w * (i as f32 / (samples.len() - 1) as f32);
        let y_at = |v: f32| bounds.y + bounds.h * (1.0 - (v - lo) / (hi - lo));

        let mut builder = vg::PathBuilder::new();
        for (i, value) in samples.iter().enumerate() {
            let (x, y) = (x_at(i), y_at(*value));
            if i == 0 {
                builder.move_to((x, y));
            } else {
                builder.line_to((x, y));
            }
        }
        let path = builder.detach();

        let (r, g, b) = metric.color;
        let mut stroke = vg::Paint::default();
        stroke.set_anti_alias(true);
        stroke.set_style(vg::paint::Style::Stroke);
        stroke.set_stroke_width(2.0);
        stroke.set_color(vg::Color::from_argb(255, r, g, b));
        canvas.draw_path(&path, &stroke);

        // Crosshair + marker at the hovered (snapped) sample.
        if let Some(hover) = self.hover.get() {
            let (x, y) = (x_at(hover.index), y_at(hover.value));
            let mut cross = vg::Paint::default();
            cross.set_style(vg::paint::Style::Stroke);
            cross.set_stroke_width(1.0);
            cross.set_color(vg::Color::from_argb(150, 150, 150, 150));
            canvas.draw_line((x, bounds.y), (x, bounds.y + bounds.h), &cross);
            canvas.draw_line((bounds.x, y), (bounds.x + bounds.w, y), &cross);

            let mut dot = vg::Paint::default();
            dot.set_anti_alias(true);
            dot.set_color(vg::Color::from_argb(255, r, g, b));
            canvas.draw_circle((x, y), 4.0, &dot);
        }
    }
}

fn extent(values: &[f32]) -> (f32, f32) {
    let lo = values.iter().copied().fold(f32::INFINITY, f32::min);
    let hi = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let pad = ((hi - lo) * 0.1).max(0.5);
    (lo - pad, hi + pad)
}

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Metric {
    index: usize,
    name: &'static str,
    unit: &'static str,
    color: (u8, u8, u8),
    samples: Signal<VecDeque<f32>>,
    current: Signal<f32>,
    walker: Signal<Walker>,
}

/// Smooth random walk: xorshift* for the step, exponential pull to the mean.
#[derive(Clone, Copy, PartialEq)]
struct Walker {
    state: u64,
    value: f32,
}

impl Walker {
    fn next(&mut self, floor: f32, ceiling: f32) -> f32 {
        self.state ^= self.state >> 12;
        self.state ^= self.state << 25;
        self.state ^= self.state >> 27;
        // Top 24 bits of the scrambled state -> uniform noise in [-0.5, 0.5].
        let noise =
            ((self.state.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 40) as f32 / 16_777_216.0) - 0.5;
        let mean = (floor + ceiling) / 2.0;
        self.value += noise * (ceiling - floor) * 0.06 + (mean - self.value) * 0.04;
        self.value = self.value.clamp(floor, ceiling);
        self.value
    }
}

#[derive(Clone, Copy, PartialEq)]
struct Hover {
    index: usize,
    value: f32,
    tip_x: f32,
    tip_y: f32,
}

struct Dash {
    metrics: Vec<Metric>,
    order: Signal<Vec<usize>>,
    selected: Signal<usize>,
    paused: Signal<bool>,
    hz: Signal<f32>,
    hover: Signal<Option<Hover>>,
    drag_from: Signal<Option<usize>>,
    drag_over: Signal<Option<usize>>,
    cursor: Signal<(f32, f32)>,
    timer: Timer,
}

enum DashEvent {
    Tick,
    TogglePause,
    SetRate(f32),
    Select(usize),
    ChartHover(f32, f32),
    ChartLeave,
    Cursor(f32, f32),
    DragStart(usize),
    DragOver(usize),
    Drop(usize),
    DragEnd,
}

impl Model for Dash {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|dash_event, _| match dash_event {
            DashEvent::Tick => {
                for metric in &self.metrics {
                    let (floor, ceiling) = METRIC_DEFS[metric.index].4;
                    let mut walker = metric.walker.get();
                    let value = walker.next(floor, ceiling);
                    metric.walker.set(walker);
                    metric.current.set(value);
                    metric.samples.update(|samples| {
                        samples.push_back(value);
                        while samples.len() > CHART_LEN {
                            samples.pop_front();
                        }
                    });
                }
            }

            DashEvent::TogglePause => {
                let paused = !self.paused.get();
                self.paused.set(paused);
                if paused {
                    cx.stop_timer(self.timer);
                } else {
                    cx.start_timer(self.timer);
                }
            }

            DashEvent::SetRate(hz) => {
                let hz = hz.clamp(1.0, 60.0);
                self.hz.set(hz);
                let interval = Duration::from_micros((1_000_000.0 / hz) as u64);
                // Changing the interval of a live timer in place — no
                // teardown/rebuild of a subscription.
                cx.modify_timer(self.timer, |state| {
                    state.set_interval(interval);
                });
            }

            DashEvent::Select(index) => self.selected.set(*index),

            DashEvent::ChartHover(x, y) => {
                let metric = self.metrics[self.selected.get()];
                let samples = metric.samples.get();
                if samples.len() < 2 {
                    return;
                }
                let bounds = cx.bounds();
                let ratio = (x / bounds.w.max(1.0)).clamp(0.0, 1.0);
                let index = (ratio * (samples.len() - 1) as f32).round() as usize;
                let value = samples[index];
                // Trap: `on_mouse_move` and `cx.bounds()` are in PHYSICAL
                // pixels, but `left`/`top` in `Pixels(..)` are logical, so
                // the tooltip position has to be divided by the scale factor
                // (2.0 on this Retina display) or it lands twice as far out.
                let scale = cx.scale_factor();
                let (width, height) = (bounds.w / scale, bounds.h / scale);
                let (lx, ly) = (x / scale, y / scale);
                // Flip the tooltip when it would run off the right edge.
                let tip_x = if lx > width - 110.0 { lx - 104.0 } else { lx + 14.0 };
                let tip_y = (ly - 10.0).clamp(0.0, (height - 46.0).max(0.0));
                self.hover.set(Some(Hover { index, value, tip_x, tip_y }));
            }

            DashEvent::ChartLeave => self.hover.set(None),

            DashEvent::Cursor(x, y) => {
                if self.drag_from.get().is_some() {
                    let scale = cx.scale_factor();
                    self.cursor.set((x / scale, y / scale));
                }
            }

            DashEvent::DragStart(slot) => {
                self.drag_from.set(Some(*slot));
                let scale = cx.scale_factor();
                self.cursor
                    .set((cx.mouse().cursor_x / scale, cx.mouse().cursor_y / scale));
            }

            DashEvent::DragOver(slot) => {
                if self.drag_from.get().is_some() {
                    self.drag_over.set(Some(*slot));
                }
            }

            DashEvent::Drop(target) => {
                if let Some(source) = self.drag_from.get() {
                    if source != *target {
                        self.order.update(|order| {
                            let card = order.remove(source);
                            order.insert(*target, card);
                        });
                    }
                }
            }

            // Ordering matters: `on_drop` runs while `WindowEvent::MouseUp`
            // is still propagating from the card up to this model, and it
            // *queues* `Drop`. If the model cleared `drag_from` directly in
            // the MouseUp arm, the queued `Drop` would then see `None` and
            // silently do nothing. Queuing `DragEnd` instead keeps the order
            // Drop -> DragEnd.
            DashEvent::DragEnd => {
                self.drag_from.set(None);
                self.drag_over.set(None);
            }
        });

        // Any mouse release ends a drag, even one released outside a card.
        event.map(|window_event, _| {
            if let WindowEvent::MouseUp(MouseButton::Left) = window_event {
                if self.drag_from.get().is_some() {
                    cx.emit(DashEvent::DragEnd);
                }
            }
        });
    }
}

/// name, unit, rgb, PRNG seed, (floor, ceiling)
const METRIC_DEFS: [(&str, &str, (u8, u8, u8), u64, (f32, f32)); CARDS] = [
    ("CPU", "%", (0x4f, 0x9d, 0xf7), 0x9E37_79B9_7F4A_7C15, (2.0, 98.0)),
    ("Memory", "%", (0x7a, 0xc7, 0x4f), 0xBF58_476D_1CE4_E5B9, (30.0, 92.0)),
    ("Network In", "MB/s", (0xf2, 0xa0, 0x3d), 0x94D0_49BB_1331_11EB, (0.0, 120.0)),
    ("Network Out", "MB/s", (0xe0, 0x63, 0x63), 0xD1B5_4A32_D192_ED03, (0.0, 80.0)),
    ("Disk", "MB/s", (0xb2, 0x7c, 0xe8), 0xA24B_AED4_963E_E407, (0.0, 500.0)),
    ("Requests", "rps", (0x3f, 0xc1, 0xc9), 0x9FB2_1C65_1E98_DF25, (100.0, 4000.0)),
];

// ---------------------------------------------------------------------------
// Style — vizia has a real CSS engine, including `transition`, so every
// animation in this app is declarative.
// ---------------------------------------------------------------------------

const STYLE: &str = r#"
.dash {
    width: 1s;
    height: 1s;
    padding: 12px;
    vertical-gap: 10px;
}

.controls {
    height: auto;
    horizontal-gap: 12px;
    alignment: center;
}

.controls .pause { min-width: 88px; }
.dim { color: #8a8a8a; font-size: 13px; height: auto; }
.hz { min-width: 56px; font-size: 13px; height: auto; }

.card-grid { height: auto; vertical-gap: 10px; }
.card-row { height: auto; horizontal-gap: 10px; }

.card {
    width: 1s;
    height: 118px;
    padding: 10px;
    vertical-gap: 2px;
    background-color: #ffffff10;
    border-width: 1px;
    border-color: #ffffff20;
    corner-radius: 8px;
    /* The deliberate animation: hover elevation + selection/drop feedback
       are pure CSS transitions driven by vizia's animation system. */
    transition: background-color 180ms, border-color 180ms, scale 180ms, shadow 180ms;
}

.card:hover {
    background-color: #ffffff1c;
    scale: 1.02;
    shadow: 0px 4px 12px #00000055;
}

.card.selected {
    border-color: #4f9df7;
    background-color: #4f9df722;
}

.card.drop-target {
    border-color: #f2a03d;
    background-color: #f2a03d33;
    scale: 1.04;
}

.card-name { font-size: 12px; color: #9a9a9a; height: auto; }
.card-value-row { height: auto; horizontal-gap: 4px; alignment: bottom left; }
.card-value { font-size: 26px; height: auto; }
.card-unit { font-size: 12px; color: #9a9a9a; height: auto; padding-bottom: 4px; }
.spark { width: 1s; height: 1s; }

.chart-panel {
    height: 1s;
    vertical-gap: 6px;
    padding: 10px;
    background-color: #ffffff10;
    border-width: 1px;
    border-color: #ffffff20;
    corner-radius: 8px;
}

.chart-title { font-size: 14px; height: auto; }
.chart-area { width: 1s; height: 1s; }

.tooltip {
    width: 96px;
    height: auto;
    padding: 5px;
    background-color: #1c1c1ee8;
    border-width: 1px;
    border-color: #ffffff33;
    corner-radius: 5px;
}

.tip-value { font-size: 13px; height: auto; }
.tip-index { font-size: 11px; color: #9a9a9a; height: auto; }

.ghost {
    width: 130px;
    height: 34px;
    padding: 8px;
    background-color: #4f9df7cc;
    corner-radius: 6px;
    font-size: 12px;
    shadow: 0px 6px 16px #00000077;
}
"#;
