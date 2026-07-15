//! "Pulse" — live metrics dashboard per apps/SPEC-2.md, in egui 0.35 (eframe)
//! with egui_plot 0.36 (charts) and egui_dnd 0.16 (card reorder).
//!
//! Repaint strategy (the headline datapoint for immediate mode):
//! - While running, each frame calls `ctx.request_repaint_after(<time until
//!   the next tick deadline>)`; egui sleeps until then (or until input).
//!   This is *reactive* repainting — exactly one scheduled wakeup per tick,
//!   NOT a continuous repaint loop.
//! - While paused, we request nothing: egui repaints only on input events.
//! - Animations (`Context::animate_bool`, egui_dnd's swap/return tweens)
//!   request their own repaints while they are in flight, so they stay
//!   smooth even when paused.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Color32, Id, Margin, RichText, Sense, Shape, Stroke, epaint::Shadow, vec2,
};
use egui_dnd::dnd;
use egui_plot::{Line, Plot, PlotPoints, Points, VLine};

/// Samples kept per metric (main chart plots all of these).
const HISTORY: usize = 300;
/// Samples shown in each card's sparkline.
const SPARK: usize = 60;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 640.0])
            .with_resizable(true),
        ..Default::default()
    };
    eframe::run_native(
        "Pulse (egui)", // window title
        options,
        Box::new(|_cc| Ok(Box::new(PulseApp::new()))),
    )
}

struct Metric {
    name: &'static str,
    unit: &'static str,
    color: Color32,
    lo: f64,
    hi: f64,
    /// Max random-walk step per tick.
    step: f64,
    value: f64,
    samples: VecDeque<f64>,
}

struct PulseApp {
    /// The six metrics; identity is the index into this Vec (never reordered).
    metrics: Vec<Metric>,
    /// Display order of the cards; this is what egui_dnd reorders. The
    /// usize values are stable metric identities, which keeps selection and
    /// per-card animation state stable across reorders.
    order: Vec<usize>,
    /// Identity (index into `metrics`) of the selected metric.
    selected: usize,
    paused: bool,
    /// Tick rate in Hz (1..=60), driven by the slider.
    hz: f64,
    /// Deadline bookkeeping for the fixed-rate tick.
    last_tick: Instant,
    /// xorshift64 state for the synthetic random walk (no rand dependency).
    rng: u64,
    /// Total samples generated since launch; doubles as the x axis.
    total: u64,
}

impl PulseApp {
    fn new() -> Self {
        let metrics = vec![
            Metric::new("CPU", "%", Color32::from_rgb(0x4c, 0x8b, 0xf5), 0.0, 100.0, 4.0, 45.0),
            Metric::new("Memory", "GB", Color32::from_rgb(0x9a, 0x6b, 0xf5), 0.0, 24.0, 0.4, 11.0),
            Metric::new("Net In", "MB/s", Color32::from_rgb(0x35, 0xb5, 0x7c), 0.0, 400.0, 18.0, 120.0),
            Metric::new("Net Out", "MB/s", Color32::from_rgb(0x2f, 0xb8, 0xc5), 0.0, 400.0, 18.0, 80.0),
            Metric::new("Disk", "MB/s", Color32::from_rgb(0xe0, 0x8a, 0x3c), 0.0, 600.0, 30.0, 210.0),
            Metric::new("Requests", "req/s", Color32::from_rgb(0xd9, 0x53, 0x80), 0.0, 2000.0, 90.0, 640.0),
        ];
        let order = (0..metrics.len()).collect();
        let mut app = Self {
            metrics,
            order,
            selected: 0,
            paused: false,
            hz: 10.0, // default 10 Hz per spec
            last_tick: Instant::now(),
            rng: 0x9e37_79b9_7f4a_7c15,
            total: 0,
        };
        // Pre-fill the full history so charts look alive immediately.
        for _ in 0..HISTORY {
            app.step_metrics();
        }
        app
    }

    /// xorshift64 → uniform f64 in [0, 1).
    fn next_f64(&mut self) -> f64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 11) as f64 / (1u64 << 53) as f64
    }

    /// One synthetic sample per metric: smooth bounded random walk.
    fn step_metrics(&mut self) {
        for i in 0..self.metrics.len() {
            let r = self.next_f64() - 0.5;
            let m = &mut self.metrics[i];
            // Gentle pull toward the middle keeps the walk from pinning at
            // the clamp bounds; the random term dominates short-term shape.
            let mid_pull = (0.5 * (m.lo + m.hi) - m.value) * 0.01;
            m.value = (m.value + r * 2.0 * m.step + mid_pull).clamp(m.lo, m.hi);
            if m.samples.len() == HISTORY {
                m.samples.pop_front();
            }
            m.samples.push_back(m.value);
        }
        self.total += 1;
    }

    /// Fixed-rate tick driven by wall clock; see the module docs for the
    /// repaint strategy.
    fn tick_clock(&mut self, ctx: &egui::Context) {
        if self.paused {
            return; // no repaint request: egui idles until input
        }
        let period = Duration::from_secs_f64(1.0 / self.hz);
        // Don't burst-catch-up after long stalls (window hidden, debugger…).
        if self.last_tick.elapsed() > period + Duration::from_millis(500) {
            self.last_tick = Instant::now() - period;
        }
        while self.last_tick.elapsed() >= period {
            self.last_tick += period;
            self.step_metrics();
        }
        // Exactly one scheduled wakeup, at the next tick deadline.
        ctx.request_repaint_after(period - self.last_tick.elapsed());
    }

    /// Controls row: pause/resume + tick-rate slider with live readout.
    fn controls(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let label = if self.paused { "▶ Resume" } else { "⏸ Pause" };
            if ui.button(label).clicked() {
                self.paused = !self.paused;
                if !self.paused {
                    self.last_tick = Instant::now(); // don't catch up on resume
                }
            }
            ui.separator();
            ui.label("Tick rate");
            ui.add(
                egui::Slider::new(&mut self.hz, 1.0..=60.0)
                    .fixed_decimals(0)
                    .suffix(" Hz"),
            );
            ui.separator();
            ui.label(
                RichText::new(format!("{:.0} Hz · sample #{}", self.hz, self.total)).weak(),
            );
        });
    }

    /// The 3×2 metric card grid: drag any card (whole card is the drag
    /// handle) to reorder; click a card to select it for the main chart.
    fn cards(&mut self, ui: &mut egui::Ui) {
        let spacing = ui.spacing().item_spacing.x;
        let card_size = vec2(
            ((ui.available_width() - 2.0 * spacing) / 3.0 - 1.0).max(140.0),
            116.0,
        );
        let metrics = &self.metrics; // split borrow: dnd gets &mut self.order
        let selected = self.selected;
        let mut clicked = None;
        ui.horizontal_wrapped(|ui| {
            dnd(ui, "metric-cards").show_vec_sized(
                &mut self.order,
                card_size,
                |ui, item: &mut usize, handle, state| {
                    let idx = *item;
                    let m = &metrics[idx];
                    let is_selected = idx == selected;

                    // Placeholder shape *under* the card, filled in after we
                    // know the card's rect + hover state (same-frame shadow).
                    let shadow_slot = ui.painter().add(Shape::Noop);

                    // Whole card = drag handle; extra click sense = select.
                    let response = handle.sense(Sense::click()).ui(ui, |ui| {
                        let stroke = if is_selected {
                            Stroke::new(2.0, ui.visuals().selection.stroke.color)
                        } else {
                            Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color)
                        };
                        egui::Frame::new()
                            .fill(ui.visuals().faint_bg_color)
                            .stroke(stroke)
                            .corner_radius(8.0)
                            .inner_margin(Margin::same(10))
                            .show(ui, |ui| {
                                ui.set_min_size(card_size - vec2(22.0, 22.0));
                                ui.set_max_width(card_size.x - 22.0);
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("⠿").weak()); // grip cue
                                    ui.label(RichText::new(m.name).small().strong());
                                });
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(format!("{:.1}", m.value))
                                            .size(24.0)
                                            .strong()
                                            .color(m.color),
                                    );
                                    ui.label(RichText::new(m.unit).small().weak());
                                });
                                // Sparkline: last 60 samples, no axes/interaction.
                                let pts: PlotPoints = m
                                    .samples
                                    .iter()
                                    .rev()
                                    .take(SPARK)
                                    .rev()
                                    .enumerate()
                                    .map(|(i, v)| [i as f64, *v])
                                    .collect();
                                Plot::new(("spark", idx))
                                    .height(30.0)
                                    .width(ui.available_width())
                                    .show_axes(false)
                                    .show_grid(false)
                                    .show_background(false)
                                    .show_x(false)
                                    .show_y(false)
                                    .allow_drag(false)
                                    .allow_zoom(false)
                                    .allow_scroll(false)
                                    .allow_boxed_zoom(false)
                                    .allow_double_click_reset(false)
                                    .sense(Sense::hover())
                                    .set_margin_fraction(vec2(0.0, 0.15))
                                    .show(ui, |p| {
                                        p.line(Line::new(m.name, pts).color(m.color).width(1.5));
                                    });
                            });
                    });

                    if response.clicked() {
                        clicked = Some(idx);
                    }

                    // Hover-elevation animation: `animate_bool` tweens
                    // 0→1 over the style's animation_time and requests its
                    // own repaints while animating.
                    let t = ui.ctx().animate_bool(
                        Id::new(("card-elev", idx)),
                        response.hovered() && !state.dragged,
                    );
                    if t > 0.0 {
                        let shadow = Shadow {
                            offset: [0, 3],
                            blur: (12.0 * t) as u8,
                            spread: (2.0 * t) as u8,
                            color: Color32::from_black_alpha((90.0 * t) as u8),
                        };
                        ui.painter().set(shadow_slot, shadow.as_shape(response.rect, 8.0));
                    }
                },
            );
        });
        if let Some(idx) = clicked {
            self.selected = idx;
        }
    }

    /// Main chart of the selected metric: last 300 samples, scrolling x,
    /// crosshair + snapped marker + tooltip on hover.
    fn chart(&mut self, ui: &mut egui::Ui) {
        let m = &self.metrics[self.selected];
        let first = self.total - m.samples.len() as u64;
        let last = self.total.saturating_sub(1);
        let pts: Vec<[f64; 2]> = m
            .samples
            .iter()
            .enumerate()
            .map(|(i, v)| [(first + i as u64) as f64, *v])
            .collect();
        let (name, unit, color) = (m.name, m.unit, m.color);
        let samples = &m.samples;

        let mut snapped: Option<(f64, f64)> = None;
        let response = Plot::new("main-chart")
            .show_crosshair(true) // built-in crosshair at the cursor
            .label_formatter(|_| None) // we show our own snapped tooltip instead
            .allow_drag(false)
            .allow_zoom(false)
            .allow_scroll(false)
            .allow_boxed_zoom(false)
            .allow_double_click_reset(false)
            .set_margin_fraction(vec2(0.0, 0.1))
            .show(ui, |p| {
                p.line(Line::new(name, PlotPoints::from(pts)).color(color).width(2.0));
                // Snap a marker + vline to the nearest sample under the cursor.
                if let Some(coord) = p.pointer_coordinate() {
                    let x = coord.x.round().clamp(first as f64, last as f64);
                    let y = samples[(x as u64 - first) as usize];
                    p.vline(VLine::new("cursor-x", x).color(Color32::GRAY).width(1.0));
                    p.points(Points::new("cursor-pt", vec![[x, y]]).radius(4.0).color(color));
                    snapped = Some((x, y));
                }
            });
        if let Some((x, y)) = snapped {
            response.response.clone().on_hover_ui_at_pointer(|ui| {
                ui.label(RichText::new(name).strong());
                ui.label(format!("sample #{x:.0}"));
                ui.label(format!("{y:.1} {unit}"));
            });
        }
    }

    /// Whole UI; split out from `eframe::App::ui` so tests can drive it.
    fn show(&mut self, ui: &mut egui::Ui) {
        self.tick_clock(&ui.ctx().clone());
        // (egui 0.35 unified Top/Side panels into `egui::Panel`.)
        egui::Panel::top("controls").show(ui, |ui| {
            ui.add_space(4.0);
            self.controls(ui);
            ui.add_space(4.0);
        });
        egui::CentralPanel::default().show(ui, |ui| {
            self.cards(ui);
            ui.add_space(6.0);
            ui.separator();
            self.chart(ui); // Plot fills the remaining space by default
        });
    }
}

impl Metric {
    fn new(
        name: &'static str,
        unit: &'static str,
        color: Color32,
        lo: f64,
        hi: f64,
        step: f64,
        start: f64,
    ) -> Self {
        Self {
            name,
            unit,
            color,
            lo,
            hi,
            step,
            value: start,
            samples: VecDeque::with_capacity(HISTORY),
        }
    }
}

impl eframe::App for PulseApp {
    // Since egui 0.34, `App::ui` (replacing `App::update`) hands us the root
    // `Ui` of the viewport.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    fn new_harness() -> Harness<'static, PulseApp> {
        Harness::new_ui_state(|ui, app: &mut PulseApp| app.show(ui), PulseApp::new())
    }

    // NOTE: while unpaused the app requests a repaint every frame (that is
    // the point of the tick loop), so `Harness::run` — which steps until the
    // app stops requesting repaints — never settles. Use explicit `step`s.

    #[test]
    fn pause_button_toggles_and_stops_ticking() {
        let mut harness = new_harness();
        harness.step();
        assert!(!harness.state().paused);

        harness.get_by_label("⏸ Pause").click();
        harness.step();
        assert!(harness.state().paused);

        let frozen = harness.state().total;
        std::thread::sleep(Duration::from_millis(250));
        harness.step();
        assert_eq!(harness.state().total, frozen, "paused app must not tick");

        harness.get_by_label("▶ Resume").click();
        harness.step();
        assert!(!harness.state().paused);
    }

    #[test]
    fn ticks_advance_at_roughly_the_configured_rate() {
        let mut harness = new_harness();
        harness.step();
        let before = harness.state().total;
        std::thread::sleep(Duration::from_millis(500)); // ~5 ticks at 10 Hz
        harness.step();
        let gained = harness.state().total - before;
        assert!((2..=10).contains(&gained), "expected ~5 ticks, got {gained}");
    }

    #[test]
    fn clicking_a_card_selects_its_metric() {
        let mut harness = new_harness();
        harness.step();
        assert_eq!(harness.state().selected, 0);
        // Card labels are in the AccessKit tree; click the "Disk" card label.
        harness.get_by_label("Disk").click();
        harness.step();
        harness.step();
        assert_eq!(harness.state().selected, 4);
    }
}
