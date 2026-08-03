//! "Pulse" — live metrics dashboard (SPEC-2), floem git main @ 778bb5f2.
//!
//! Architecture notes (research-relevant):
//! - Fine-grained reactivity: each metric's value and sample history live in
//!   `RwSignal`s. The view tree is built once; a `canvas` paint closure that
//!   reads a signal is auto-tracked and repainted when that signal changes —
//!   there is no per-frame loop and no manual damage/cache management (the
//!   iced port needed 7 explicit `canvas::Cache`s for the same effect).
//! - Repaint model: floem only repaints views whose tracked signals changed.
//!   The 10 Hz sampler is a self-re-arming `exec_after` chain (floem has no
//!   interval/subscription primitive — the upstream timer example uses this
//!   exact Effect + exec_after + trigger pattern).
//! - Drag-and-drop reorder is BUILT IN: `.draggable_with_config()` starts a
//!   drag with an automatic ghost (the view is re-painted at the cursor),
//!   `dragging_style` styles that ghost, `DragTargetEnter` events carry the
//!   source's custom data for live grid reflow, and releasing animates the
//!   ghost with a configurable spring — none of this is hand-rolled here.

use std::time::Duration;

use floem::Application;
use floem::action::exec_after;
use floem::event::DragConfig;
use floem::kurbo::{BezPath, Circle, Line, Point, Rect, Size, Stroke};
use floem::paint::PaintCx;
use floem::prelude::*;
use floem::reactive::Effect;
use floem::style::Transition;
use floem::taffy::style::FlexWrap;
use floem::text::{Attrs, AttrsList, TextLayout};
use floem::views::slider::{Slider, SliderChanged};
use floem::window::WindowConfig;

const HISTORY: usize = 300; // samples kept per metric (main chart)
const SPARK: usize = 60; // samples shown in each card sparkline
const CARD_HEIGHT: f64 = 130.0;

const BG_PANEL: Color = Color::from_rgb8(0xf4, 0xf4, 0xf6);
const BG_CARD: Color = Color::from_rgb8(0xea, 0xea, 0xee);
const BORDER: Color = Color::from_rgb8(0xc9, 0xc9, 0xd2);
const ACCENT: Color = Color::from_rgb8(0x3b, 0x6f, 0xe0);
const TEXT_DIM: Color = Color::from_rgb8(0x70, 0x70, 0x7a);
const TEXT_MAIN: Color = Color::from_rgb8(0x20, 0x20, 0x28);

/// Static metadata + reactive state of one metric. `RwSignal` is `Copy`,
/// so the whole struct is freely copied into event/paint closures.
#[derive(Clone, Copy)]
struct Metric {
    name: &'static str,
    unit: &'static str,
    max: f64,
    color: Color,
    value: RwSignal<f64>,
    samples: RwSignal<Vec<f64>>,
}

impl Metric {
    fn new(name: &'static str, unit: &'static str, max: f64, color: Color, start: f64) -> Self {
        Self {
            name,
            unit,
            max,
            color,
            value: RwSignal::new(start),
            samples: RwSignal::new(vec![start]),
        }
    }

    /// One step of a smooth, mean-reverting random walk.
    fn step(&self, rng: &RwSignal<u64>) {
        let jitter = (next_f64(rng) - 0.5) * self.max * 0.04;
        let value = self.value.get_untracked();
        let reversion = (self.max * 0.5 - value) * 0.01;
        let value = (value + jitter + reversion).clamp(0.0, self.max);

        self.value.set(value);
        self.samples.update(|s| {
            if s.len() == HISTORY {
                s.remove(0);
            }
            s.push(value);
        });
    }

    fn display(&self) -> String {
        format!("{:.1}{}", self.value.get(), self.unit)
    }
}

/// Tiny xorshift* PRNG — avoids pulling in `rand` for synthetic data.
fn next_f64(state: &RwSignal<u64>) -> f64 {
    let mut x = state.get_untracked();
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    state.set(x);
    (x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 40) as f64 / (1u64 << 24) as f64
}

fn main() {
    Application::new()
        .window(
            |_| app_view(),
            Some(
                WindowConfig::default()
                    .title("Pulse (floem)")
                    .size(Size::new(900.0, 640.0)),
            ),
        )
        .run();
}

fn app_view() -> impl IntoView {
    let metrics: [Metric; 6] = [
        Metric::new("CPU", "%", 100.0, Color::from_rgb8(0x4f, 0x9c, 0xf5), 35.0),
        Metric::new("Memory", "%", 100.0, Color::from_rgb8(0xa7, 0x7b, 0xf3), 62.0),
        Metric::new("Net In", " MB/s", 400.0, Color::from_rgb8(0x3f, 0xc0, 0x8a), 120.0),
        Metric::new("Net Out", " MB/s", 400.0, Color::from_rgb8(0xe8, 0xa3, 0x3d), 80.0),
        Metric::new("Disk", " MB/s", 200.0, Color::from_rgb8(0xe2, 0x63, 0x7f), 40.0),
        Metric::new("Requests", " r/s", 1000.0, Color::from_rgb8(0x38, 0xb6, 0xd0), 420.0),
    ];

    let order: RwSignal<Vec<usize>> = RwSignal::new((0..metrics.len()).collect());
    let selected = RwSignal::new(0usize);
    let running = RwSignal::new(true);
    let hz = RwSignal::new(10.0f64);
    let ticks = RwSignal::new(1u64);
    let rng = RwSignal::new(0x9E37_79B9_7F4A_7C15u64);

    // The 10 Hz (configurable) sampler: a self-re-arming exec_after chain.
    // The Effect re-runs whenever `tick` fires; the delay is re-read from
    // `hz` each round so slider changes apply on the next tick.
    let tick = RwSignal::new(());
    Effect::new(move |_| {
        tick.track();
        let delay = Duration::from_secs_f64(1.0 / hz.get_untracked().max(1.0));
        exec_after(delay, move |_| {
            if running.get_untracked() {
                for metric in &metrics {
                    metric.step(&rng);
                }
                ticks.update(|t| *t += 1);
            }
            tick.set(());
        });
    });

    let controls = Stack::horizontal((
        Label::new("Pulse").style(|s| s.font_size(22.0).color(TEXT_MAIN)),
        Empty::new().style(|s| s.flex_grow(1.0)),
        Button::new(Label::derived(move || {
            if running.get() { "Pause" } else { "Resume" }
        }))
        .action(move || running.update(|r| *r = !*r))
        .style(|s| s.padding(6.0).padding_horiz(14.0)),
        Label::derived(move || format!("{:.0} Hz", hz.get()))
            .style(|s| s.font_size(14.0).width(50.0)),
        Slider::new_ranged(move || hz.get(), 1.0..=60.0)
            .step(1.0)
            .on_event_stop(SliderChanged::listener(), move |_, changed| {
                hz.set(changed.value.clamp(1.0, 60.0));
            })
            .style(|s| s.width(220.0)),
    ))
    .style(|s| s.gap(12.0).items_center().width_full());

    // Card grid: 2 rows × 3 columns via flex wrap; `dyn_stack` keyed by
    // metric index so reordering the signal reorders (not rebuilds) views.
    let cards = dyn_stack(
        move || order.get(),
        |mi| *mi,
        move |mi| card(metrics[mi], mi, order, selected),
    )
    .style(|s| {
        s.flex_row()
            .flex_wrap(FlexWrap::Wrap)
            .gap(12.0)
            .width_full()
    });

    let hover: RwSignal<Option<Point>> = RwSignal::new(None);

    let chart_header = Stack::horizontal((
        Label::derived(move || metrics[selected.get()].name)
            .style(|s| s.font_size(15.0).color(TEXT_MAIN)),
        Label::derived(move || metrics[selected.get()].display())
            .style(move |s| s.font_size(15.0).color(metrics[selected.get()].color)),
        Empty::new().style(|s| s.flex_grow(1.0)),
        Label::new(format!("last {HISTORY} samples — hover for details"))
            .style(|s| s.font_size(12.0).color(TEXT_DIM)),
    ))
    .style(|s| s.gap(12.0).items_center().width_full());

    // The paint closure reads `selected`, the samples signal, `ticks` and
    // `hover`; floem's SignalTracker repaints the canvas when any change.
    let chart = canvas(move |cx, size| {
        let metric = metrics[selected.get()];
        let samples = metric.samples.get();
        draw_chart(cx, size, &metric, &samples, ticks.get(), hover.get());
    })
    .on_event_cont(listener::PointerMove, move |_, update| {
        hover.set(Some(update.current.logical_point()));
    })
    .on_event_cont(listener::PointerLeave, move |_, _| hover.set(None))
    .style(|s| s.flex_grow(1.0).width_full());

    let chart_panel = Stack::vertical((chart_header, chart)).style(|s| {
        s.flex_col()
            .gap(8.0)
            .padding(12.0)
            .flex_grow(1.0)
            .width_full()
            .background(BG_PANEL)
            .border(1.0)
            .border_color(BORDER)
            .border_radius(10.0)
    });

    Stack::vertical((controls, cards, chart_panel))
        .style(|s| s.flex_col().gap(14.0).padding(14.0).size_full())
}

/// One metric card: click selects, drag reorders (built-in floem DnD).
fn card(
    metric: Metric,
    mi: usize,
    order: RwSignal<Vec<usize>>,
    selected: RwSignal<usize>,
) -> impl IntoView {
    Stack::vertical((
        Label::new(metric.name).style(|s| s.font_size(13.0).color(TEXT_DIM)),
        Label::derived(move || metric.display()).style(|s| s.font_size(26.0).color(TEXT_MAIN)),
        canvas(move |cx, size| {
            let samples = metric.samples.get();
            draw_sparkline(cx, size, &samples, metric.max, metric.color);
        })
        .style(|s| s.height(34.0).width_full()),
    ))
    .style(move |s| {
        let is_selected = selected.get() == mi;
        s.flex_col()
            .gap(4.0)
            .padding(12.0)
            // 3 columns: (900 - 2*14 padding - 2*12 gaps) / 3 ≈ 282, as %.
            .width_pct(31.9)
            .height(CARD_HEIGHT)
            .border_radius(10.0)
            .background(BG_CARD)
            .border(if is_selected { 2.0 } else { 1.0 })
            .border_color(if is_selected { ACCENT } else { BORDER })
            // Deliberate animation #1: hover "elevation" via box-shadow +
            // background, tweened by style transitions (200 ms linear).
            .transition_background(Transition::linear(200.millis()))
            .hover(|s| {
                s.background(Color::from_rgb8(0xf0, 0xf0, 0xf4))
                    .box_shadow_blur(12.0)
                    .box_shadow_color(Color::BLACK.with_alpha(0.25))
            })
    })
    // Ghost styling while dragging (floem re-paints the view at the cursor).
    .dragging_style(|s| {
        s.box_shadow_blur(18.0)
            .box_shadow_color(Color::BLACK.with_alpha(0.4))
            .border_color(ACCENT)
            .border(1.0)
            .border_radius(10.0)
            .background(BG_CARD.with_alpha(0.92))
    })
    .on_event_stop(listener::Click, move |_, _| selected.set(mi))
    // Live reflow: when a dragged card enters this card's slot, move it
    // here — the vacated/reflowed grid is the drop indicator.
    .on_event_stop(listener::DragTargetEnter, move |_, drag_enter| {
        if let Some(data) = &drag_enter.custom_data
            && let Some(dragged) = data.downcast_ref::<usize>()
            && *dragged != mi
        {
            order.update(|order| {
                let from = order.iter().position(|m| m == dragged).unwrap();
                let to = order.iter().position(|m| *m == mi).unwrap();
                order.remove(from);
                order.insert(to, *dragged);
            });
        }
    })
    // Deliberate animation #2 (built-in): drag release springs the ghost
    // back/into place with a configurable easing.
    .draggable_with_config(move || {
        DragConfig::default()
            .with_threshold(8.0)
            .with_custom_data(mi)
            .with_easing(floem::easing::Spring::snappy())
    })
}

// ---------------------------------------------------------------------------
// Canvas drawing (kurbo paths through floem's Renderer)
// ---------------------------------------------------------------------------

fn draw_sparkline(cx: &mut PaintCx, size: Size, samples: &[f64], max: f64, color: Color) {
    let n = samples.len().min(SPARK);
    if n < 2 {
        return;
    }
    let (w, h) = (size.width, size.height);
    let step = w / (SPARK - 1) as f64;
    let start_x = w - (n - 1) as f64 * step;

    let mut path = BezPath::new();
    for (i, value) in samples[samples.len() - n..].iter().enumerate() {
        let x = start_x + i as f64 * step;
        let y = h - (value / max) * (h - 2.0) - 1.0;
        if i == 0 {
            path.move_to(Point::new(x, y));
        } else {
            path.line_to(Point::new(x, y));
        }
    }
    cx.stroke(&path, color, &Stroke::new(1.5));
}

fn chart_sample_y(value: f64, max: f64, height: f64) -> f64 {
    let usable = height - 4.0;
    usable - (value / max) * (usable - 8.0)
}

fn draw_text(cx: &mut PaintCx, text: &str, size: f32, color: Color, at: Point) -> Size {
    let attrs = Attrs::new().color(color).font_size(size);
    let layout = TextLayout::new_with_text(text, AttrsList::new(attrs), None);
    let text_size = layout.size();
    layout.draw(cx, at);
    text_size
}

fn draw_chart(
    cx: &mut PaintCx,
    size: Size,
    metric: &Metric,
    samples: &[f64],
    ticks: u64,
    hover: Option<Point>,
) {
    let (w, h) = (size.width, size.height);
    let n = samples.len().min(HISTORY);
    let step = w / (HISTORY - 1) as f64;
    let start_x = w - (n - 1) as f64 * step;

    // Horizontal gridlines + labels at 0 / 25 / 50 / 75 / 100 %.
    for i in 0..=4 {
        let value = metric.max * i as f64 / 4.0;
        let y = chart_sample_y(value, metric.max, h);
        cx.stroke(
            &Line::new(Point::new(0.0, y), Point::new(w, y)),
            BORDER.with_alpha(0.6),
            &Stroke::new(1.0),
        );
        let text = format!("{value:.0}{}", metric.unit);
        draw_text(cx, &text, 10.0, TEXT_DIM.with_alpha(0.7), Point::new(4.0, y - 14.0));
    }

    if n < 2 {
        return;
    }

    // The metric line.
    let mut line = BezPath::new();
    for (i, value) in samples[samples.len() - n..].iter().enumerate() {
        let point = Point::new(start_x + i as f64 * step, chart_sample_y(*value, metric.max, h));
        if i == 0 {
            line.move_to(point);
        } else {
            line.line_to(point);
        }
    }
    cx.stroke(&line, metric.color, &Stroke::new(2.0));

    // Crosshair + tooltip overlay while hovered.
    let Some(hover) = hover else { return };
    if hover.x < start_x - step || hover.x < 0.0 || hover.x > w || hover.y < 0.0 || hover.y > h {
        return;
    }

    let index = (((hover.x - start_x) / step).round().max(0.0) as usize).min(n - 1);
    let value = samples[samples.len() - n + index];
    let x = start_x + index as f64 * step;
    let y = chart_sample_y(value, metric.max, h);

    let crosshair = TEXT_MAIN.with_alpha(0.35);
    cx.stroke(
        &Line::new(Point::new(x, 0.0), Point::new(x, h)),
        crosshair,
        &Stroke::new(1.0),
    );
    cx.stroke(
        &Line::new(Point::new(0.0, y), Point::new(w, y)),
        crosshair,
        &Stroke::new(1.0),
    );
    cx.fill(&Circle::new(Point::new(x, y), 3.5), metric.color, 0.0);

    // Tooltip: "<value>  ·  #<absolute sample> (t-<age>)". Text is measured
    // with a real TextLayout (no char-count estimation needed).
    let age = n - 1 - index;
    let absolute = ticks.saturating_sub(age as u64);
    let text = format!("{:.1}{}  ·  #{} (t-{})", value, metric.unit, absolute, age);

    let attrs = Attrs::new().color(TEXT_MAIN).font_size(12.0);
    let layout = TextLayout::new_with_text(&text, AttrsList::new(attrs), None);
    let text_size = layout.size();

    let tip = Size::new(text_size.width + 12.0, 24.0);
    let at = Point::new((x + 12.0).min(w - tip.width - 4.0), (y - 34.0).max(4.0));
    cx.fill(
        &Rect::from_origin_size(at, tip).to_rounded_rect(5.0),
        Color::WHITE.with_alpha(0.95),
        0.0,
    );
    layout.draw(
        cx,
        Point::new(at.x + 6.0, at.y + (tip.height - text_size.height) / 2.0),
    );
}
