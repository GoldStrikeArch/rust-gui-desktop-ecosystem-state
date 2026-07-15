//! "Babel" — SPEC-5 text & i18n stress test.
//! Dioxus 0.7.9 desktop (wry/tao webview renderer), plain cargo (no dx CLI).
//!
//! The whole text stack (shaping, BiDi, font fallback, grapheme-cluster
//! editing, IME) is WebKit's; Dioxus contributes RSX + signals only.
//! No helper crates, no bundled fonts — system fonts + automatic fallback.
//!
//! Self-test hooks (used by the launch verification, harmless otherwise):
//! - env BABEL_BIG=1     -> start with the big (~11k line) document loaded
//! - env BABEL_SCROLL=1  -> run a rAF-timed programmatic scroll probe over the
//!   rendering pane and print "SCROLL_PROBE {frames,ms}" to stdout.

use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
use dioxus::prelude::*;

/// Shared corpus, embedded — do not retype (SPEC-5 rule).
static CORPUS: &str = include_str!("../../babel-assets/corpus.txt");

fn main() {
    dioxus::LaunchBuilder::desktop()
        .with_cfg(
            Config::new().with_window(
                WindowBuilder::new()
                    .with_title("Babel (dioxus)")
                    .with_inner_size(LogicalSize::new(880.0, 620.0))
                    .with_resizable(true),
            ),
        )
        .launch(App);
}

#[component]
fn App() -> Element {
    // Editing pane is seeded with the [MIXED] corpus line.
    let mixed = CORPUS
        .lines()
        .find(|l| l.starts_with("[MIXED]"))
        .unwrap_or("");
    let mut editor = use_signal(|| mixed.to_string());
    let mut big = use_signal(|| std::env::var_os("BABEL_BIG").is_some());

    // Rendering pane content: the corpus, or the corpus repeated ~1000x
    // (≈11k lines) for the large-document test. One text node in one <pre>.
    let doc = use_memo(move || {
        if big() {
            CORPUS.repeat(1000)
        } else {
            CORPUS.to_string()
        }
    });
    let line_count = use_memo(move || doc.read().lines().count());

    // Optional scripted scroll probe: smooth-scrolls the rendering pane via
    // requestAnimationFrame and reports achieved frame count/duration.
    use_future(move || async move {
        if std::env::var_os("BABEL_SCROLL").is_none() {
            return;
        }
        let mut eval = document::eval(
            r#"
            // Wait 4s so the window can be focused first (unfocused WKWebView
            // throttles requestAnimationFrame, which would skew the number).
            setTimeout(() => {
                const el = document.querySelector('.render');
                let frames = 0;
                const t0 = performance.now();
                function step() {
                    el.scrollTop += 48;
                    frames += 1;
                    const ms = performance.now() - t0;
                    if (ms < 3000 && el.scrollTop + el.clientHeight < el.scrollHeight) {
                        requestAnimationFrame(step);
                    } else {
                        dioxus.send(JSON.stringify({frames, ms, scrollTop: el.scrollTop}));
                    }
                }
                requestAnimationFrame(step);
            }, 4000);
            "#,
        );
        if let Ok(report) = eval.recv::<String>().await {
            println!("SCROLL_PROBE {report}");
        }
    });

    // Optional selection probe (BABEL_SEL=1): prints the editor's
    // selectionStart/End (UTF-16 units) whenever it changes, so scripted
    // arrow-key tests can measure caret movement across grapheme clusters.
    use_future(move || async move {
        if std::env::var_os("BABEL_SEL").is_none() {
            return;
        }
        let mut eval = document::eval(
            r#"
            const ta = document.querySelector('.editor');
            setInterval(() => {
                const r = ta.getBoundingClientRect();
                dioxus.send(JSON.stringify({
                    start: ta.selectionStart, end: ta.selectionEnd,
                    active: document.activeElement ? document.activeElement.tagName : 'none',
                    rect: [r.x|0, r.y|0, (r.width)|0, (r.height)|0],
                    win: [window.innerWidth, window.innerHeight],
                }));
            }, 400);
            "#,
        );
        loop {
            match eval.recv::<String>().await {
                Ok(sel) => println!("SEL {sel}"),
                Err(e) => {
                    println!("SEL_ERR {e:?}");
                    break;
                }
            }
        }
    });

    rsx! {
        style { {CSS} }
        div { class: "root",
            div { class: "bar",
                span { class: "title", "Babel (dioxus)" }
                button {
                    onclick: move |_| big.toggle(),
                    if big() { "Unload big doc" } else { "Load big doc (×1000)" }
                }
                span { class: "meta", "{line_count} lines · system fonts, WebKit fallback" }
            }
            div { class: "panes",
                // Rendering pane: read-only, scrollable, one paragraph per line.
                div { class: "pane render", pre { "{doc}" } }
                // Editing pane: real multiline editing surface (WebKit textarea):
                // mouse/Shift+arrow selection, caret movement, IME, copy/paste.
                textarea {
                    class: "pane editor",
                    spellcheck: "false",
                    value: "{editor}",
                    // BABEL_ECHO=1: echo editor content to stdout after each
                    // input so scripted keyboard tests can verify grapheme-
                    // cluster editing objectively.
                    oninput: move |e| {
                        let v = e.value();
                        if std::env::var_os("BABEL_ECHO").is_some() {
                            println!("EDIT chars={} value={:?}", v.chars().count(), v);
                        }
                        editor.set(v);
                    },
                }
            }
        }
    }
}

const CSS: &str = r#"
:root { color-scheme: light dark; }
* { box-sizing: border-box; }
body { margin: 0; font-family: system-ui, sans-serif; }
.root {
  display: flex; flex-direction: column; height: 100vh;
  padding: 10px; gap: 8px;
  background: #f2f2f6; color: #1d1d1f;
}
.bar { display: flex; align-items: center; gap: 10px; }
.title { font-weight: 600; }
.bar button { padding: 3px 10px; }
.meta { font-size: 11.5px; opacity: 0.65; margin-left: auto; }
.panes { flex: 1; display: flex; gap: 8px; min-height: 0; }
.pane {
  border: 1px solid #c8c8cc; border-radius: 6px;
  background: #ffffff; padding: 10px;
  font-size: 15px; line-height: 1.65;
}
.render { flex: 1.5; overflow-y: auto; min-width: 0; }
.render pre {
  margin: 0;
  font: inherit;               /* proportional system font, not monospace */
  white-space: pre-wrap;
  overflow-wrap: break-word;
}
.editor {
  flex: 1; resize: none; color: inherit;
  font: inherit; line-height: inherit;
}
@media (prefers-color-scheme: dark) {
  .root { background: #1e1e21; color: #e8e8ea; }
  .pane { background: #2a2a2e; border-color: #48484d; }
}
"#;
