//! Babel (xilem) — SPEC-5 text & i18n stress test.
//!
//! Two panes: a read-only rendering pane showing the shared multilingual
//! corpus (stock `prose` view → masonry `TextArea<false>` → parley 0.6 +
//! fontique system-font fallback → vello 0.6 glyph rendering incl. COLR/
//! bitmap emoji), and an editable pane (stock `text_input`) seeded with the
//! [MIXED] line. "Load big doc" swaps in the corpus repeated 1000×.

use std::time::Instant;

use xilem::masonry::properties::types::AsUnit;
use xilem::style::{Padding, Style as _};
use xilem::view::{
    flex_col, flex_row, label, portal, prose, sized_box, text_button, text_input, FlexExt as _,
    FlexSpacer,
};
use xilem::winit::dpi::{LogicalPosition, LogicalSize};
use xilem::winit::error::EventLoopError;
use xilem::{Color, EventLoop, InsertNewline, WidgetView, WindowOptions, Xilem};

const CORPUS: &str = include_str!("../../babel-assets/corpus.txt");
const BIG_REPEATS: usize = 1000;

struct Babel {
    /// Contents of the rendering pane.
    rendered: String,
    /// Status line: line count + how long the swap took.
    status: String,
    /// Editing pane contents (seeded with the [MIXED] corpus line).
    editor: String,
}

impl Babel {
    fn new() -> Self {
        let mixed = CORPUS
            .lines()
            .find(|l| l.starts_with("[MIXED]"))
            .unwrap_or("[MIXED] missing")
            .to_string();
        Self {
            rendered: CORPUS.to_string(),
            status: format!("{} lines", CORPUS.lines().count()),
            editor: mixed,
        }
    }

    fn load_big(&mut self) {
        let t0 = Instant::now();
        let mut big = String::with_capacity(CORPUS.len() * BIG_REPEATS);
        for _ in 0..BIG_REPEATS {
            big.push_str(CORPUS);
        }
        let lines = big.lines().count();
        self.rendered = big;
        self.status = format!("{lines} lines (string built in {:?})", t0.elapsed());
    }

    fn reset(&mut self) {
        self.rendered = CORPUS.to_string();
        self.status = format!("{} lines", CORPUS.lines().count());
    }
}

fn app_logic(state: &mut Babel) -> impl WidgetView<Babel> + use<> {
    let render_pane = sized_box(portal(
        prose(state.rendered.clone()).text_size(15.0),
    ))
    .expand()
    .background_color(Color::from_rgb8(0x1f, 0x1f, 0x23))
    .flex(1.0);

    let editor_pane = sized_box(portal(
        text_input(state.editor.clone(), |s: &mut Babel, t| s.editor = t)
            .insert_newline(InsertNewline::OnEnter),
    ))
    .expand_width()
    .height(160.px())
    .background_color(Color::from_rgb8(0x27, 0x27, 0x2a));

    let toolbar = flex_row((
        text_button("Load big doc", |s: &mut Babel| s.load_big()),
        text_button("Reset", |s: &mut Babel| s.reset()),
        FlexSpacer::Flex(1.0),
        label(state.status.clone()),
    ));

    flex_col((
        toolbar,
        render_pane,
        label("Editor (seeded with the [MIXED] line):"),
        editor_pane,
    ))
    .padding(Padding::all(8.0))
}

fn main() -> Result<(), EventLoopError> {
    let app = Xilem::new_simple(Babel::new(), app_logic, {
        let o = WindowOptions::new("Babel (xilem)")
            .with_initial_inner_size(LogicalSize::new(800.0, 600.0));
        // Optional fixed position for scripted interaction testing.
        match std::env::var("BABEL_POS").ok().and_then(|v| {
            let (x, y) = v.split_once(',')?;
            Some((x.parse::<f64>().ok()?, y.parse::<f64>().ok()?))
        }) {
            Some((x, y)) => o.with_initial_position(LogicalPosition::new(x, y)),
            None => o,
        }
    });
    app.run_in(EventLoop::with_user_event())
}
