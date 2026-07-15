//! "Babel" per apps/SPEC-5.md — text & i18n stress test for egui 0.35.
//!
//! egui has NO system-font discovery/fallback: every non-Latin script must
//! be bundled. This app bundles 6 Noto fonts (~18.1 MiB, see FRICTION.md).
//! epaint 0.35 shapes with `harfrust` (a Rust HarfBuzz port, GSUB/GPOS) but
//! has no paragraph-level Unicode BiDi reordering (individual RTL word runs
//! shape directionally; explicit `TODO(emilk): heed bidi characters`) and
//! rasterizes outlines only (no COLR/CBDT → emoji are monochrome).

use eframe::egui;
use unicode_segmentation::UnicodeSegmentation as _;

/// Shared corpus — embedded, not retyped (SPEC-5 §2).
const CORPUS: &str = include_str!("../../babel-assets/corpus.txt");

const FONT_SIZE: f32 = 15.0;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0])
            .with_position([60.0, 40.0]) // deterministic position for scripted screenshots
            .with_resizable(true),
        ..Default::default()
    };
    eframe::run_native(
        "Babel (egui)",
        options,
        Box::new(|cc| {
            install_fonts(&cc.egui_ctx);
            Ok(Box::new(BabelApp::new()))
        }),
    )
}

/// Bundle the minimal set of fonts needed for the corpus (egui cannot see
/// system fonts). Fallback resolution walks the family list in order and
/// picks the first face whose charmap contains the character, so Latin
/// stays on the default Ubuntu-Light and each script lands on its Noto font.
/// The full monochrome Noto Emoji is inserted *before* epaint's built-in
/// NotoEmoji subset so ZWJ/flag GSUB ligatures come from the complete font.
fn install_fonts(ctx: &egui::Context) {
    ctx.set_fonts(babel_font_definitions());
}

fn babel_font_definitions() -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();

    let bundled: &[(&str, &'static [u8])] = &[
        ("NotoEmojiFull", include_bytes!("../fonts/NotoEmoji-Regular.ttf")),
        ("NotoSansArabic", include_bytes!("../fonts/NotoSansArabic-Regular.ttf")),
        ("NotoSansHebrew", include_bytes!("../fonts/NotoSansHebrew-Regular.ttf")),
        ("NotoSansDevanagari", include_bytes!("../fonts/NotoSansDevanagari-Regular.ttf")),
        ("NotoSansThai", include_bytes!("../fonts/NotoSansThai-Regular.ttf")),
        ("NotoSansCJKsc", include_bytes!("../fonts/NotoSansCJKsc-Regular.otf")),
    ];
    for (name, bytes) in bundled {
        fonts.font_data.insert(
            (*name).to_owned(),
            std::sync::Arc::new(egui::FontData::from_static(bytes)),
        );
    }

    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        let list = fonts.families.entry(family).or_default();
        // Default proportional list is [Ubuntu-Light, NotoEmoji-Regular,
        // emoji-icon-font]; put the full emoji font right after the Latin
        // font, then the per-script fonts, keeping CJK late (largest
        // charmap; also contains Latin which must NOT win over Ubuntu).
        let mut insert_at = list.len().min(1);
        for (name, _) in bundled {
            list.insert(insert_at, (*name).to_owned());
            insert_at += 1;
        }
    }

    fonts
}

struct BabelApp {
    editor_text: String,
    /// `None` → show the 11-line corpus; `Some(lines)` → the "big doc"
    /// (corpus × 1000 ≈ 11k lines), rendered with a virtualized ScrollArea.
    big_doc: Option<Vec<String>>,
}

impl BabelApp {
    fn new() -> Self {
        let mixed = CORPUS
            .lines()
            .find(|l| l.starts_with("[MIXED]"))
            .unwrap_or("[MIXED] missing")
            .to_owned();
        Self {
            editor_text: mixed,
            big_doc: None,
        }
    }
}

impl eframe::App for BabelApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            // ---- controls ----
            ui.horizontal(|ui| {
                if ui.button("Load big doc").clicked() {
                    let lines: Vec<String> = std::iter::repeat_n(CORPUS.lines(), 1000)
                        .flatten()
                        .map(str::to_owned)
                        .collect();
                    self.big_doc = Some(lines);
                }
                if ui.button("Reset").clicked() {
                    self.big_doc = None;
                }
                let shown = self
                    .big_doc
                    .as_ref()
                    .map_or_else(|| CORPUS.lines().count(), Vec::len);
                ui.label(format!(
                    "{shown} lines · 6 bundled fonts ≈ 18.1 MiB (egui sees no system fonts)"
                ));
            });
            ui.separator();

            // ---- rendering pane (read-only, scrollable) ----
            let editor_pane_height = 150.0;
            let render_pane_height = ui.available_height() - editor_pane_height;
            let row_height = ui.fonts_mut(|f| {
                f.row_height(&egui::FontId::proportional(FONT_SIZE))
            }) + ui.spacing().item_spacing.y;

            let scroll = egui::ScrollArea::both()
                .id_salt("render-pane")
                .max_height(render_pane_height)
                .auto_shrink(false);
            match &self.big_doc {
                None => {
                    scroll.show(ui, |ui| {
                        for line in CORPUS.lines() {
                            ui.label(egui::RichText::new(line).size(FONT_SIZE));
                        }
                    });
                }
                Some(lines) => {
                    // Virtualized: only visible rows are laid out/shaped.
                    scroll.show_rows(ui, row_height, lines.len(), |ui, range| {
                        for line in &lines[range] {
                            ui.label(egui::RichText::new(line.as_str()).size(FONT_SIZE));
                        }
                    });
                }
            }

            ui.separator();

            // ---- editing pane ----
            ui.label("Editor (seeded with the [MIXED] corpus line):");
            ui.add(
                egui::TextEdit::multiline(&mut self.editor_text)
                    .font(egui::FontId::proportional(FONT_SIZE))
                    .desired_width(f32::INFINITY)
                    .desired_rows(3),
            );
            // Diagnostics for the grapheme/caret tests: scalar vs grapheme
            // counts make "did backspace delete the whole 👨‍👩‍👧‍👦?" observable.
            ui.label(format!(
                "editor: {} scalars · {} graphemes",
                self.editor_text.chars().count(),
                self.editor_text.graphemes(true).count(),
            ));
        });
    }
}

/// Objective text-stack probes: lay the corpus out with epaint directly and
/// inspect glyph positions/coverage (screenshots of RTL scripts are easy to
/// misread; glyph coordinates are not).
#[cfg(test)]
mod tests {
    use super::*;
    use egui::epaint::text::{Fonts, LayoutJob, TextOptions};

    fn fonts() -> Fonts {
        Fonts::new(TextOptions::default(), babel_font_definitions())
    }

    fn layout_line(fonts: &mut Fonts, text: &str) -> std::sync::Arc<egui::Galley> {
        let job = LayoutJob::simple_singleline(
            text.to_owned(),
            egui::FontId::proportional(FONT_SIZE),
            egui::Color32::BLACK,
        );
        fonts.with_pixels_per_point(1.0).layout_job(job)
    }

    /// Print each corpus line's glyphs in *visual* order (sorted by x).
    /// `⟪…⟫` wraps any char whose glyph came out of the layout in a
    /// different left-to-right position than its logical index (i.e. the
    /// engine moved it) — quick way to see what reordering, if any, exists.
    #[test]
    fn dump_visual_order() {
        let mut fonts = fonts();
        for line in CORPUS.lines() {
            let galley = layout_line(&mut fonts, line);
            let row = &galley.rows[0];
            let mut glyphs: Vec<(f32, char)> =
                row.glyphs.iter().map(|g| (g.pos.x, g.chr)).collect();
            glyphs.sort_by(|a, b| a.0.total_cmp(&b.0));
            let visual: String = glyphs.iter().map(|(_, c)| *c).collect();
            println!("LOGICAL: {line}");
            println!("VISUAL : {visual}");
            println!();
        }
    }

    /// Coverage: which bundled font owns each corpus char (`Font::characters`
    /// gives the ground-truth charmap union; `has_glyph` has false negatives
    /// for chars owned by the replacement-char's face).
    #[test]
    fn corpus_coverage_report() {
        let mut fonts = fonts();
        let font_id = egui::FontId::proportional(FONT_SIZE);
        let mut view = fonts.with_pixels_per_point(1.0);
        let owners = view.fonts.font(&font_id.family).characters().clone();
        let mut missing = Vec::new();
        for c in CORPUS.chars().filter(|c| !c.is_whitespace()) {
            if !owners.contains_key(&c) {
                missing.push(c);
            }
        }
        missing.sort_unstable();
        missing.dedup();
        println!("corpus chars with NO glyph in any bundled font: {missing:?}");
        for probe in ['👍', '👨', '😀', '\u{308}', 'ก', 'क', 'م', 'ש', '世', 'あ', '한'] {
            println!("{probe:?} owned by: {:?}", owners.get(&probe));
        }
    }

    /// Disambiguated visual order: only *advancing* glyphs (ligature
    /// continuations are zero-width and alias x positions, which made
    /// `dump_visual_order` unreadable for RTL). Prints each advancing glyph
    /// as `logical_index:char` sorted by x — monotone indices = NO
    /// reordering; reversed indices within a run = the shaper flipped it.
    #[test]
    fn visual_order_advancing_only() {
        let mut fonts = fonts();
        for line in CORPUS.lines().filter(|l| {
            l.starts_with("[AR]") || l.starts_with("[HE]") || l.starts_with("[MIXED]")
        }) {
            let galley = layout_line(&mut fonts, line);
            let row = &galley.rows[0];
            let mut glyphs: Vec<(f32, usize, char)> = row
                .glyphs
                .iter()
                .enumerate()
                .filter(|(_, g)| g.advance_width > 0.01)
                .map(|(i, g)| (g.pos.x, i, g.chr))
                .collect();
            glyphs.sort_by(|a, b| a.0.total_cmp(&b.0));
            println!("LOGICAL: {line}");
            let visual: String = glyphs.iter().map(|(_, _, c)| *c).collect();
            println!("VISUAL (advancing only): {visual}");
            let indices: Vec<usize> = glyphs.iter().map(|(_, i, _)| *i).collect();
            let monotone = indices.windows(2).all(|w| w[0] < w[1]);
            println!("indices monotone (no reordering): {monotone}");
            if !monotone {
                // where does order break?
                for w in indices.windows(2) {
                    if w[0] > w[1] {
                        println!("  reorder at logical {} -> {}", w[0], w[1]);
                    }
                }
            }
            println!();
        }
    }

    /// Digit ordering inside the RTL runs, immune to terminal BiDi: raw x
    /// per digit char. Correct rendering keeps digits LTR (x(١)<x(٢)<x(٣)).
    #[test]
    fn digit_order_in_rtl() {
        let mut fonts = fonts();
        let line = CORPUS.lines().find(|l| l.starts_with("[AR]")).unwrap();
        let galley = layout_line(&mut fonts, line);
        for g in galley.rows[0].glyphs.iter() {
            if matches!(g.chr, '١' | '٢' | '٣' | '4' | '5' | '6' | '7' | '8' | '9') {
                println!("U+{:04X} ({:?}) x={:.1}", g.chr as u32, g.chr, g.pos.x);
            }
        }
    }

    /// Does the ZWJ family sequence ligate (GSUB via harfrust)? epaint keeps
    /// one `Glyph` per char even for ligatures (zero-width continuations), so
    /// the tell is the advance pattern: ligated = one advancing glyph + rest
    /// zero-width; not ligated = several advancing glyphs.
    #[test]
    fn family_emoji_glyph_count() {
        let mut fonts = fonts();
        for probe in ["👨‍👩‍👧‍👦", "👍🏽", "🇺🇳", "🏳️‍🌈", "ffi"] {
            let galley = layout_line(&mut fonts, probe);
            let advances: Vec<f32> = galley
                .rows
                .iter()
                .flat_map(|r| r.glyphs.iter().map(|g| g.advance_width))
                .collect();
            let advancing = advances.iter().filter(|a| **a > 0.01).count();
            println!(
                "{probe}: {} scalars -> {} glyphs, {advancing} advancing {advances:?}",
                probe.chars().count(),
                advances.len(),
            );
        }
    }
}
