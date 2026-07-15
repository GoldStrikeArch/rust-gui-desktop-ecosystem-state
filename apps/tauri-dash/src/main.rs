// Pulse (Tauri) — RCN GUI ecosystem research, iteration 2 (SPEC-2).
//
// Architecture (deliberate, see FRICTION.md): the synthetic metric generator
// lives in Rust on a dedicated thread and pushes every sample batch to the
// webview through Tauri's event bridge (`AppHandle::emit`) at the tick rate
// (default 10 Hz, 1–60 Hz via the `set_rate` command). This exercises the IPC
// path under sustained load — the point of measuring Tauri here. The frontend
// owns only presentation state (ring buffers, card order, selection).
//
// The frontend measures event inter-arrival jitter and emit→JS latency and
// reports them back over `report_stats`, which prints to stdout, so a headless
// launch verifies the event path end to end.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

/// Shared generator controls. The rate is stored in milli-hertz so both knobs
/// fit in lock-free atomics (nothing blocks the emit loop).
struct GenCtl {
    paused: AtomicBool,
    rate_mhz: AtomicU32,
}

impl GenCtl {
    fn hz(&self) -> f64 {
        f64::from(self.rate_mhz.load(Ordering::Relaxed)) / 1000.0
    }
}

/// One tick pushed over the event bridge: a batch of all 6 metric values.
#[derive(Clone, Serialize)]
struct Tick {
    seq: u64,
    /// Wall-clock ms at emit time; the frontend subtracts this from its own
    /// clock (same machine) to estimate IPC event latency.
    emitted_ms: f64,
    values: [f64; 6],
}

#[derive(Serialize)]
struct Config {
    hz: f64,
    paused: bool,
}

/// Initial state for the frontend controls (rate may come from `PULSE_HZ`).
#[tauri::command]
fn get_config(ctl: State<'_, Arc<GenCtl>>) -> Config {
    Config { hz: ctl.hz(), paused: ctl.paused.load(Ordering::Relaxed) }
}

/// Tick-rate slider backend. Clamps to 1–60 Hz, returns the applied rate.
#[tauri::command]
fn set_rate(hz: f64, ctl: State<'_, Arc<GenCtl>>) -> f64 {
    let hz = hz.clamp(1.0, 60.0);
    ctl.rate_mhz.store((hz * 1000.0) as u32, Ordering::Relaxed);
    hz
}

/// Pause/resume backend.
#[tauri::command]
fn set_paused(paused: bool, ctl: State<'_, Arc<GenCtl>>) {
    ctl.paused.store(paused, Ordering::Relaxed);
}

/// IPC statistics measured in the webview, printed to stdout for headless
/// verification of the event path (arrival jitter + emit→listener latency).
#[tauri::command]
#[allow(clippy::too_many_arguments)]
fn report_stats(
    count: u32,
    mean_interval_ms: f64,
    max_interval_ms: f64,
    jitter_ms: f64,
    mean_latency_ms: f64,
    max_latency_ms: f64,
    ctl: State<'_, Arc<GenCtl>>,
) {
    println!(
        "[pulse] rate={:.0}Hz ticks={} | interval mean={:.2}ms max={:.2}ms sd={:.2}ms | emit->JS latency mean={:.2}ms max={:.2}ms",
        ctl.hz(), count, mean_interval_ms, max_interval_ms, jitter_ms, mean_latency_ms, max_latency_ms
    );
}

/// xorshift64* — a smooth random walk does not justify pulling in `rand`.
fn next_u64(s: &mut u64) -> u64 {
    *s ^= *s << 13;
    *s ^= *s >> 7;
    *s ^= *s << 17;
    s.wrapping_mul(0x2545_F491_4F6C_DD1D)
}

/// Uniform noise in [-1, 1].
fn noise(s: &mut u64) -> f64 {
    (next_u64(s) >> 11) as f64 / ((1u64 << 53) as f64) * 2.0 - 1.0
}

fn wall_ms() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs_f64() * 1000.0
}

fn spawn_generator(app: AppHandle, ctl: Arc<GenCtl>) {
    std::thread::spawn(move || {
        // Per-metric (center, volatility): CPU, Memory, Net In, Net Out, Disk, Requests.
        const SHAPE: [(f64, f64); 6] =
            [(45.0, 2.5), (62.0, 0.8), (30.0, 3.5), (22.0, 3.0), (55.0, 0.6), (48.0, 4.0)];
        let mut vals: [f64; 6] = [45.0, 62.0, 30.0, 22.0, 55.0, 48.0];
        let mut rng: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut seq: u64 = 0;
        let mut next = Instant::now();
        loop {
            // Absolute-deadline pacing so sleep drift does not accumulate.
            next += Duration::from_secs_f64(1.0 / ctl.hz());
            let now = Instant::now();
            if next > now {
                std::thread::sleep(next - now);
            } else {
                next = now; // fell behind (or the rate was raised): no burst catch-up
            }
            if ctl.paused.load(Ordering::Relaxed) {
                continue;
            }
            for (v, (center, vol)) in vals.iter_mut().zip(SHAPE) {
                // Mean-reverting random walk, clamped to 0..100.
                *v = (*v + vol * noise(&mut rng) + 0.03 * (center - *v)).clamp(0.0, 100.0);
            }
            // Fire-and-forget across the IPC bridge (payload is JSON-serialized).
            let _ = app.emit("tick", Tick { seq, emitted_ms: wall_ms(), values: vals });
            seq += 1;
        }
    });
}

fn main() {
    // PULSE_HZ lets a headless run start at a non-default rate (e.g. 60) so the
    // IPC event path can be measured without scripting the slider.
    let hz: f64 = std::env::var("PULSE_HZ")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(10.0)
        .clamp(1.0, 60.0);
    let ctl = Arc::new(GenCtl {
        paused: AtomicBool::new(false),
        rate_mhz: AtomicU32::new((hz * 1000.0) as u32),
    });
    let gen_ctl = ctl.clone();
    tauri::Builder::default()
        .manage(ctl)
        .invoke_handler(tauri::generate_handler![get_config, set_rate, set_paused, report_stats])
        .setup(move |app| {
            spawn_generator(app.handle().clone(), gen_ctl);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
