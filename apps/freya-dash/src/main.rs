//! "Pulse" — live metrics dashboard on Freya 0.4 (SPEC-2).
//!
//! Idiomatic Freya: the UI is a tree of builder-style elements rebuilt by
//! `render`, reactivity comes from `use_state` signals, drag-and-drop uses the
//! built-in `DragZone`/`DropZone` components, and the sparklines / main chart
//! are drawn with the `canvas()` element straight onto the Skia canvas.

use std::{
    collections::VecDeque,
    time::Duration,
};

use async_io::Timer;
use freya::{
    animation::*,
    engine::prelude::{
        Paint,
        PaintStyle,
        PathBuilder,
        SkColor,
        SkRect,
    },
    prelude::*,
};

const SPARK_SAMPLES: usize = 60;
const CHART_SAMPLES: usize = 300;
const CARD_W: f32 = 280.;
const CARD_H: f32 = 108.;

const BG: Color = Color::from_argb(255, 22, 24, 29);
const PANEL: Color = Color::from_argb(255, 32, 35, 42);
const PANEL_HOVER: Color = Color::from_argb(255, 40, 44, 53);
const TEXT: Color = Color::from_argb(255, 226, 229, 236);
const MUTED: Color = Color::from_argb(255, 138, 146, 162);
const ACCENT: Color = Color::from_argb(255, 122, 162, 247);
const GRID: Color = Color::from_argb(255, 52, 57, 68);

fn main() {
    launch(
        LaunchConfig::new().with_window(
            WindowConfig::new(app)
                .with_title("Pulse (freya)")
                .with_size(900.0, 640.0)
                .with_background(BG),
        ),
    )
}

// ---------------------------------------------------------------- data model

/// One metric: a name, a unit, a colour and a rolling window of samples.
#[derive(Clone, PartialEq)]
struct Metric {
    name: &'static str,
    unit: &'static str,
    color: Color,
    samples: VecDeque<f64>,
    min: f64,
    max: f64,
    rng: u64,
}

impl Metric {
    fn new(name: &'static str, unit: &'static str, color: Color, min: f64, max: f64, seed: u64) -> Self {
        let start = (min + max) / 2.0;
        Self {
            name,
            unit,
            color,
            samples: VecDeque::from(vec![start; CHART_SAMPLES]),
            min,
            max,
            rng: seed,
        }
    }

    /// Deterministic xorshift64* — avoids a `rand` dependency for a random walk.
    fn next_unit(&mut self) -> f64 {
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        (self.rng.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Smooth random walk clamped into the metric's range.
    fn tick(&mut self) {
        let last = *self.samples.back().unwrap_or(&0.0);
        let mid = (self.min + self.max) / 2.0;
        let step = (self.next_unit() - 0.5) * (self.max - self.min) * 0.08;
        // Mild mean reversion keeps the walk off the clamps.
        let next = (last + step + (mid - last) * 0.01).clamp(self.min, self.max);
        self.samples.push_back(next);
        while self.samples.len() > CHART_SAMPLES {
            self.samples.pop_front();
        }
    }

    fn current(&self) -> f64 {
        *self.samples.back().unwrap_or(&0.0)
    }

    fn tail(&self, n: usize) -> Vec<f64> {
        let skip = self.samples.len().saturating_sub(n);
        self.samples.iter().skip(skip).copied().collect()
    }
}

fn seed_metrics() -> Vec<Metric> {
    vec![
        Metric::new("CPU", "%", Color::from_argb(255, 122, 162, 247), 0., 100., 0x1234_5678),
        Metric::new("Memory", "%", Color::from_argb(255, 158, 206, 106), 0., 100., 0x2345_6789),
        Metric::new("Network In", "MB/s", Color::from_argb(255, 224, 175, 104), 0., 120., 0x3456_789a),
        Metric::new("Network Out", "MB/s", Color::from_argb(255, 187, 154, 247), 0., 120., 0x4567_89ab),
        Metric::new("Disk", "MB/s", Color::from_argb(255, 125, 207, 255), 0., 500., 0x5678_9abc),
        Metric::new("Requests", "rps", Color::from_argb(255, 247, 118, 142), 0., 2000., 0x6789_abcd),
    ]
}

// ---------------------------------------------------------------- root

fn app() -> impl IntoElement {
    let mut metrics = use_state(seed_metrics);
    // Display order holds metric indices; drag-and-drop permutes this, never the data.
    let mut order = use_state(|| (0..6).collect::<Vec<usize>>());
    let mut selected = use_state(|| 0usize);
    let mut paused = use_state(|| false);
    let mut rate_hz = use_state(|| 10.0f64);
    let mut drop_target = use_state(|| None::<usize>);

    // Live data. `spawn` attaches the task to this (root) scope; `Timer` comes
    // from async-io because Freya's executor ships no interval primitive.
    use_hook(move || {
        spawn(async move {
            loop {
                let period = Duration::from_secs_f64(1.0 / rate_hz.peek().max(1.0));
                Timer::after(period).await;
                if *paused.peek() {
                    continue;
                }
                for metric in metrics.write().iter_mut() {
                    metric.tick();
                }
            }
        });
    });

    let hz = *rate_hz.read();
    let is_paused = *paused.read();
    let sel = *selected.read();
    let target = *drop_target.read();

    let rows: Vec<Element> = order
        .read()
        .chunks(3)
        .map(|chunk| {
            let cells: Vec<Element> = chunk
                .iter()
                .map(|&id| {
                    let metric = metrics.read()[id].clone();
                    let card = MetricCard {
                        id,
                        name: metric.name,
                        unit: metric.unit,
                        color: metric.color,
                        value: metric.current(),
                        spark: metric.tail(SPARK_SAMPLES),
                        min: metric.min,
                        max: metric.max,
                        selected: sel == id,
                        drop_hint: target == Some(id),
                        on_select: EventHandler::new(move |_| selected.set(id)),
                    };

                    DropZone::new(
                        DragZone::new(id, card.clone())
                            .drag_element(
                                rect()
                                    .width(Size::px(CARD_W))
                                    .height(Size::px(CARD_H))
                                    .opacity(0.85)
                                    .child(card),
                            )
                            .show_while_dragging(true),
                        move |dragged: usize| {
                            drop_target.set(None);
                            reorder(&mut order, dragged, id);
                        },
                    )
                    .on_drag_over(move |over: bool| {
                        if over {
                            drop_target.set(Some(id));
                        } else if *drop_target.peek() == Some(id) {
                            drop_target.set(None);
                        }
                    })
                    .key(id)
                    .into()
                })
                .collect();

            rect().horizontal().spacing(10.).children(cells).into()
        })
        .collect();

    let selected_metric = metrics.read()[sel].clone();

    rect()
        .expanded()
        .background(BG)
        .color(TEXT)
        .padding(Gaps::new_all(12.))
        .spacing(12.)
        .child(
            // ------------------------------------------------ controls row
            rect()
                .horizontal()
                .spacing(12.)
                .cross_align(Alignment::Center)
                .height(Size::px(38.))
                .child(
                    Button::new()
                        .on_press(move |_| paused.toggle())
                        .child(if is_paused { "Resume" } else { "Pause" }),
                )
                .child(
                    label()
                        .text(format!("{hz:.0} Hz"))
                        .width(Size::px(56.))
                        .font_size(14.)
                        .color(MUTED),
                )
                .child(
                    rect().width(Size::px(240.)).child(
                        Slider::new(move |percent: f64| {
                            rate_hz.set(1.0 + percent / 100.0 * 59.0);
                        })
                        .value((hz - 1.0) / 59.0 * 100.0),
                    ),
                )
                .child(
                    label()
                        .text("drag a card to reorder · click to select")
                        .font_size(13.)
                        .color(MUTED),
                ),
        )
        .children(rows)
        .child(
            // ------------------------------------------------ main chart
            MainChart {
                name: selected_metric.name,
                unit: selected_metric.unit,
                color: selected_metric.color,
                min: selected_metric.min,
                max: selected_metric.max,
                samples: selected_metric.tail(CHART_SAMPLES),
            },
        )
}

/// Move `dragged` so that it takes the slot currently occupied by `onto`.
fn reorder(order: &mut State<Vec<usize>>, dragged: usize, onto: usize) {
    if dragged == onto {
        return;
    }
    let mut list = order.write();
    let Some(from) = list.iter().position(|&x| x == dragged) else {
        return;
    };
    let Some(to) = list.iter().position(|&x| x == onto) else {
        return;
    };
    let value = list.remove(from);
    list.insert(to, value);
}

// ---------------------------------------------------------------- metric card

#[derive(Clone, PartialEq)]
struct MetricCard {
    id: usize,
    name: &'static str,
    unit: &'static str,
    color: Color,
    value: f64,
    spark: Vec<f64>,
    min: f64,
    max: f64,
    selected: bool,
    drop_hint: bool,
    on_select: EventHandler<Event<PressEventData>>,
}

impl Component for MetricCard {
    fn render(&self) -> impl IntoElement {
        let mut hovering = use_state(|| false);

        // Deliberate animation: hover elevation. `use_animation` + `OnChange::Rerun`
        // re-runs the tween whenever the dependency signal read inside it changes.
        let lift = use_animation(move |conf| {
            conf.on_change(OnChange::Rerun);
            AnimNum::new(0., if hovering() { 1. } else { 0. })
                .time(160)
                .ease(Ease::Out)
                .function(Function::Quad)
        });
        let t = lift.get().value();

        let spark = self.spark.clone();
        let (min, max) = (self.min, self.max);
        let line = SkColor::from(self.color);
        let border_color = if self.selected {
            self.color
        } else if self.drop_hint {
            ACCENT
        } else {
            Color::from_argb(255, 48, 52, 63)
        };

        rect()
            .width(Size::px(CARD_W))
            .height(Size::px(CARD_H))
            .padding(Gaps::new_all(10.))
            .spacing(2.)
            .background(Color::lerp(PANEL, PANEL_HOVER, t))
            .rounded_lg()
            .border(
                Border::new()
                    .fill(border_color)
                    .width(if self.selected || self.drop_hint { 2.0 } else { 1.0 }),
            )
            .shadow(
                Shadow::new()
                    .y(2. + 4. * t)
                    .blur(6. + 10. * t)
                    .color(Color::from_argb((60. + 60. * t) as u8, 0, 0, 0)),
            )
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| hovering.set(false))
            .on_press(self.on_select.clone())
            .child(
                label()
                    .text(self.name)
                    .font_size(13.)
                    .color(MUTED),
            )
            .child(
                rect()
                    .horizontal()
                    .cross_align(Alignment::End)
                    .spacing(4.)
                    .child(
                        label()
                            .text(format_value(self.value))
                            .font_size(26.)
                            .color(TEXT),
                    )
                    .child(label().text(self.unit).font_size(12.).color(MUTED)),
            )
            .child(
                live_canvas(RenderCallback::new(move |ctx: &mut CanvasContext| {
                    draw_sparkline(ctx, &spark, min, max, line);
                }))
                .width(Size::fill())
                .height(Size::fill())
                .key(self.id),
            )
    }
}

fn format_value(v: f64) -> String {
    if v >= 100.0 {
        format!("{v:.0}")
    } else {
        format!("{v:.1}")
    }
}

// ---------------------------------------------------------------- main chart

#[derive(Clone, PartialEq)]
struct MainChart {
    name: &'static str,
    unit: &'static str,
    color: Color,
    min: f64,
    max: f64,
    samples: Vec<f64>,
}

impl Component for MainChart {
    fn render(&self) -> impl IntoElement {
        // Hover crosshair state: the cursor position inside the plot, in logical px.
        let mut cursor = use_state(|| None::<(f32, f32)>);
        let mut area = use_state(|| (0.0f32, 0.0f32));

        let samples = self.samples.clone();
        let (min, max) = (self.min, self.max);
        let line = SkColor::from(self.color);
        let hover = *cursor.read();
        let (w, h) = *area.read();

        // Snap the crosshair to the nearest sample so the tooltip reports a real value.
        let picked = hover.and_then(|(x, _)| {
            if samples.is_empty() || w <= 1.0 {
                return None;
            }
            let frac = (x / w).clamp(0.0, 1.0);
            let idx = ((frac * (samples.len() - 1) as f32).round() as usize).min(samples.len() - 1);
            Some((idx, samples[idx]))
        });

        let crosshair_x = hover.map(|(x, _)| x);
        let plot_samples = samples.clone();

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .background(PANEL)
            .rounded_lg()
            .padding(Gaps::new_all(10.))
            .spacing(6.)
            .child(
                rect()
                    .horizontal()
                    .spacing(8.)
                    .cross_align(Alignment::Center)
                    .child(label().text(self.name).font_size(14.).color(TEXT))
                    .child(
                        label()
                            .text(format!("last {} samples · {}", self.samples.len(), self.unit))
                            .font_size(12.)
                            .color(MUTED),
                    ),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::fill())
                    .child(
                        live_canvas(RenderCallback::new(move |ctx: &mut CanvasContext| {
                            draw_chart(ctx, &plot_samples, min, max, line, crosshair_x);
                        }))
                        .width(Size::fill())
                        .height(Size::fill())
                        .on_sized(move |e: Event<SizedEventData>| {
                            let a = e.data().area;
                            area.set((a.width(), a.height()));
                        })
                        .on_pointer_move(move |e: Event<PointerEventData>| {
                            let p = e.data().element_location();
                            cursor.set(Some((p.x as f32, p.y as f32)));
                        })
                        .on_pointer_leave(move |_| cursor.set(None)),
                    )
                    // Tooltip is a real element (Freya's canvas gives no text
                    // measurement, so drawing it in Skia would mean hand-rolling
                    // a paragraph); it is positioned over the plot.
                    .maybe_child(picked.zip(hover).map(|((idx, value), (x, y))| {
                        let tw = 118.0f32;
                        let left = (x + 12.0).min((w - tw).max(0.0));
                        let top = (y - 44.0).clamp(0.0, (h - 48.0).max(0.0));
                        rect()
                            .position(Position::new_absolute().left(left).top(top))
                            .layer(Layer::Overlay)
                            .interactive(false)
                            .width(Size::px(tw))
                            .padding(Gaps::new_symmetric(6., 8.))
                            .background(Color::from_argb(235, 16, 18, 22))
                            .rounded_sm()
                            .border(Border::new().fill(GRID).width(1.0))
                            .child(
                                label()
                                    .text(format!("#{idx}"))
                                    .font_size(11.)
                                    .color(MUTED),
                            )
                            .child(
                                label()
                                    .text(format!("{:.2} {}", value, self.unit))
                                    .font_size(13.)
                                    .color(TEXT),
                            )
                    })),
            )
    }
}

// ---------------------------------------------------------------- skia drawing

/// Build a `canvas()` that actually repaints when its captured data changes.
///
/// Freya's `RenderCallback` implements `PartialEq` as "always equal", so a
/// canvas whose only changing input is the closure it captured diffs as
/// *unchanged*: the old element stays in the tree and keeps painting the very
/// first closure forever (observed: sparklines frozen at their seed values).
/// Event-handler callbacks, by contrast, always compare *unequal*, so attaching
/// one no-op handler is enough to make the element be replaced every render.
/// See FRICTION.md — this is the single sharpest edge found in Freya 0.4.
fn live_canvas(on_render: RenderCallback) -> Canvas {
    canvas(on_render).on_wheel(|_| {})
}

fn draw_sparkline(ctx: &mut CanvasContext, samples: &[f64], min: f64, max: f64, color: SkColor) {
    let (w, h) = (ctx.size.width, ctx.size.height);
    if samples.len() < 2 || w <= 0.0 || h <= 0.0 {
        return;
    }

    // Sparklines auto-scale to the visible window (padded by 8% of the metric's
    // full range) so a 38 px strip still shows the shape of the walk.
    let pad = (max - min) * 0.08;
    let lo = samples.iter().cloned().fold(f64::MAX, f64::min).max(min) - pad;
    let hi = samples.iter().cloned().fold(f64::MIN, f64::max).min(max) + pad;
    let span = (hi - lo).max(f64::EPSILON);
    let point = |i: usize| {
        let x = i as f32 / (samples.len() - 1) as f32 * w;
        let y = h - ((samples[i] - lo) / span) as f32 * h;
        (x, y.clamp(0.0, h))
    };

    // Skia 0.98 (the version behind freya-skia-safe) builds paths through
    // `PathBuilder`; `Path` itself is immutable.
    let mut builder = PathBuilder::new();
    let (x0, y0) = point(0);
    builder.move_to((x0, y0));
    for i in 1..samples.len() {
        let (x, y) = point(i);
        builder.line_to((x, y));
    }
    let path = builder.snapshot();

    // Filled area under the curve, then the stroke on top.
    let fill_path = builder.line_to((w, h)).line_to((0.0, h)).close().detach();

    let mut fill = Paint::default();
    fill.set_anti_alias(true);
    fill.set_style(PaintStyle::Fill);
    fill.set_color(color.with_a(48));
    ctx.canvas.draw_path(&fill_path, &fill);

    let mut stroke = Paint::default();
    stroke.set_anti_alias(true);
    stroke.set_style(PaintStyle::Stroke);
    stroke.set_stroke_width(1.5);
    stroke.set_color(color);
    ctx.canvas.draw_path(&path, &stroke);
}

fn draw_chart(
    ctx: &mut CanvasContext,
    samples: &[f64],
    min: f64,
    max: f64,
    color: SkColor,
    crosshair_x: Option<f32>,
) {
    let (w, h) = (ctx.size.width, ctx.size.height);
    if w <= 0.0 || h <= 0.0 {
        return;
    }

    // Horizontal gridlines.
    let mut grid = Paint::default();
    grid.set_anti_alias(false);
    grid.set_style(PaintStyle::Stroke);
    grid.set_stroke_width(1.0);
    grid.set_color(SkColor::from(GRID));
    for i in 0..=4 {
        let y = h * i as f32 / 4.0;
        ctx.canvas
            .draw_rect(SkRect::new(0.0, y, w, y + 0.6), &grid);
    }

    if samples.len() >= 2 {
        let span = (max - min).max(f64::EPSILON);
        let mut builder = PathBuilder::new();
        for (i, value) in samples.iter().enumerate() {
            let x = i as f32 / (samples.len() - 1) as f32 * w;
            let y = (h - ((value - min) / span) as f32 * h).clamp(0.0, h);
            if i == 0 {
                builder.move_to((x, y));
            } else {
                builder.line_to((x, y));
            }
        }
        let path = builder.snapshot();
        let fill_path = builder.line_to((w, h)).line_to((0.0, h)).close().detach();

        let mut fill = Paint::default();
        fill.set_anti_alias(true);
        fill.set_style(PaintStyle::Fill);
        fill.set_color(color.with_a(38));
        ctx.canvas.draw_path(&fill_path, &fill);

        let mut stroke = Paint::default();
        stroke.set_anti_alias(true);
        stroke.set_style(PaintStyle::Stroke);
        stroke.set_stroke_width(2.0);
        stroke.set_color(color);
        ctx.canvas.draw_path(&path, &stroke);

        // Crosshair + marker snapped to the nearest sample.
        if let Some(cx) = crosshair_x {
            let frac = (cx / w).clamp(0.0, 1.0);
            let idx = ((frac * (samples.len() - 1) as f32).round() as usize).min(samples.len() - 1);
            let x = idx as f32 / (samples.len() - 1) as f32 * w;
            let y = (h - ((samples[idx] - min) / span) as f32 * h).clamp(0.0, h);

            let mut cross = Paint::default();
            cross.set_anti_alias(false);
            cross.set_style(PaintStyle::Fill);
            cross.set_color(SkColor::from(MUTED).with_a(150));
            ctx.canvas.draw_rect(SkRect::new(x - 0.5, 0.0, x + 0.5, h), &cross);

            let mut dot = Paint::default();
            dot.set_anti_alias(true);
            dot.set_style(PaintStyle::Fill);
            dot.set_color(color);
            ctx.canvas.draw_circle((x, y), 3.5, &dot);
        }
    }
}
