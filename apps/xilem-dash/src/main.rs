//! "Pulse" — live metrics dashboard per apps/SPEC-2.md, built with xilem 0.4.0.
//!
//! Repaint strategy: the 10 Hz tick arrives via a `task_raw` tokio task; each
//! tick mutates app state, which re-runs `app_logic`, diffs the view tree, and
//! only the changed widgets (value labels, sparklines, chart) invalidate paint.
//! Vello then re-renders the window scene on the GPU. Hover crosshair redraws
//! are widget-local (`request_paint_only`, no view rebuild). Animations
//! (hover elevation) use masonry's per-widget `on_anim_frame`, which only
//! schedules frames while an animation is in flight.

mod widgets;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use xilem::core::fork;
use xilem::masonry::kurbo::{Point, Size, Vec2};
use xilem::masonry::properties::types::Length;
use xilem::style::Style as _;
use xilem::view::{
    flex_col, flex_row, grid, label, slider, text_button, transformed, FlexExt as _,
    GridExt as _,
};
use xilem::winit::error::EventLoopError;
use xilem::{EventLoop, FontWeight, WidgetView, WindowOptions, Xilem};

use widgets::{card_frame, chart, sparkline, CardEvent, CHART_CAPACITY};

const GRID_COLS: usize = 3;
const GRID_GAP: f64 = 12.0;
const SPARK_LEN: usize = 60;

/// Tiny xorshift* PRNG — avoids pulling in the `rand` crate.
struct Rng(u64);

impl Rng {
    fn next_f64(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

struct Metric {
    name: &'static str,
    unit: &'static str,
    value: f64,
    lo: f64,
    hi: f64,
    step: f64,
    samples: VecDeque<f32>,
}

impl Metric {
    fn new(name: &'static str, unit: &'static str, start: f64, lo: f64, hi: f64, step: f64) -> Self {
        let mut samples = VecDeque::with_capacity(CHART_CAPACITY);
        samples.push_back(start as f32);
        Self { name, unit, value: start, lo, hi, step, samples }
    }

    fn tick(&mut self, rng: &mut Rng) {
        self.value = (self.value + (rng.next_f64() - 0.5) * self.step).clamp(self.lo, self.hi);
        if self.samples.len() == CHART_CAPACITY {
            self.samples.pop_front();
        }
        self.samples.push_back(self.value as f32);
    }
}

/// Card-reorder drag in progress.
struct Drag {
    from_slot: usize,
    /// Window position where the press happened.
    press: Point,
    /// Current pointer window position.
    cur: Point,
    /// Grid origin in window coords, derived at drag start.
    grid_origin: Point,
    /// Size of one card at drag start.
    card: Size,
    target_slot: usize,
}

struct Pulse {
    metrics: Vec<Metric>,
    /// order[slot] = metric index shown in that slot.
    order: Vec<usize>,
    selected: usize,
    running: bool,
    hz: f64,
    drag: Option<Drag>,
    rng: Rng,
    /// Tick period in ms, shared with the timer task.
    period_ms: Arc<AtomicU64>,
}

impl Pulse {
    fn new() -> Self {
        Self {
            metrics: vec![
                Metric::new("CPU", "%", 35.0, 0.0, 100.0, 6.0),
                Metric::new("Memory", " GB", 11.0, 2.0, 24.0, 0.5),
                Metric::new("Net In", " MB/s", 40.0, 0.0, 125.0, 9.0),
                Metric::new("Net Out", " MB/s", 12.0, 0.0, 125.0, 5.0),
                Metric::new("Disk", " MB/s", 90.0, 0.0, 600.0, 40.0),
                Metric::new("Requests", "/s", 350.0, 0.0, 2000.0, 90.0),
            ],
            order: (0..6).collect(),
            selected: 0,
            running: true,
            hz: 10.0,
            drag: None,
            rng: Rng(0x9E3779B97F4A7C15),
            period_ms: Arc::new(AtomicU64::new(100)),
        }
    }

    fn tick(&mut self) {
        for m in &mut self.metrics {
            m.tick(&mut self.rng);
        }
    }

    fn card_event(&mut self, slot: usize, event: CardEvent) {
        match event {
            CardEvent::Clicked | CardEvent::DoubleClicked => {
                self.selected = self.order[slot];
            }
            CardEvent::DragStart { window, origin, size, .. } => {
                // Cards are uniform: reconstruct the grid origin from this
                // card's own window origin and its slot position.
                let col = (slot % GRID_COLS) as f64;
                let row = (slot / GRID_COLS) as f64;
                let grid_origin = origin
                    - Vec2::new(col * (size.width + GRID_GAP), row * (size.height + GRID_GAP));
                self.selected = self.order[slot];
                self.drag = Some(Drag {
                    from_slot: slot,
                    press: window,
                    cur: window,
                    grid_origin,
                    card: size,
                    target_slot: slot,
                });
            }
            CardEvent::DragMove { window } => {
                if let Some(drag) = &mut self.drag {
                    drag.cur = window;
                    let rel = window - drag.grid_origin;
                    let col = (rel.x / (drag.card.width + GRID_GAP))
                        .floor()
                        .clamp(0.0, (GRID_COLS - 1) as f64) as usize;
                    let row = (rel.y / (drag.card.height + GRID_GAP)).floor().clamp(0.0, 1.0)
                        as usize;
                    drag.target_slot = row * GRID_COLS + col;
                }
            }
            CardEvent::DragEnd { .. } => {
                if let Some(drag) = self.drag.take() {
                    if drag.target_slot != drag.from_slot {
                        // remove + insert: the grid reflows around the card.
                        let metric = self.order.remove(drag.from_slot);
                        self.order.insert(drag.target_slot, metric);
                    }
                }
            }
            CardEvent::DragCancelled => {
                self.drag = None;
            }
            CardEvent::EscapePressed => {}
        }
    }
}

fn metric_card(state: &Pulse, slot: usize) -> impl WidgetView<Pulse> + use<> {
    let metric_idx = state.order[slot];
    let m = &state.metrics[metric_idx];
    let spark: Vec<f32> = m
        .samples
        .iter()
        .rev()
        .take(SPARK_LEN)
        .rev()
        .copied()
        .collect();

    let content = flex_col((
        label(m.name).text_size(13.0).flex(1.0),
        label(format!("{:.1}{}", m.value, m.unit))
            .text_size(26.0)
            .weight(FontWeight::BOLD),
        sparkline(spark),
    ))
    .gap(Length::px(2.0));

    let (dragged, offset, drop_hint) = match &state.drag {
        Some(drag) if drag.from_slot == slot => (true, drag.cur - drag.press, false),
        Some(drag) if drag.target_slot == slot => (false, Vec2::ZERO, true),
        _ => (false, Vec2::ZERO, false),
    };

    let card = card_frame(content, move |state: &mut Pulse, event| {
        state.card_event(slot, event);
    })
    .fill() // grid cells: fill the whole cell
    .selected(state.selected == metric_idx)
    .drop_hint(drop_hint);

    // The dragged card follows the cursor via a render transform ("ghost");
    // everyone else sits at the identity transform (uniform view type).
    transformed(card).translate(if dragged { offset } else { Vec2::ZERO })
}

fn app_logic(state: &mut Pulse) -> impl WidgetView<Pulse> + use<> {
    let cards = (0..6)
        .map(|slot| {
            metric_card(state, slot).grid_pos((slot % GRID_COLS) as i32, (slot / GRID_COLS) as i32)
        })
        .collect::<Vec<_>>();
    let card_grid = grid(cards, GRID_COLS as i32, 2).spacing(Length::px(GRID_GAP));

    let selected = &state.metrics[state.selected];
    let chart_title = label(format!(
        "{} — last {} samples",
        selected.name,
        selected.samples.len().min(CHART_CAPACITY)
    ))
    .text_size(14.0)
    .weight(FontWeight::BOLD);
    let main_chart = chart(
        selected.samples.iter().copied().collect(),
        selected.unit,
    );

    let period_ms = state.period_ms.clone();
    let controls = flex_row((
        text_button(if state.running { "Pause" } else { "Resume" }, |state: &mut Pulse| {
            state.running = !state.running;
        }),
        label(format!("{:.0} Hz", state.hz)).text_size(13.0),
        // NOTE: masonry 0.4's Slider clamps its own width to 100..200 px and
        // ignores the flex allotment, so `.flex(1.0)` would be pointless.
        slider(1.0, 60.0, state.hz, move |state: &mut Pulse, hz| {
            state.hz = hz;
            state
                .period_ms
                .store((1000.0 / hz).round().max(1.0) as u64, Ordering::Relaxed);
        })
        .step(1.0),
    ))
    .gap(Length::px(10.0));

    let body = flex_col((
        card_grid.flex(1.1),
        chart_title,
        main_chart.flex(1.0),
        controls,
    ))
    .gap(Length::px(10.0))
    .padding(12.0);

    // The timer only exists while running: pausing tears the task down.
    fork(
        body,
        state.running.then(|| {
            let period_ms = period_ms.clone();
            xilem::view::task_raw(
                move |proxy| {
                    let period_ms = period_ms.clone();
                    async move {
                        loop {
                            let ms = period_ms.load(Ordering::Relaxed).max(1);
                            xilem::tokio::time::sleep(Duration::from_millis(ms)).await;
                            if proxy.message(()).is_err() {
                                break;
                            }
                        }
                    }
                },
                |state: &mut Pulse, ()| state.tick(),
            )
        }),
    )
}

fn main() -> Result<(), EventLoopError> {
    let window = WindowOptions::new("Pulse (xilem)")
        .with_initial_inner_size(xilem::dpi::LogicalSize::new(900.0, 640.0))
        .with_resizable(true);
    Xilem::new_simple(Pulse::new(), app_logic, window).run_in(EventLoop::with_user_event())
}
