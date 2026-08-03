//! "Babel" — text & i18n stress test (SPEC-5), vizia 0.4.
//!
//! Architecture note (research-relevant): vizia renders through Skia, and its
//! text stack is Skia's `SkParagraph` (`skia-safe` built with `textlayout`)
//! on top of the platform font manager — on macOS that is CoreText, so
//! shaping, BiDi resolution and per-script fallback are the system's. There
//! is no cosmic-text/fontdb layer to configure and nothing is bundled.
//!
//! Layout is stacked rather than side-by-side (SPEC-5 allows either): the
//! corpus lines are long, and at 800 px a two-pane split would wrap every
//! one of them, which would hide exactly the reordering behaviour the
//! screenshot is supposed to show.
//!
//! Verification hooks (research only, opt-in):
//!   BABEL_SELFTEST=1  grapheme/caret/selection probes on stderr, plus an
//!                     `EDIT ...` line on every editor change so synthetic
//!                     keystrokes can be verified from a captured log.
//!                     No resizing is needed: at the spec's 800x600 all 11
//!                     corpus lines fit unwrapped.

use unicode_segmentation::UnicodeSegmentation;
use vizia::prelude::*;

/// The shared corpus — embedded, never retyped.
const CORPUS: &str = include_str!("../../babel-assets/corpus.txt");

/// Repetitions used by "Load big doc" (11 lines x 1000 = 11,000 lines).
const BIG_DOC_REPEATS: usize = 1000;

/// Editor probe string: ASCII, a ZWJ family cluster, ASCII.
const PROBE: &str = "a👨‍👩‍👧‍👦b";

fn main() -> Result<(), ApplicationError> {
    let selftest = std::env::var_os("BABEL_SELFTEST").is_some();

    if selftest {
        probe_corpus();
    }

    Application::new(move |cx| {
        cx.add_stylesheet(STYLE).expect("failed to add stylesheet");

        let corpus_lines: Vec<String> =
            CORPUS.lines().filter(|line| !line.trim().is_empty()).map(str::to_owned).collect();
        let mixed = corpus_lines
            .iter()
            .find(|line| line.starts_with("[MIXED]"))
            .cloned()
            .unwrap_or_default();

        let lines = Signal::new(corpus_lines.clone());
        let editor = Signal::new(if selftest { PROBE.to_owned() } else { mixed });
        let status = Signal::new(format!("{} corpus lines", corpus_lines.len()));

        Babel { lines, editor, status, corpus: corpus_lines, selftest }.build(cx);

        VStack::new(cx, move |cx| {
            HStack::new(cx, |cx| {
                Label::new(cx, "Rendering pane — apps/babel-assets/corpus.txt")
                    .class("pane-title");
                Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));
                Button::new(cx, |cx| Label::new(cx, "Load big doc"))
                    .on_press(|cx| cx.emit(BabelEvent::LoadBigDoc));
                Label::new(cx, status).class("dim");
            })
            .class("bar");

            // Read-only rendering pane. Deliberately NOT virtualized (vizia
            // ships `VirtualList`, but using it here would measure the
            // virtualizer instead of the text stack — see FRICTION.md).
            ScrollView::new(cx, move |cx| {
                VStack::new(cx, move |cx| {
                    Binding::new(cx, lines, move |cx| {
                        for line in lines.get() {
                            Label::new(cx, line)
                                .class("corpus-line")
                                .text_wrap(false)
                                .hoverable(false);
                        }
                    });
                })
                .class("corpus");
            })
            .show_horizontal_scrollbar(true)
            .class("render-pane");

            Label::new(cx, "Editing pane — mouse + Shift+arrow selection, ⌘C/⌘V")
                .class("pane-title");

            Textbox::new_multiline(cx, editor, true)
                .class("editor")
                .on_edit(|cx, text| cx.emit(BabelEvent::SetEditor(text)));
        })
        .class("app");
    })
    .title("Babel (vizia)")
    .inner_size((800, 600))
    .run()
}

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

struct Babel {
    lines: Signal<Vec<String>>,
    editor: Signal<String>,
    status: Signal<String>,
    corpus: Vec<String>,
    selftest: bool,
}

enum BabelEvent {
    LoadBigDoc,
    SetEditor(String),
}

impl Model for Babel {
    fn event(&mut self, _cx: &mut EventContext, event: &mut Event) {
        event.take(|babel_event, _| match babel_event {
            BabelEvent::LoadBigDoc => {
                let started = std::time::Instant::now();
                let mut big = Vec::with_capacity(self.corpus.len() * BIG_DOC_REPEATS);
                for _ in 0..BIG_DOC_REPEATS {
                    big.extend(self.corpus.iter().cloned());
                }
                let count = big.len();
                self.lines.set(big);
                let ms = started.elapsed().as_secs_f64() * 1000.0;
                self.status.set(format!("{count} lines"));
                if self.selftest {
                    eprintln!("BIGDOC lines={count} generate_ms={ms:.2}");
                }
            }

            BabelEvent::SetEditor(text) => {
                if self.selftest {
                    eprintln!(
                        "EDIT text={:?} bytes={} chars={} graphemes={}",
                        text,
                        text.len(),
                        text.chars().count(),
                        text.graphemes(true).count()
                    );
                }
                self.editor.set(text);
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Verification probes (BABEL_SELFTEST=1) — stderr only, never in the UI
// ---------------------------------------------------------------------------

fn probe_corpus() {
    eprintln!("PROBE corpus_bytes={}", CORPUS.len());
    for line in CORPUS.lines().filter(|line| !line.trim().is_empty()) {
        let tag = line.split(']').next().unwrap_or("").trim_start_matches('[');
        eprintln!(
            "PROBE line={tag} bytes={} chars={} graphemes={} scripts={}",
            line.len(),
            line.chars().count(),
            line.graphemes(true).count(),
            script_mix(line)
        );
    }

    let family = "👨‍👩‍👧‍👦";
    eprintln!(
        "PROBE cluster=family bytes={} chars={} graphemes={} scalars={:?}",
        family.len(),
        family.chars().count(),
        family.graphemes(true).count(),
        family.chars().map(|c| format!("U+{:04X}", c as u32)).collect::<Vec<_>>()
    );
    eprintln!(
        "PROBE editor_seed={PROBE:?} graphemes={} (expect 3: 'a', family, 'b')",
        PROBE.graphemes(true).count()
    );
    eprintln!(
        "PROBE caret_expectation: one Backspace at end should leave {:?}",
        "a👨‍👩‍👧‍👦"
    );
}

/// Coarse script census of a line — enough to show a single paragraph really
/// does mix Latin, Arabic, Hebrew, CJK, Devanagari, Thai, Hangul and emoji.
fn script_mix(line: &str) -> usize {
    let mut seen = 0u16;
    for c in line.chars() {
        let bit = match c as u32 {
            0x0041..=0x024F => 1 << 0,  // Latin
            0x0590..=0x05FF => 1 << 1,  // Hebrew
            0x0600..=0x06FF => 1 << 2,  // Arabic
            0x0900..=0x097F => 1 << 3,  // Devanagari
            0x0E00..=0x0E7F => 1 << 4,  // Thai
            0x1100..=0x11FF | 0xAC00..=0xD7AF => 1 << 5, // Hangul
            0x3040..=0x30FF => 1 << 6,  // Kana
            0x4E00..=0x9FFF => 1 << 7,  // Han
            0x1F300..=0x1FAFF | 0x2600..=0x27BF => 1 << 8, // emoji-ish
            _ => 0,
        };
        seen |= bit;
    }
    seen.count_ones() as usize
}

// ---------------------------------------------------------------------------
// Style
// ---------------------------------------------------------------------------

const STYLE: &str = r#"
.app {
    width: 1s;
    height: 1s;
    padding: 10px;
    vertical-gap: 8px;
}

.bar { height: auto; horizontal-gap: 10px; alignment: center; }
.pane-title { height: auto; font-size: 12px; color: #8a8a8a; }
.dim { height: auto; font-size: 12px; color: #8a8a8a; }

.render-pane {
    width: 1s;
    height: 1s;
    border-width: 1px;
    border-color: #ffffff22;
    corner-radius: 6px;
}

.corpus { width: auto; height: auto; padding: 8px; vertical-gap: 4px; }
.corpus-line { height: auto; width: auto; font-size: 15px; }

.editor {
    width: 1s;
    height: 130px;
    font-size: 15px;
}
"#;
