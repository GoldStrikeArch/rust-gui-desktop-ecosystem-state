//! "Babel" — text & i18n stress test in gpui 0.2.2 (SPEC-5).
//!
//! Rendering pane: the shared corpus (embedded via `include_str!`), one
//! paragraph per row, in a `uniform_list` — gpui's virtualized list (the
//! same element Zed uses for big lists), which only lays out/paints the
//! visible rows. "Load big doc" swaps in the corpus repeated 1000×
//! (11,000 lines) behind the same list.
//!
//! Editing pane: the hand-rolled multiline editor from gpui-tray
//! (editor.rs — EntityInputHandler + custom Element; gpui ships no text
//! widget), seeded with the [MIXED] line.
//!
//! Text stack on macOS: gpui shapes with CoreText (system font +
//! CTFontCreateForString-style fallback cascade) and rasterizes color emoji
//! natively, so no fonts are bundled — what the screenshot shows is the
//! platform text stack as exposed by gpui.

mod editor;

use gpui::{
    App, Application, Bounds, Context, Entity, SharedString, Subscription, TitlebarOptions,
    Window, WindowAppearance, WindowBounds, WindowOptions, div, prelude::*, px, rgb, size,
    uniform_list,
};

use editor::Editor;

/// The shared corpus — embedded, not retyped (SPEC-5 rule).
const CORPUS: &str = include_str!("../../babel-assets/corpus.txt");

struct Theme {
    bg: gpui::Rgba,
    panel: gpui::Rgba,
    border: gpui::Rgba,
    text: gpui::Rgba,
    dim: gpui::Rgba,
    accent: gpui::Rgba,
    stripe: gpui::Rgba,
}

fn theme(appearance: WindowAppearance) -> Theme {
    match appearance {
        WindowAppearance::Dark | WindowAppearance::VibrantDark => Theme {
            bg: rgb(0x1e1e21),
            panel: rgb(0x27272b),
            border: rgb(0x3f3f46),
            text: rgb(0xe4e4e7),
            dim: rgb(0x9f9fa8),
            accent: rgb(0x60a5fa),
            stripe: rgb(0x232327),
        },
        _ => Theme {
            bg: rgb(0xf4f4f5),
            panel: rgb(0xffffff),
            border: rgb(0xd4d4d8),
            text: rgb(0x18181b),
            dim: rgb(0x6b7280),
            accent: rgb(0x2563eb),
            stripe: rgb(0xfafafa),
        },
    }
}

struct BabelApp {
    /// Lines shown in the rendering pane (11 corpus lines, or 11,000).
    doc: Vec<SharedString>,
    big: bool,
    editor: Entity<Editor>,
    _appearance_sub: Subscription,
}

fn corpus_lines() -> Vec<SharedString> {
    CORPUS
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| SharedString::from(l.to_string()))
        .collect()
}

impl BabelApp {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mixed = corpus_lines()
            .iter()
            .find(|l| l.starts_with("[MIXED]"))
            .cloned()
            .unwrap_or_default();
        let editor = cx.new(|cx| Editor::new(cx, mixed.as_ref(), "Edit here…"));
        editor.read(cx).focus_handle.focus(window);
        let sub = window.observe_window_appearance(|_, cx| cx.refresh_windows());
        Self {
            doc: corpus_lines(),
            big: false,
            editor,
            _appearance_sub: sub,
        }
    }

    fn toggle_big_doc(&mut self, cx: &mut Context<Self>) {
        if self.big {
            self.doc = corpus_lines();
            self.big = false;
        } else {
            // Corpus repeated ~1000× ≈ 11k lines, generated in code.
            let base = corpus_lines();
            let mut doc = Vec::with_capacity(base.len() * 1000);
            for rep in 0..1000 {
                for line in &base {
                    // Prefix with the repetition so scrolled content visibly
                    // changes (and each row is a distinct SharedString).
                    doc.push(SharedString::from(format!("{rep:04} {line}")));
                }
            }
            self.doc = doc;
            self.big = true;
        }
        cx.notify();
    }
}

impl Render for BabelApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(window.appearance());
        let line_count = self.doc.len();
        let entity = cx.entity();

        // --- Rendering pane: virtualized read-only list -------------------
        let stripe = t.stripe;
        let text_color = t.text;
        let rendering_pane = uniform_list(
            "corpus-list",
            line_count,
            move |range, _window, cx: &mut App| {
                let app = entity.read(cx);
                range
                    .map(|ix| {
                        div()
                            .h(px(24.))
                            .px_3()
                            .flex()
                            .items_center()
                            .when(ix % 2 == 1, |d| d.bg(stripe))
                            .text_size(px(15.))
                            .text_color(text_color)
                            .whitespace_nowrap()
                            .child(app.doc[ix].clone())
                    })
                    .collect::<Vec<_>>()
            },
        )
        .flex_1()
        .border_1()
        .border_color(t.border)
        .rounded_md()
        .bg(t.panel);

        // --- Toolbar -------------------------------------------------------
        let toolbar = div()
            .flex_none()
            .flex()
            .items_center()
            .gap_3()
            .child(
                div()
                    .id("big-doc")
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .bg(t.accent)
                    .text_color(gpui::white())
                    .text_sm()
                    .cursor_pointer()
                    .hover(|s| s.opacity(0.85))
                    .child(if self.big {
                        "Reset to corpus"
                    } else {
                        "Load big doc"
                    })
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_big_doc(cx))),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(t.dim)
                    .child(format!("{line_count} lines · CoreText via gpui, system fonts, nothing bundled")),
            );

        // --- Editing pane ---------------------------------------------------
        let editor_pane = div()
            .flex_none()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_sm()
                    .text_color(t.dim)
                    .child("Editor (hand-rolled) — seeded with [MIXED]; selection / caret / IME here"),
            )
            .child(
                div()
                    .id("editor-scroll")
                    .h(px(120.))
                    .overflow_y_scroll()
                    .rounded_md()
                    .border_1()
                    .border_color(t.border)
                    .bg(t.panel)
                    .child(div().p_2().text_size(px(15.)).child(self.editor.clone())),
            );

        div()
            .size_full()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .bg(t.bg)
            .text_color(t.text)
            .line_height(px(24.))
            .child(rendering_pane)
            .child(toolbar)
            .child(editor_pane)
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        editor::bind_editor_keys(cx);

        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        // A bit wider than the spec's ~800×600 so the longest corpus line
        // fits unclipped for the comparison screenshot (allowed by SPEC-5).
        let bounds = Bounds::centered(None, size(px(980.), px(640.)), cx);
        cx.open_window(
            WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some(SharedString::from("Babel (gpui)")),
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| BabelApp::new(window, cx)),
        )
        .unwrap();
        cx.activate(true);
    });
}
