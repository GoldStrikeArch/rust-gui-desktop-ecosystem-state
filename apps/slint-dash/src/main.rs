// Pulse — live metrics dashboard (Slint 1.17.1).
//
// Rust owns all data: a `slint::Timer` (repeated) ticks the synthetic random
// walk at the configured rate, rebuilds the sparkline/chart `Path` command
// strings, and pushes them into properties/models. Slint's property system
// marks the scene dirty and the femtovg (GL) renderer redraws the window once
// per tick — there is no repaint while paused/idle.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Duration;

use slint::{ComponentHandle, Model, ModelRc, SharedString, Timer, TimerMode, VecModel};

slint::include_modules!();

const N_CHART: usize = 300;
const N_SPARK: usize = 60;

struct MetricDef {
    name: &'static str,
    unit: &'static str,
    start: f32,
    min: f32,
    max: f32,
    step: f32,
}

const DEFS: [MetricDef; 6] = [
    MetricDef { name: "CPU", unit: "%", start: 42.0, min: 0.0, max: 100.0, step: 3.0 },
    MetricDef { name: "Memory", unit: "GB", start: 11.5, min: 2.0, max: 24.0, step: 0.25 },
    MetricDef { name: "Network In", unit: "MB/s", start: 24.0, min: 0.0, max: 120.0, step: 4.0 },
    MetricDef { name: "Network Out", unit: "MB/s", start: 9.0, min: 0.0, max: 80.0, step: 2.5 },
    MetricDef { name: "Disk", unit: "MB/s", start: 55.0, min: 0.0, max: 400.0, step: 12.0 },
    MetricDef { name: "Requests", unit: "/s", start: 310.0, min: 0.0, max: 900.0, step: 20.0 },
];

/// Tiny xorshift* PRNG so we do not need the `rand` crate.
struct Rng(u64);
impl Rng {
    fn next_f32(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f32 / (1u64 << 53) as f32
    }
}

struct Sim {
    histories: Vec<VecDeque<f32>>,
    rng: Rng,
}

impl Sim {
    fn new() -> Self {
        let mut sim = Sim {
            histories: DEFS
                .iter()
                .map(|d| {
                    let mut q = VecDeque::with_capacity(N_CHART + 1);
                    q.push_back(d.start);
                    q
                })
                .collect(),
            rng: Rng(0x9E3779B97F4A7C15),
        };
        // Prefill so charts are full from the first frame.
        for _ in 0..N_CHART {
            sim.tick();
        }
        sim
    }

    fn tick(&mut self) {
        for (i, def) in DEFS.iter().enumerate() {
            let h = &mut self.histories[i];
            let last = *h.back().unwrap();
            let next = (last + (self.rng.next_f32() * 2.0 - 1.0) * def.step)
                .clamp(def.min, def.max);
            h.push_back(next);
            while h.len() > N_CHART {
                h.pop_front();
            }
        }
    }
}

fn minmax(vals: impl Iterator<Item = f32> + Clone) -> (f32, f32) {
    let mn = vals.clone().fold(f32::INFINITY, f32::min);
    let mx = vals.fold(f32::NEG_INFINITY, f32::max);
    (mn, mx)
}

/// Last `N_SPARK` samples as SVG path commands in a 0..59 x 0..40 viewbox.
fn spark_cmds(h: &VecDeque<f32>) -> String {
    let n = N_SPARK.min(h.len());
    let tail = h.iter().skip(h.len() - n).copied();
    let (mn, mx) = minmax(tail.clone());
    let span = (mx - mn).max(1e-6);
    let mut s = String::with_capacity(n * 14);
    for (i, v) in tail.enumerate() {
        let x = i as f32 * 59.0 / (n.max(2) - 1) as f32;
        let y = 38.0 - (v - mn) / span * 36.0;
        if i == 0 {
            s.push_str(&format!("M {x:.1} {y:.1}"));
        } else {
            s.push_str(&format!(" L {x:.1} {y:.1}"));
        }
    }
    s
}

/// Full history as (line, filled-area) commands in a 0..299 x 0..100 viewbox,
/// plus the (min, max) used for normalization.
fn chart_cmds(h: &VecDeque<f32>) -> (String, String, f32, f32) {
    let (mn, mx) = minmax(h.iter().copied());
    let span = (mx - mn).max(1e-6);
    let mut line = String::with_capacity(h.len() * 16);
    let mut last_x = 0.0f32;
    for (i, v) in h.iter().enumerate() {
        let x = i as f32 * 299.0 / (h.len().max(2) - 1) as f32;
        let y = 98.0 - (v - mn) / span * 96.0;
        last_x = x;
        if i == 0 {
            line.push_str(&format!("M {x:.1} {y:.1}"));
        } else {
            line.push_str(&format!(" L {x:.1} {y:.1}"));
        }
    }
    let fill = format!("{line} L {last_x:.1} 100 L 0 100 Z");
    (line, fill, mn, mx)
}

fn round1(v: f32) -> f32 {
    (v * 10.0).round() / 10.0
}

fn refresh_cards(ui: &MainWindow, sim: &Sim) {
    let model = ui.get_metrics();
    for i in 0..DEFS.len() {
        if let Some(mut row) = model.row_data(i) {
            let h = &sim.histories[i];
            row.display = SharedString::from(format!("{:.1}", h.back().unwrap()));
            row.spark = SharedString::from(spark_cmds(h));
            model.set_row_data(i, row);
        }
    }
}

fn refresh_chart(ui: &MainWindow, sim: &Sim) {
    let sel = ui.get_selected_id().clamp(0, 5) as usize;
    let h = &sim.histories[sel];
    let (line, fill, mn, mx) = chart_cmds(h);
    ui.set_chart_line(SharedString::from(line));
    ui.set_chart_fill(SharedString::from(fill));
    ui.set_chart_min(round1(mn));
    ui.set_chart_max(round1(mx));
    ui.set_chart_count(h.len() as i32);
    ui.set_sel_name(SharedString::from(DEFS[sel].name));
    ui.set_sel_unit(SharedString::from(DEFS[sel].unit));
    let samples: Vec<f32> = h.iter().map(|v| round1(*v)).collect();
    ui.set_chart_samples(ModelRc::new(VecModel::from(samples)));
}

fn start_timer(timer: &Timer, hz: i32, ui: slint::Weak<MainWindow>, sim: Rc<RefCell<Sim>>) {
    let hz = hz.clamp(1, 60);
    timer.start(
        TimerMode::Repeated,
        Duration::from_secs_f64(1.0 / hz as f64),
        move || {
            if let Some(ui) = ui.upgrade() {
                sim.borrow_mut().tick();
                let sim = sim.borrow();
                refresh_cards(&ui, &sim);
                refresh_chart(&ui, &sim);
            }
        },
    );
}

fn main() -> Result<(), slint::PlatformError> {
    let ui = MainWindow::new()?;
    let sim = Rc::new(RefCell::new(Sim::new()));
    let timer = Rc::new(Timer::default());

    // Initial model: one row per metric, slot == id.
    let rows: Vec<MetricData> = DEFS
        .iter()
        .enumerate()
        .map(|(i, d)| MetricData {
            id: i as i32,
            name: d.name.into(),
            unit: d.unit.into(),
            display: SharedString::from(format!("{:.1}", sim.borrow().histories[i].back().unwrap())),
            spark: SharedString::from(spark_cmds(&sim.borrow().histories[i])),
            slot: i as i32,
        })
        .collect();
    let metrics = Rc::new(VecModel::from(rows));
    ui.set_metrics(ModelRc::from(metrics.clone()));
    refresh_chart(&ui, &sim.borrow());

    // Reorder: move card from one slot to another; slots in between shift.
    ui.on_reorder({
        let metrics = metrics.clone();
        move |from, to| {
            for r in 0..metrics.row_count() {
                let mut row = metrics.row_data(r).unwrap();
                let s = row.slot;
                row.slot = if s == from {
                    to
                } else if from < to && s > from && s <= to {
                    s - 1
                } else if to < from && s >= to && s < from {
                    s + 1
                } else {
                    s
                };
                if row.slot != s {
                    metrics.set_row_data(r, row);
                }
            }
        }
    });

    ui.on_select({
        let ui = ui.as_weak();
        let sim = sim.clone();
        move |id| {
            if let Some(ui) = ui.upgrade() {
                ui.set_selected_id(id);
                refresh_chart(&ui, &sim.borrow());
            }
        }
    });

    ui.on_toggle_pause({
        let ui = ui.as_weak();
        let sim = sim.clone();
        let timer = timer.clone();
        move || {
            if let Some(ui) = ui.upgrade() {
                if ui.get_running() {
                    timer.stop();
                    ui.set_running(false);
                } else {
                    start_timer(&timer, ui.get_rate(), ui.as_weak(), sim.clone());
                    ui.set_running(true);
                }
            }
        }
    });

    ui.on_rate_changed({
        let ui = ui.as_weak();
        let sim = sim.clone();
        let timer = timer.clone();
        move |hz| {
            if let Some(ui) = ui.upgrade() {
                ui.set_rate(hz);
                if ui.get_running() {
                    start_timer(&timer, hz, ui.as_weak(), sim.clone());
                }
            }
        }
    });

    start_timer(&timer, 10, ui.as_weak(), sim.clone());
    ui.run()
}
