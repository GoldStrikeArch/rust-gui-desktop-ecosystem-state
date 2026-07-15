//! "Grid" per apps/SPEC-7.md — 100k-row virtualized data table in egui 0.35
//! (eframe shell, egui_extras::TableBuilder for the table itself).
//!
//! Evidence printed to stdout (captured in verify-stdout.log):
//! - `BUILD_MS <ms>` once at startup (data generation + initial model build)
//! - `FILTER_MS <query_len> <ms>` on every filter application
//!
//! `GRID_SELFTEST=1` runs a scripted driver (src/selftest.rs, verification
//! code): applies 1-char and 4-char filters, sorts, then jump-scrolls the
//! whole 100k range to exercise virtualization for the RSS measurement.

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use std::collections::HashSet;
use std::time::Instant;

mod selftest;

const ROW_COUNT: usize = 100_000;

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
    "Retail", "Logistics", "Finance", "Energy", "Health", "Media", "Cloud",
    "Gaming",
];
const COLUMN_TITLES: [&str; 6] = ["ID", "Name", "Category", "Value", "Date", "Status"];

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 640.0])
            .with_resizable(true),
        ..Default::default()
    };
    eframe::run_native(
        "Grid (egui)",
        options,
        Box::new(|_cc| Ok(Box::new(GridApp::new()))),
    )
}

/// SplitMix64 — tiny seeded PRNG, deterministic across runs/platforms.
/// Hand-rolled (10 LoC) instead of pulling in the `rand` stack.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Status {
    Ok,
    Warn,
    Err,
}

struct Row {
    id: u32,
    name: String,
    category: u8, // index into CATEGORIES
    value: f64,
    date: String, // ISO yyyy-mm-dd
    status: Status,
}

fn generate_rows() -> Vec<Row> {
    let mut rng = Rng(0x5EED_2026_0709_0007);
    (0..ROW_COUNT)
        .map(|i| {
            let name = format!(
                "{}-{}-{:04}",
                ADJECTIVES[rng.below(16) as usize],
                NOUNS[rng.below(16) as usize],
                rng.below(10_000)
            );
            let date = format!(
                "{:04}-{:02}-{:02}",
                2020 + rng.below(6),
                1 + rng.below(12),
                1 + rng.below(28)
            );
            Row {
                id: i as u32,
                name,
                category: rng.below(8) as u8,
                value: rng.below(1_000_000) as f64 / 100.0,
                date,
                status: match rng.below(10) {
                    0..=6 => Status::Ok,
                    7..=8 => Status::Warn,
                    _ => Status::Err,
                },
            }
        })
        .collect()
}

struct GridApp {
    rows: Vec<Row>,
    filter: String,
    /// Display model: indices into `rows`, filtered then sorted.
    visible: Vec<u32>,
    /// (column index, ascending)
    sort: Option<(usize, bool)>,
    /// Selected rows, by index into `rows` (survives re-sort/re-filter).
    selected: HashSet<u32>,
    /// Shift-click range anchor, as an index into `visible`.
    anchor: Option<usize>,
    /// One-shot programmatic scroll target (used by the self-test driver).
    pending_scroll: Option<usize>,
    selftest: Option<selftest::SelfTest>,
}

impl GridApp {
    fn new() -> Self {
        let t0 = Instant::now();
        let rows = generate_rows();
        let visible: Vec<u32> = (0..rows.len() as u32).collect();
        println!("BUILD_MS {:.2}", t0.elapsed().as_secs_f64() * 1000.0);
        Self {
            rows,
            filter: String::new(),
            visible,
            sort: None,
            selected: HashSet::new(),
            anchor: None,
            pending_scroll: None,
            selftest: selftest::SelfTest::from_env(),
        }
    }

    /// Re-derive `visible` from the filter, re-apply the sort, and print the
    /// self-timed `FILTER_MS <query_len> <ms>` evidence line.
    fn apply_filter(&mut self) {
        let t0 = Instant::now();
        let query = self.filter.to_lowercase();
        self.visible = (0..self.rows.len() as u32)
            .filter(|&i| {
                query.is_empty() || self.rows[i as usize].name.contains(&query)
            })
            .collect();
        self.resort();
        self.anchor = None;
        println!(
            "FILTER_MS {} {:.2}",
            self.filter.chars().count(),
            t0.elapsed().as_secs_f64() * 1000.0
        );
    }

    /// Click on header `col`: sort ascending, or flip direction if already
    /// sorted by that column.
    fn toggle_sort(&mut self, col: usize) {
        self.sort = match self.sort {
            Some((c, asc)) if c == col => Some((c, !asc)),
            _ => Some((col, true)),
        };
        self.resort();
        self.anchor = None;
    }

    fn resort(&mut self) {
        let Some((col, asc)) = self.sort else { return };
        let rows = &self.rows;
        self.visible.sort_unstable_by(|&a, &b| {
            let (ra, rb) = (&rows[a as usize], &rows[b as usize]);
            let ord = match col {
                0 => ra.id.cmp(&rb.id),
                1 => ra.name.cmp(&rb.name),
                2 => CATEGORIES[ra.category as usize].cmp(CATEGORIES[rb.category as usize]),
                3 => ra.value.total_cmp(&rb.value),
                4 => ra.date.cmp(&rb.date), // ISO dates sort lexicographically
                _ => ra.status.cmp(&rb.status),
            };
            if asc { ord } else { ord.reverse() }
        });
    }

    /// Selection semantics: click = select only; Shift-click = range from
    /// anchor; Cmd/Ctrl-click = toggle. `vi` is an index into `visible`.
    fn handle_row_click(&mut self, vi: usize, modifiers: egui::Modifiers) {
        let di = self.visible[vi];
        if modifiers.shift {
            if let Some(a) = self.anchor {
                let (lo, hi) = (a.min(vi), a.max(vi));
                self.selected = self.visible[lo..=hi].iter().copied().collect();
                return; // keep the anchor for further range tweaks
            }
        }
        if modifiers.command {
            if !self.selected.remove(&di) {
                self.selected.insert(di);
            }
        } else {
            self.selected.clear();
            self.selected.insert(di);
        }
        self.anchor = Some(vi);
    }

    /// Whole UI; split out from `eframe::App::ui` so egui_kittest can drive
    /// it headlessly.
    fn show(&mut self, ui: &mut egui::Ui) {
        if self.selftest.is_some() {
            selftest::drive(ui.ctx(), self);
        }

        // egui 0.35 unified Top/Side panels into `egui::Panel`.
        egui::Panel::top("controls").show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Filter:");
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.filter)
                        .hint_text("substring of name…")
                        .desired_width(240.0),
                );
                if response.changed() {
                    self.apply_filter();
                }
                ui.label(format!(
                    "{} of {} rows",
                    thousands(self.visible.len()),
                    thousands(ROW_COUNT)
                ));
                ui.separator();
                ui.label(format!("{} selected", self.selected.len()));
                ui.weak("(click / shift-click range / cmd-click toggle)");
            });
            ui.add_space(4.0);
        });
        egui::CentralPanel::default().show(ui, |ui| self.show_table(ui));
    }

    fn show_table(&mut self, ui: &mut egui::Ui) {
        // Cell text must NOT be selectable: selectable labels sense
        // click+drag (for text selection) and steal clicks on the text from
        // the row response (found via kittest: clicks on cell text never
        // reached `TableRow::response()`).
        ui.style_mut().interaction.selectable_labels = false;

        let mut header_clicked: Option<usize> = None;
        let mut row_clicked: Option<usize> = None;
        let row_height = egui::TextStyle::Body.resolve(ui.style()).size + 6.0;
        let sort = self.sort;

        let mut builder = TableBuilder::new(ui)
            .striped(true)
            .resizable(true) // drag header dividers to resize columns
            .sense(egui::Sense::click()) // whole-row click responses
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::initial(64.0).at_least(40.0)) // ID
            .column(Column::initial(220.0).at_least(80.0).clip(true)) // Name
            .column(Column::initial(96.0).at_least(60.0)) // Category
            .column(Column::initial(96.0).at_least(60.0)) // Value
            .column(Column::initial(110.0).at_least(80.0)) // Date
            .column(Column::remainder().at_least(70.0)); // Status
        if let Some(target) = self.pending_scroll.take() {
            builder = builder
                .scroll_to_row(target, Some(egui::Align::Center))
                .animate_scrolling(false);
        }

        builder
            .header(24.0, |mut header| {
                for (ci, title) in COLUMN_TITLES.iter().enumerate() {
                    header.col(|ui| {
                        // Sort indicator is hand-assembled: text suffix on a
                        // selectable label acting as the click target.
                        let is_sorted = matches!(sort, Some((c, _)) if c == ci);
                        let text = match sort {
                            Some((c, asc)) if c == ci => {
                                format!("{title} {}", if asc { "▲" } else { "▼" })
                            }
                            _ => (*title).to_string(),
                        };
                        if ui
                            .selectable_label(is_sorted, egui::RichText::new(text).strong())
                            .clicked()
                        {
                            header_clicked = Some(ci);
                        }
                    });
                }
            })
            .body(|body| {
                let rows = &self.rows;
                let visible = &self.visible;
                let selected = &self.selected;
                // `rows` is the virtualized path: only on-screen rows get
                // laid out; the rest contribute height only.
                body.rows(row_height, visible.len(), |mut table_row| {
                    let vi = table_row.index();
                    let row = &rows[visible[vi] as usize];
                    table_row.set_selected(selected.contains(&visible[vi]));
                    table_row.col(|ui| {
                        ui.label(row.id.to_string());
                    });
                    table_row.col(|ui| {
                        ui.label(&row.name);
                    });
                    table_row.col(|ui| {
                        ui.label(CATEGORIES[row.category as usize]);
                    });
                    table_row.col(|ui| {
                        // Right-aligned numeric cell.
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                ui.label(format!("{:.2}", row.value));
                            },
                        );
                    });
                    table_row.col(|ui| {
                        ui.label(&row.date);
                    });
                    table_row.col(|ui| status_chip(ui, row.status));
                    if table_row.response().clicked() {
                        row_clicked = Some(vi);
                    }
                });
            });

        if let Some(ci) = header_clicked {
            self.toggle_sort(ci);
        }
        if let Some(vi) = row_clicked {
            let modifiers = ui.input(|i| i.modifiers);
            self.handle_row_click(vi, modifiers);
        }
    }
}

/// Custom cell rendering: colored status chip (rounded filled frame).
fn status_chip(ui: &mut egui::Ui, status: Status) {
    let (text, bg) = match status {
        Status::Ok => ("Ok", egui::Color32::from_rgb(0x2e, 0x7d, 0x32)),
        Status::Warn => ("Warn", egui::Color32::from_rgb(0xb8, 0x86, 0x0b)),
        Status::Err => ("Err", egui::Color32::from_rgb(0xc6, 0x28, 0x28)),
    };
    egui::Frame::new()
        .fill(bg)
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(8, 1))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(text)
                    .color(egui::Color32::WHITE)
                    .size(11.0),
            );
        });
}

fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

impl eframe::App for GridApp {
    // egui 0.35: `App::ui` (replacing `App::update` since 0.34) hands us the
    // root `Ui` of the viewport.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    fn new_harness() -> Harness<'static, GridApp> {
        let mut harness = Harness::builder()
            .with_size(egui::Vec2::new(1000.0, 640.0))
            .build_ui_state(|ui, app: &mut GridApp| app.show(ui), GridApp::new());
        harness.run();
        harness
    }

    /// Filter typing goes through the real TextEdit (AccessKit tree), so this
    /// exercises the same `changed()` → `apply_filter` path as a user.
    #[test]
    fn filter_narrows_to_matching_names() {
        let mut harness = new_harness();
        let expected = harness
            .state()
            .rows
            .iter()
            .filter(|r| r.name.contains("amber"))
            .count();
        assert!(expected > 0, "seeded data should contain 'amber' names");

        let input = harness.get_by_role(egui::accesskit::Role::TextInput);
        input.focus();
        input.type_text("amber");
        harness.run();

        assert_eq!(harness.state().visible.len(), expected);
        // Counter label is exposed to the a11y tree.
        harness.get_by_label(&format!("{} of 100,000 rows", thousands(expected)));
    }

    /// Header click sorts ascending; second click flips to descending with
    /// the indicator in the label.
    #[test]
    fn header_click_sorts_and_flips() {
        let mut harness = new_harness();

        harness.get_by_label_contains("Value").click();
        harness.run();
        assert_eq!(harness.state().sort, Some((3, true)));
        {
            let state = harness.state();
            let first = state.rows[state.visible[0] as usize].value;
            let last = state.rows[*state.visible.last().unwrap() as usize].value;
            assert!(first <= last);
        }

        harness.get_by_label_contains("Value ▲").click();
        harness.run();
        assert_eq!(harness.state().sort, Some((3, false)));
        let state = harness.state();
        let first = state.rows[state.visible[0] as usize].value;
        let last = state.rows[*state.visible.last().unwrap() as usize].value;
        assert!(first >= last);
    }

    /// Row click selects through the real row Response (kittest can't hold
    /// Shift during a click, so plain click goes through the UI and the
    /// shift/cmd variants are exercised at the model level below).
    #[test]
    fn row_click_selects_via_ui() {
        let mut harness = new_harness();

        let first_name = harness.state().rows[0].name.clone();
        harness.get_all_by_label(&first_name).next().unwrap().click();
        harness.run();
        assert_eq!(harness.state().selected.len(), 1);
        assert!(harness.state().selected.contains(&0));

        // Shift-click row 4 through the UI -> contiguous range 0..=4.
        // (Modifiers must go into RawInput: `handle_row_click` reads
        // `ui.input(|i| i.modifiers)`, not the pointer event.)
        let fifth_name = harness.state().rows[4].name.clone();
        harness.input_mut().modifiers = egui::Modifiers { shift: true, ..Default::default() };
        harness.get_all_by_label(&fifth_name).next().unwrap().click();
        harness.run();
        harness.input_mut().modifiers = egui::Modifiers::default();
        assert_eq!(harness.state().selected, HashSet::from([0, 1, 2, 3, 4]));
    }

    /// Selection semantics: plain, shift-range, cmd-toggle.
    #[test]
    fn selection_model_shift_range_and_cmd_toggle() {
        let mut app = GridApp::new();
        let plain = egui::Modifiers::default();
        let shift = egui::Modifiers { shift: true, ..Default::default() };
        let command = egui::Modifiers { command: true, ..Default::default() };

        app.handle_row_click(10, plain);
        assert_eq!(app.selected, HashSet::from([10]));

        app.handle_row_click(14, shift); // range 10..=14
        assert_eq!(app.selected.len(), 5);
        assert!(app.selected.contains(&12));

        app.handle_row_click(20, command); // toggle on
        assert!(app.selected.contains(&20));
        app.handle_row_click(20, command); // toggle off
        assert!(!app.selected.contains(&20));
    }

    /// Drag the divider between the ID and Name header cells ~40 px to the
    /// right and assert the Name column moved. The divider's exact x is not
    /// exposed, so probe a few positions left of the Name header (the
    /// grab zone is +/- resize_grab_radius_side around the line).
    #[test]
    fn column_resize_by_divider_drag() {
        let mut harness = new_harness();
        let header_y = harness.get_by_label("ID").rect().center().y;
        let name_left_before = harness.get_by_label("Name").rect().left();

        let mut moved = false;
        let mut x = name_left_before - 12.0;
        while x <= name_left_before - 2.0 {
            let from = egui::pos2(x, header_y);
            harness.hover_at(from);
            harness.step();
            harness.drag_at(from);
            harness.step();
            for dx in [8.0, 24.0, 40.0] {
                harness.event(egui::Event::PointerMoved(from + egui::vec2(dx, 0.0)));
                harness.step();
            }
            harness.drop_at(from + egui::vec2(40.0, 0.0));
            harness.step();
            harness.step();
            let name_left_after = harness.get_by_label("Name").rect().left();
            if name_left_after > name_left_before + 25.0 {
                moved = true;
                break;
            }
            x += 2.0;
        }
        assert!(moved, "no probe position resized the ID column");
    }

    /// Rasterize the real UI (wgpu offscreen) and keep it as visual
    /// evidence (screenshot-kittest.png): table grid, chips, header.
    #[test]
    fn render_screenshot_evidence() {
        let mut harness = new_harness();
        let image = harness.render().expect("wgpu offscreen render");
        image
            .save(concat!(env!("CARGO_MANIFEST_DIR"), "/screenshot-kittest.png"))
            .expect("save png");
    }
}

