//! "Pulse" — live metrics dashboard (SPEC-2), iced 0.14.
//!
//! Architecture notes (research-relevant):
//! - Elm architecture: `Pulse` state + `Message` + `update` + `view`.
//!   We use `iced::application::timed` so `update` receives the current
//!   `Instant`, which drives the built-in `iced::Animation` API.
//! - Repaint model: iced only repaints when a message arrives. The 10 Hz
//!   sampler is an `iced::time::every` subscription; chart geometry lives in
//!   `canvas::Cache`s that are cleared exactly once per tick. Between ticks
//!   nothing repaints unless (a) a hover animation is in flight (then we
//!   subscribe to `window::frames()`), (b) the user interacts, or (c) the
//!   chart crosshair moves (canvas-local `Action::request_redraw`).
//! - Drag-and-drop is hand-rolled: `mouse_area` per card gives press/enter
//!   messages, a global `event::listen_with` subscription (only active while
//!   dragging) tracks the cursor and the button release, and the ghost is a
//!   `pin` inside a root `stack` following the cursor.

use std::collections::VecDeque;
use std::time::Duration;

use iced::alignment;
use iced::event;
use iced::mouse;
use iced::time::{self, Instant};
use iced::widget::{
    button, canvas, column, container, float, mouse_area, pin, row, slider,
    space, stack, text,
};
use iced::window;
use iced::{
    Animation, Border, Center, Color, Element, Event, Fill, Point, Rectangle,
    Renderer, Shadow, Subscription, Task, Theme, Vector,
};

const HISTORY: usize = 300; // samples kept per metric (main chart)
const SPARK: usize = 60; // samples shown in each card sparkline
const CARD_HEIGHT: f32 = 130.0;
const DRAG_THRESHOLD: f32 = 8.0; // px before a press becomes a drag

pub fn main() -> iced::Result {
    iced::application::timed(
        Pulse::new,
        Pulse::update,
        Pulse::subscription,
        Pulse::view,
    )
    .title(|_: &Pulse| String::from("Pulse (iced)"))
    .window_size((900.0, 640.0))
    .antialiasing(true)
    .run()
}

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

struct Metric {
    name: &'static str,
    unit: &'static str,
    max: f32,
    color: Color,
    value: f32,
    samples: VecDeque<f32>,
}

impl Metric {
    fn new(name: &'static str, unit: &'static str, max: f32, color: Color, start: f32) -> Self {
        let mut samples = VecDeque::with_capacity(HISTORY);
        samples.push_back(start);

        Self { name, unit, max, color, value: start, samples }
    }

    /// One step of a smooth, mean-reverting random walk.
    fn step(&mut self, rng: &mut Rng) {
        let jitter = (rng.next_f32() - 0.5) * self.max * 0.04;
        let reversion = (self.max * 0.5 - self.value) * 0.01;

        self.value = (self.value + jitter + reversion).clamp(0.0, self.max);

        if self.samples.len() == HISTORY {
            let _ = self.samples.pop_front();
        }
        self.samples.push_back(self.value);
    }

    fn display(&self) -> String {
        format!("{:.1}{}", self.value, self.unit)
    }
}

/// Tiny xorshift* PRNG — avoids pulling in `rand` for synthetic data.
struct Rng(u64);

impl Rng {
    fn next_f32(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;

        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 40) as f32
            / (1u64 << 24) as f32
    }
}

struct Drag {
    /// Index into `metrics` of the card being dragged.
    metric: usize,
    /// Where the pointer was when the drag started.
    origin: Option<Point>,
    /// Current pointer position (window coordinates).
    cursor: Point,
    /// True once the pointer moved beyond `DRAG_THRESHOLD`.
    active: bool,
}

struct Pulse {
    metrics: Vec<Metric>,
    /// Grid slots (2 rows × 3 columns) → metric index.
    order: Vec<usize>,
    selected: usize,
    running: bool,
    hz: u32,
    ticks: u64,
    rng: Rng,
    drag: Option<Drag>,
    /// Hover-elevation animation per metric (the deliberate animation).
    hover: Vec<Animation<bool>>,
    spark_caches: Vec<canvas::Cache>,
    chart_cache: canvas::Cache,
    now: Instant,
}

#[derive(Debug, Clone)]
enum Message {
    Tick,
    TogglePause,
    RateChanged(u32),
    CardPressed(usize),
    CardEntered(usize),
    CardExited(usize),
    PointerMoved(Point),
    PointerReleased,
    Animate,
}

impl Pulse {
    fn new() -> (Self, Task<Message>) {
        let metrics = vec![
            Metric::new("CPU", "%", 100.0, Color::from_rgb8(0x4f, 0x9c, 0xf5), 35.0),
            Metric::new("Memory", "%", 100.0, Color::from_rgb8(0xa7, 0x7b, 0xf3), 62.0),
            Metric::new("Net In", " MB/s", 400.0, Color::from_rgb8(0x3f, 0xc0, 0x8a), 120.0),
            Metric::new("Net Out", " MB/s", 400.0, Color::from_rgb8(0xe8, 0xa3, 0x3d), 80.0),
            Metric::new("Disk", " MB/s", 200.0, Color::from_rgb8(0xe2, 0x63, 0x7f), 40.0),
            Metric::new("Requests", " r/s", 1000.0, Color::from_rgb8(0x38, 0xb6, 0xd0), 420.0),
        ];

        let count = metrics.len();

        (
            Self {
                metrics,
                order: (0..count).collect(),
                selected: 0,
                running: true,
                hz: 10,
                ticks: 1,
                rng: Rng(0x9E37_79B9_7F4A_7C15),
                drag: None,
                hover: (0..count)
                    .map(|_| Animation::new(false).quick())
                    .collect(),
                spark_caches: (0..count).map(|_| canvas::Cache::new()).collect(),
                chart_cache: canvas::Cache::new(),
                now: Instant::now(),
            },
            Task::none(),
        )
    }

    fn update(&mut self, message: Message, now: Instant) -> Task<Message> {
        self.now = now;

        match message {
            Message::Tick => {
                for metric in &mut self.metrics {
                    metric.step(&mut self.rng);
                }
                self.ticks += 1;

                // Only the geometry that changed is re-tessellated.
                for cache in &self.spark_caches {
                    cache.clear();
                }
                self.chart_cache.clear();
            }
            Message::TogglePause => {
                self.running = !self.running;
            }
            Message::RateChanged(hz) => {
                self.hz = hz;
            }
            Message::CardPressed(slot) => {
                let metric = self.order[slot];

                if self.selected != metric {
                    self.selected = metric;
                    self.chart_cache.clear();
                }

                self.drag = Some(Drag {
                    metric,
                    origin: None,
                    cursor: Point::ORIGIN,
                    active: false,
                });
            }
            Message::CardEntered(slot) => {
                let metric = self.order[slot];

                self.hover[metric].go_mut(true, now);

                if let Some(drag) = &self.drag
                    && drag.active
                    && drag.metric != metric
                {
                    // Live reflow preview: move the dragged card into the
                    // hovered slot; the vacated styling below doubles as the
                    // slot indicator.
                    let dragged = drag.metric;
                    self.order.retain(|&m| m != dragged);
                    self.order.insert(slot, dragged);
                }
            }
            Message::CardExited(metric) => {
                self.hover[metric].go_mut(false, now);
            }
            Message::PointerMoved(position) => {
                if let Some(drag) = &mut self.drag {
                    drag.cursor = position;

                    match drag.origin {
                        None => drag.origin = Some(position),
                        Some(origin) => {
                            if !drag.active
                                && origin.distance(position) > DRAG_THRESHOLD
                            {
                                drag.active = true;
                            }
                        }
                    }
                }
            }
            Message::PointerReleased => {
                // The order was already updated live while dragging, so a
                // release simply commits (drops the ghost + slot indicator).
                self.drag = None;
            }
            Message::Animate => {
                // `now` was refreshed above; the view re-interpolates.
            }
        }

        Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subs = Vec::new();

        if self.running {
            subs.push(
                time::every(Duration::from_secs_f64(1.0 / f64::from(self.hz)))
                    .map(|_| Message::Tick),
            );
        }

        // Global pointer tracking is only alive while a drag is in progress.
        if self.drag.is_some() {
            subs.push(event::listen_with(drag_listener));
        }

        // Per-frame redraws only while an animation is actually running.
        if self.hover.iter().any(|animation| animation.is_animating(self.now)) {
            subs.push(window::frames().map(|_| Message::Animate));
        }

        Subscription::batch(subs)
    }

    fn view(&self) -> Element<'_, Message> {
        let controls = row![
            text("Pulse").size(22),
            space::horizontal(),
            button(if self.running { "Pause" } else { "Resume" })
                .on_press(Message::TogglePause)
                .padding([6, 14]),
            text(format!("{} Hz", self.hz)).size(14).width(50),
            slider(1..=60u32, self.hz, Message::RateChanged).width(220),
        ]
        .spacing(12)
        .align_y(Center);

        let cards = column![
            row((0..3).map(|slot| self.card(slot))).spacing(12),
            row((3..6).map(|slot| self.card(slot))).spacing(12),
        ]
        .spacing(12);

        let selected = &self.metrics[self.selected];

        let chart_panel = container(
            column![
                row![
                    text(selected.name).size(15),
                    text(selected.display())
                        .size(15)
                        .color(selected.color),
                    space::horizontal(),
                    text(format!("last {HISTORY} samples — hover for details"))
                        .size(12)
                        .style(text::secondary),
                ]
                .spacing(12)
                .align_y(Center),
                canvas(Chart {
                    metric: selected,
                    cache: &self.chart_cache,
                    ticks: self.ticks,
                })
                .width(Fill)
                .height(Fill),
            ]
            .spacing(8),
        )
        .padding(12)
        .style(panel_style)
        .width(Fill)
        .height(Fill);

        let content = column![controls, cards, chart_panel]
            .spacing(14)
            .padding(14);

        // Root stack: the UI plus (while dragging) a ghost card pinned to
        // the cursor. The ghost contains no interactive widgets, so events
        // still reach the cards underneath it.
        let mut layers = stack![content];

        if let Some(drag) = &self.drag
            && drag.active
        {
            let metric = &self.metrics[drag.metric];

            layers = layers.push(
                pin(container(
                    column![
                        text(metric.name).size(13).style(text::secondary),
                        text(metric.display()).size(22).color(metric.color),
                    ]
                    .spacing(4)
                    .padding([10, 14]),
                )
                .width(200)
                .style(ghost_style))
                .x(drag.cursor.x + 12.0)
                .y(drag.cursor.y + 8.0),
            );
        }

        layers.into()
    }

    fn card(&self, slot: usize) -> Element<'_, Message> {
        let metric_index = self.order[slot];
        let metric = &self.metrics[metric_index];

        let is_selected = self.selected == metric_index;
        let is_drag_source = self
            .drag
            .as_ref()
            .is_some_and(|drag| drag.active && drag.metric == metric_index);

        let body = column![
            text(metric.name).size(13).style(text::secondary),
            text(metric.display()).size(26),
            canvas(Sparkline {
                metric,
                cache: &self.spark_caches[metric_index],
            })
            .width(Fill)
            .height(34.0),
        ]
        .spacing(4);

        let elevation = self.hover[metric_index].interpolate(0.0, 1.0, self.now);

        let card = float(
            container(body)
                .padding(12)
                .width(Fill)
                .height(CARD_HEIGHT)
                .style(move |theme| {
                    card_style(theme, is_selected, is_drag_source, elevation)
                }),
        )
        .scale(1.0 + elevation * 0.02)
        .translate(move |_bounds, _viewport| Vector::new(0.0, -2.0 * elevation));

        mouse_area(card)
            .on_press(Message::CardPressed(slot))
            .on_enter(Message::CardEntered(slot))
            .on_exit(Message::CardExited(metric_index))
            .interaction(if is_drag_source {
                mouse::Interaction::Grabbing
            } else {
                mouse::Interaction::Grab
            })
            .into()
    }
}

/// Global event filter, subscribed only while a drag is in progress.
fn drag_listener(
    event: Event,
    _status: event::Status,
    _window: window::Id,
) -> Option<Message> {
    match event {
        Event::Mouse(mouse::Event::CursorMoved { position }) => {
            Some(Message::PointerMoved(position))
        }
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
            Some(Message::PointerReleased)
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Canvas programs
// ---------------------------------------------------------------------------

/// Card sparkline: last `SPARK` samples, cached until the next tick.
struct Sparkline<'a> {
    metric: &'a Metric,
    cache: &'a canvas::Cache,
}

impl canvas::Program<Message> for Sparkline<'_> {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let geometry = self.cache.draw(renderer, bounds.size(), |frame| {
            let samples = &self.metric.samples;
            let n = samples.len().min(SPARK);

            if n < 2 {
                return;
            }

            let w = frame.width();
            let h = frame.height();
            let step = w / (SPARK - 1) as f32;
            let start_x = w - (n - 1) as f32 * step;

            let window = samples.iter().skip(samples.len() - n);
            let path = canvas::Path::new(|p| {
                for (i, value) in window.enumerate() {
                    let x = start_x + i as f32 * step;
                    let y = h - (value / self.metric.max) * (h - 2.0) - 1.0;

                    if i == 0 {
                        p.move_to(Point::new(x, y));
                    } else {
                        p.line_to(Point::new(x, y));
                    }
                }
            });

            frame.stroke(
                &path,
                canvas::Stroke::default()
                    .with_width(1.5)
                    .with_color(self.metric.color),
            );
        });

        vec![geometry]
    }
}

/// Main chart: cached line layer + fresh crosshair/tooltip overlay.
struct Chart<'a> {
    metric: &'a Metric,
    cache: &'a canvas::Cache,
    ticks: u64,
}

impl Chart<'_> {
    const PAD_BOTTOM: f32 = 4.0;

    /// Horizontal step and x of the first visible sample.
    fn layout(&self, width: f32) -> (f32, f32, usize) {
        let n = self.metric.samples.len().min(HISTORY);
        let step = width / (HISTORY - 1) as f32;
        let start_x = width - (n - 1) as f32 * step;

        (step, start_x, n)
    }

    fn sample_y(&self, value: f32, height: f32) -> f32 {
        let usable = height - Self::PAD_BOTTOM;
        usable - (value / self.metric.max) * (usable - 8.0)
    }
}

impl canvas::Program<Message> for Chart<'_> {
    /// Hovered cursor position, in chart-local coordinates.
    type State = Option<Point>;

    fn update(
        &self,
        state: &mut Self::State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        match event {
            canvas::Event::Mouse(
                mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft,
            ) => {
                let position = cursor.position_in(bounds);

                if position != *state {
                    *state = position;

                    // Repaints just this widget; the cached line layer is
                    // reused, only the overlay is re-recorded.
                    Some(canvas::Action::request_redraw())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let palette = theme.extended_palette();

        let chart = self.cache.draw(renderer, bounds.size(), |frame| {
            let w = frame.width();
            let h = frame.height();
            let (step, start_x, n) = self.layout(w);

            // Horizontal gridlines + labels at 0 / 25 / 50 / 75 / 100 %.
            for i in 0..=4 {
                let value = self.metric.max * i as f32 / 4.0;
                let y = self.sample_y(value, h);

                frame.stroke(
                    &canvas::Path::line(
                        Point::new(0.0, y),
                        Point::new(w, y),
                    ),
                    canvas::Stroke::default()
                        .with_width(1.0)
                        .with_color(palette.background.weak.color),
                );

                frame.fill_text(canvas::Text {
                    content: format!("{value:.0}{}", self.metric.unit),
                    position: Point::new(4.0, y - 3.0),
                    color: palette.background.base.text.scale_alpha(0.4),
                    size: 10.0.into(),
                    align_y: alignment::Vertical::Bottom,
                    ..canvas::Text::default()
                });
            }

            if n < 2 {
                return;
            }

            // The metric line.
            let samples = self.metric.samples.iter().skip(self.metric.samples.len() - n);
            let line = canvas::Path::new(|p| {
                for (i, value) in samples.enumerate() {
                    let point = Point::new(
                        start_x + i as f32 * step,
                        self.sample_y(*value, h),
                    );

                    if i == 0 {
                        p.move_to(point);
                    } else {
                        p.line_to(point);
                    }
                }
            });

            frame.stroke(
                &line,
                canvas::Stroke {
                    line_join: canvas::LineJoin::Round,
                    ..canvas::Stroke::default()
                        .with_width(2.0)
                        .with_color(self.metric.color)
                },
            );
        });

        // Crosshair + tooltip overlay (only when hovered; never cached).
        let Some(hover) = *state else {
            return vec![chart];
        };

        let mut overlay = canvas::Frame::new(renderer, bounds.size());
        let w = overlay.width();
        let h = overlay.height();
        let (step, start_x, n) = self.layout(w);

        if n >= 2 && hover.x >= start_x - step {
            let index =
                (((hover.x - start_x) / step).round().max(0.0) as usize).min(n - 1);
            let value = self.metric.samples[self.metric.samples.len() - n + index];
            let x = start_x + index as f32 * step;
            let y = self.sample_y(value, h);

            // Crosshair.
            let crosshair_color = palette.background.base.text.scale_alpha(0.35);
            overlay.stroke(
                &canvas::Path::line(Point::new(x, 0.0), Point::new(x, h)),
                canvas::Stroke::default().with_width(1.0).with_color(crosshair_color),
            );
            overlay.stroke(
                &canvas::Path::line(Point::new(0.0, y), Point::new(w, y)),
                canvas::Stroke::default().with_width(1.0).with_color(crosshair_color),
            );
            overlay.fill(&canvas::Path::circle(Point::new(x, y), 3.5), self.metric.color);

            // Tooltip: "<value>  ·  sample #<absolute> (t-<age>)".
            let age = n - 1 - index;
            let absolute = self.ticks.saturating_sub(age as u64);
            let label = format!(
                "{:.1}{}  ·  #{} (t-{})",
                value, self.metric.unit, absolute, age
            );

            let tip_size = iced::Size::new(11.0 + 6.2 * label.len() as f32, 24.0);
            let tip_at = Point::new(
                (x + 12.0).min(w - tip_size.width - 4.0),
                (y - 34.0).max(4.0),
            );

            overlay.fill(
                &canvas::Path::rounded_rectangle(tip_at, tip_size, 5.0.into()),
                palette.background.weakest.color.scale_alpha(0.95),
            );
            overlay.fill_text(canvas::Text {
                content: label,
                position: Point::new(tip_at.x + 6.0, tip_at.y + tip_size.height / 2.0),
                color: palette.background.base.text,
                size: 12.0.into(),
                align_y: alignment::Vertical::Center,
                ..canvas::Text::default()
            });
        }

        vec![chart, overlay.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.is_some() {
            mouse::Interaction::Crosshair
        } else {
            mouse::Interaction::default()
        }
    }
}

// ---------------------------------------------------------------------------
// Styles
// ---------------------------------------------------------------------------

fn card_style(
    theme: &Theme,
    selected: bool,
    drag_source: bool,
    elevation: f32,
) -> container::Style {
    let palette = theme.extended_palette();

    if drag_source {
        // The vacated slot: dimmed body + accent outline = drop indicator.
        return container::Style {
            background: Some(palette.background.weakest.color.into()),
            border: Border {
                color: palette.primary.strong.color,
                width: 1.5,
                radius: 10.0.into(),
            },
            ..container::Style::default()
        };
    }

    container::Style {
        background: Some(palette.background.weak.color.into()),
        border: Border {
            color: if selected {
                palette.primary.strong.color
            } else {
                palette.background.strong.color
            },
            width: if selected { 2.0 } else { 1.0 },
            radius: 10.0.into(),
        },
        shadow: Shadow {
            color: Color::BLACK.scale_alpha(0.25 * elevation),
            offset: Vector::new(0.0, 3.0 * elevation),
            blur_radius: 12.0 * elevation,
        },
        ..container::Style::default()
    }
}

fn panel_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();

    container::Style {
        background: Some(palette.background.weakest.color.into()),
        border: Border {
            color: palette.background.strong.color,
            width: 1.0,
            radius: 10.0.into(),
        },
        ..container::Style::default()
    }
}

fn ghost_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();

    container::Style {
        background: Some(palette.background.weak.color.scale_alpha(0.92).into()),
        border: Border {
            color: palette.primary.strong.color,
            width: 1.0,
            radius: 10.0.into(),
        },
        shadow: Shadow {
            color: Color::BLACK.scale_alpha(0.4),
            offset: Vector::new(0.0, 6.0),
            blur_radius: 18.0,
        },
        ..container::Style::default()
    }
}
