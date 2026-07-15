// Grid (Tauri) — SPEC-7 100k-row data table, RCN GUI ecosystem research.
//
// Boundary decision (see FRICTION.md): the 100,000 rows live in the Rust core
// process. The webview never holds the dataset — it is a windowed view that
// asks `get_rows(start, count)` for the slice the viewport needs, and filter/
// sort run in Rust over an index vector (`view`). This deliberately uses the
// IPC bridge as the virtualization backplane. FILTER_MS is printed here
// because the filter runs here; BUILD_MS is printed once from main().

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Mutex;
use std::time::Instant;

use serde::Serialize;
use tauri::State;

const N_ROWS: usize = 100_000;

const ADJECTIVES: [&str; 16] = [
    "amber", "brisk", "coral", "dusty", "eager", "fuzzy", "glossy", "hazel",
    "icy", "jolly", "keen", "lunar", "mossy", "noble", "opal", "prime",
];
const NOUNS: [&str; 16] = [
    "anchor", "beacon", "cobalt", "delta", "ember", "falcon", "garnet",
    "harbor", "island", "jasper", "kernel", "lagoon", "marble", "nectar",
    "orbit", "prism",
];
const CATEGORIES: [&str; 8] = [
    "Alpha", "Bravo", "Charlie", "Delta", "Echo", "Foxtrot", "Golf", "Hotel",
];
const STATUSES: [&str; 3] = ["Ok", "Warn", "Err"];

/// xorshift64* — seeded, deterministic across runs; not worth a `rand` dep.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

struct Row {
    id: u32,
    name: String, // "adjective-noun-####" — all-lowercase by construction
    category: u8, // index into CATEGORIES
    value: f64,   // 2-decimal money-ish value
    date: String, // ISO yyyy-mm-dd
    status: u8,   // index into STATUSES
}

fn generate_rows() -> Vec<Row> {
    let mut rng = Rng(0x5EED_1BAD_F00D_D00D);
    (0..N_ROWS)
        .map(|i| {
            let name = format!(
                "{}-{}-{:04}",
                ADJECTIVES[rng.below(16) as usize],
                NOUNS[rng.below(16) as usize],
                rng.below(10_000)
            );
            let value = rng.below(1_000_000) as f64 / 100.0;
            let date = format!(
                "{:04}-{:02}-{:02}",
                2020 + rng.below(6),
                1 + rng.below(12),
                1 + rng.below(28) // 1..=28: sidesteps month-length/leap logic
            );
            let status = match rng.below(10) {
                0..=6 => 0, // Ok ~70%
                7..=8 => 1, // Warn ~20%
                _ => 2,     // Err ~10%
            };
            Row {
                id: i as u32 + 1,
                name,
                category: rng.below(8) as u8,
                value,
                date,
                status,
            }
        })
        .collect()
}

/// Canonical grid state: full dataset + the current view (filtered + sorted
/// index vector). Rebuilt in full on every filter/sort change.
struct GridInner {
    rows: Vec<Row>,
    view: Vec<u32>,
    filter: String,        // lowercased substring query on `name`
    sort: Option<(u8, bool)>, // (column 0..=5, ascending)
}

fn rebuild_view(g: &mut GridInner) {
    let q = g.filter.as_str();
    let mut view: Vec<u32> = (0..g.rows.len() as u32)
        .filter(|&i| q.is_empty() || g.rows[i as usize].name.contains(q))
        .collect();
    if let Some((col, asc)) = g.sort {
        let rows = &g.rows;
        view.sort_unstable_by(|&a, &b| {
            let (ra, rb) = (&rows[a as usize], &rows[b as usize]);
            let ord = match col {
                0 => ra.id.cmp(&rb.id),
                1 => ra.name.cmp(&rb.name),
                2 => ra.category.cmp(&rb.category).then(ra.id.cmp(&rb.id)),
                3 => ra.value.total_cmp(&rb.value).then(ra.id.cmp(&rb.id)),
                4 => ra.date.cmp(&rb.date).then(ra.id.cmp(&rb.id)),
                _ => ra.status.cmp(&rb.status).then(ra.id.cmp(&rb.id)),
            };
            if asc {
                ord
            } else {
                ord.reverse()
            }
        });
    }
    g.view = view;
}

struct AppState(Mutex<GridInner>);

#[derive(Serialize)]
struct RowDto {
    // `vi` (view index) lets the frontend place/select rows without holding
    // any global state beyond what it fetched.
    vi: usize,
    id: u32,
    name: String,
    category: &'static str,
    value: String, // preformatted "12345.67" — display-only in the webview
    date: String,
    status: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Meta {
    total: usize,
    view_len: usize,
    selftest: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SortResult {
    col: u8,
    asc: bool,
    view_len: usize,
}

#[tauri::command]
fn get_meta(state: State<'_, AppState>) -> Meta {
    let g = state.0.lock().unwrap();
    Meta {
        total: g.rows.len(),
        view_len: g.view.len(),
        selftest: std::env::var("GRID_SELFTEST").is_ok(),
    }
}

/// Windowed query: rows [start, start+count) of the CURRENT view (filtered +
/// sorted). This is the virtualization backplane — the webview calls it as
/// the viewport scrolls.
#[tauri::command]
fn get_rows(start: usize, count: usize, state: State<'_, AppState>) -> Vec<RowDto> {
    let g = state.0.lock().unwrap();
    let start = start.min(g.view.len());
    let end = start.saturating_add(count.min(512)).min(g.view.len());
    g.view[start..end]
        .iter()
        .enumerate()
        .map(|(off, &ri)| {
            let r = &g.rows[ri as usize];
            RowDto {
                vi: start + off,
                id: r.id,
                name: r.name.clone(),
                category: CATEGORIES[r.category as usize],
                value: format!("{:.2}", r.value),
                date: r.date.clone(),
                status: STATUSES[r.status as usize],
            }
        })
        .collect()
}

/// Filter-as-you-type target. Times the full view rebuild (substring scan +
/// re-sort) and prints `FILTER_MS <query_len> <ms>` to stdout (SPEC-7 §5).
#[tauri::command]
fn set_filter(q: String, state: State<'_, AppState>) -> usize {
    let mut g = state.0.lock().unwrap();
    let t0 = Instant::now();
    g.filter = q.trim().to_lowercase();
    rebuild_view(&mut g);
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    println!("FILTER_MS {} {:.3}", g.filter.chars().count(), ms);
    g.view.len()
}

/// Click a header: first click sorts asc, clicking the same column again
/// toggles direction (SPEC-7 §4).
#[tauri::command]
fn set_sort(col: u8, state: State<'_, AppState>) -> SortResult {
    let mut g = state.0.lock().unwrap();
    let asc = match g.sort {
        Some((c, a)) if c == col => !a,
        _ => true,
    };
    g.sort = Some((col, asc));
    rebuild_view(&mut g);
    SortResult {
        col,
        asc,
        view_len: g.view.len(),
    }
}

/// Webview console → stdout pipe. Used by the self-test harness and
/// window.onerror so a headless launch leaves auditable evidence.
#[tauri::command]
fn report(line: String) {
    println!("{line}");
}

fn main() {
    let t0 = Instant::now();
    let mut inner = GridInner {
        rows: generate_rows(),
        view: Vec::new(),
        filter: String::new(),
        sort: None,
    };
    rebuild_view(&mut inner);
    // SPEC-7 §8: data generation + initial model (view) build.
    println!("BUILD_MS {:.3}", t0.elapsed().as_secs_f64() * 1000.0);

    tauri::Builder::default()
        .manage(AppState(Mutex::new(inner)))
        .invoke_handler(tauri::generate_handler![
            get_meta, get_rows, set_filter, set_sort, report
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
