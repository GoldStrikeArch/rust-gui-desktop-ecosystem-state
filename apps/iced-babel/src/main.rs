//! "Babel" — text & i18n stress test (SPEC-5), iced 0.14.
//!
//! Text-stack notes (research-relevant):
//! - iced's text pipeline is cosmic-text: shaping + BiDi + system font
//!   discovery/fallback via fontdb. No fonts are bundled here (the default
//!   `fira-sans` feature is OFF in 0.14 defaults); everything on screen is
//!   resolved from macOS system fonts.
//! - Default `Shaping::Auto` upgrades any non-ASCII run to advanced shaping;
//!   we set `Shaping::Advanced` explicitly on the rendering pane to be sure.
//! - `text_editor` always shapes with `Shaping::Advanced` and has full IME
//!   plumbing (preedit + candidate-window positioning) in 0.14.
//! - Verification hooks (env vars, stderr output):
//!   BABEL_SHOT=path      save a window screenshot after 3 s (window::screenshot)
//!   BABEL_SELFTEST=1     run grapheme-caret/selection probes on
//!                        text_editor::Content and print the results
//!   BABEL_SCROLLTEST=1   auto-load the big doc, then drive the scrollable
//!                        one step per frame for 5 s and print the achieved
//!                        frame rate (objective jank measure)

use std::sync::Arc;
use std::time::{Duration, Instant};

use iced::widget::{
    button, column, container, row, scrollable, text, text_editor,
};
use iced::window;
use iced::{Element, Fill, FillPortion, Size, Subscription, Task};

const CORPUS: &str = include_str!("../../babel-assets/corpus.txt");
const BIG_REPEATS: usize = 1_000;
const SCROLL_ID: &str = "rendering-pane";

pub fn main() -> iced::Result {
    // The screenshot run opens a taller window so the full corpus pane fits
    // unwrapped (SPEC-5 allows resizing for the capture).
    let size = if std::env::var("BABEL_SHOT").is_ok() {
        Size::new(1150.0, 780.0)
    } else {
        Size::new(800.0, 600.0)
    };

    iced::application(Babel::boot, Babel::update, Babel::view)
        .title(|_: &Babel| String::from("Babel (iced)"))
        .window_size(size)
        .subscription(Babel::subscription)
        .run()
}

struct Babel {
    /// Rendering-pane lines. Either the 11 corpus paragraphs or the corpus
    /// repeated 1,000× (≈11k lines), all owned Strings.
    lines: Vec<String>,
    big: bool,
    editor: text_editor::Content,
    status: String,
    /// Set when "Load big doc" is pressed; cleared on the next frame to
    /// measure time-to-first-frame (layout + shaping of the new content).
    load_started: Option<Instant>,
    scroll_test: Option<ScrollTest>,
}

struct ScrollTest {
    started: Instant,
    frames: u32,
}

#[derive(Debug, Clone)]
enum Message {
    EditorAction(text_editor::Action),
    LoadBig,
    Reset,
    Frame,
    Shoot,
    ShotTaken(window::Screenshot),
}

fn corpus_lines() -> Vec<String> {
    CORPUS.lines().map(str::to_owned).collect()
}

fn mixed_line() -> &'static str {
    CORPUS
        .lines()
        .find(|line| line.starts_with("[MIXED]"))
        .expect("corpus has a [MIXED] line")
}

impl Babel {
    fn boot() -> (Self, Task<Message>) {
        let state = Self {
            lines: corpus_lines(),
            big: false,
            editor: text_editor::Content::with_text(mixed_line()),
            status: String::from("11 corpus lines — system fonts, no bundling"),
            load_started: None,
            scroll_test: None,
        };

        if std::env::var("BABEL_SELFTEST").is_ok() {
            selftest();
        }

        let mut tasks = Vec::new();

        if std::env::var("BABEL_SHOT").is_ok() {
            tasks.push(
                Task::future(async {
                    smol::Timer::after(Duration::from_secs(3)).await;
                })
                .then(|()| Task::done(Message::Shoot)),
            );
        }

        if std::env::var("BABEL_SCROLLTEST").is_ok() {
            tasks.push(Task::done(Message::LoadBig));
        }

        (state, Task::batch(tasks))
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::EditorAction(action) => {
                let trace = std::env::var("BABEL_TRACE").is_ok();
                if trace {
                    eprintln!("editor-action: {action:?}");
                }
                self.editor.perform(action);
                if trace {
                    eprintln!("  selection now: {:?}", self.editor.selection());
                }
                Task::none()
            }
            Message::LoadBig => {
                let corpus = corpus_lines();
                self.lines = std::iter::repeat_with(|| corpus.clone())
                    .take(BIG_REPEATS)
                    .flatten()
                    .collect();
                self.big = true;
                self.load_started = Some(Instant::now());
                self.status = format!("big doc: {} lines…", self.lines.len());
                eprintln!("load-big: {} lines", self.lines.len());
                Task::none()
            }
            Message::Reset => {
                self.lines = corpus_lines();
                self.big = false;
                self.scroll_test = None;
                self.status = String::from("11 corpus lines");
                Task::none()
            }
            Message::Frame => {
                // First frame after the big-doc swap → report layout cost.
                if let Some(started) = self.load_started.take() {
                    let elapsed = started.elapsed();
                    self.status = format!(
                        "big doc: {} lines — first frame in {elapsed:.2?}",
                        self.lines.len()
                    );
                    eprintln!("first-frame-after-load: {elapsed:.2?}");

                    if std::env::var("BABEL_SCROLLTEST").is_ok() {
                        self.scroll_test = Some(ScrollTest {
                            started: Instant::now(),
                            frames: 0,
                        });
                    }
                    return Task::none();
                }

                if let Some(test) = &mut self.scroll_test {
                    test.frames += 1;
                    let elapsed = test.started.elapsed();

                    if elapsed >= Duration::from_secs(5) {
                        let fps = f64::from(test.frames) / elapsed.as_secs_f64();
                        self.status = format!(
                            "scroll test: {:.1} fps over {:.1?} ({} lines)",
                            fps,
                            elapsed,
                            self.lines.len()
                        );
                        eprintln!(
                            "scroll-test: {:.1} fps ({} frames / {:.2?})",
                            fps, test.frames, elapsed
                        );
                        self.scroll_test = None;
                        Task::none()
                    } else {
                        iced::widget::operation::scroll_by(
                            SCROLL_ID,
                            scrollable::AbsoluteOffset { x: 0.0, y: 120.0 },
                        )
                    }
                } else {
                    Task::none()
                }
            }
            Message::Shoot => window::latest().then(|id| match id {
                Some(id) => window::screenshot(id).map(Message::ShotTaken),
                None => Task::none(),
            }),
            Message::ShotTaken(shot) => {
                let path = std::env::var("BABEL_SHOT")
                    .unwrap_or_else(|_| String::from("screenshot.png"));
                match save_png(&shot, &path) {
                    Ok(()) => eprintln!("screenshot: saved {path}"),
                    Err(error) => eprintln!("screenshot: FAILED: {error}"),
                }
                Task::none()
            }
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        // Frame ticks only while measuring (post-load timing or scroll test).
        if self.load_started.is_some() || self.scroll_test.is_some() {
            window::frames().map(|_| Message::Frame)
        } else {
            Subscription::none()
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let controls = row![
            button(if self.big { "Big doc loaded" } else { "Load big doc (×1000)" })
                .on_press(Message::LoadBig),
            button("Reset").on_press(Message::Reset),
            text(&self.status).size(13),
        ]
        .spacing(10)
        .align_y(iced::Center);

        let rendering = container(
            scrollable(
                column(self.lines.iter().map(|line| {
                    text(line)
                        .size(15)
                        .shaping(text::Shaping::Advanced)
                        .into()
                }))
                .spacing(6)
                .padding(10),
            )
            .id(SCROLL_ID)
            .width(Fill)
            .height(Fill),
        )
        .style(container::bordered_box)
        .width(FillPortion(3));

        let editor_pane = column![
            text("Editor — seeded with [MIXED]; selection / caret / IME here")
                .size(12),
            text_editor(&self.editor)
                .on_action(Message::EditorAction)
                .size(15)
                .height(Fill),
        ]
        .spacing(6)
        .width(FillPortion(2));

        column![
            controls,
            row![rendering, editor_pane].spacing(12).height(Fill),
        ]
        .spacing(10)
        .padding(12)
        .into()
    }
}

/// Grapheme / BiDi probes on `text_editor::Content` — the same editor core
/// (cosmic-text) the widget drives interactively, so motions/edits here are
/// faithful to arrow keys / backspace in the UI.
fn selftest() {
    type Content = text_editor::Content<iced::Renderer>;
    use text_editor::{Action, Edit, Motion};

    // --- ZWJ family emoji: caret motion + backspace --------------------
    let mut content = Content::with_text("a👨‍👩‍👧‍👦b");
    content.perform(Action::Move(Motion::DocumentStart));
    let mut cols = vec![content.cursor().position.column];
    for _ in 0..3 {
        content.perform(Action::Move(Motion::Right));
        cols.push(content.cursor().position.column);
    }
    eprintln!("selftest caret columns over a|family|b: {cols:?}");

    content.perform(Action::Move(Motion::DocumentEnd));
    content.perform(Action::Edit(Edit::Backspace)); // deletes 'b'
    content.perform(Action::Edit(Edit::Backspace)); // should delete WHOLE family
    let after = content.text();
    eprintln!(
        "selftest backspace over family: {:?} (len {}) -> {}",
        after,
        after.trim_end().chars().count(),
        if after.trim_end() == "a" { "CLEAN (whole grapheme)" } else { "CORRUPTED/partial" }
    );

    // --- Shift+Right selection across the BiDi boundary ----------------
    let mut content = Content::with_text(mixed_line());
    content.perform(Action::Move(Motion::DocumentStart));
    for _ in 0..16 {
        content.perform(Action::Select(Motion::Right));
    }
    eprintln!("selftest select 16x right: {:?}", content.selection());
    for _ in 0..6 {
        content.perform(Action::Select(Motion::Right));
    }
    eprintln!("selftest select 22x right (into Arabic): {:?}", content.selection());

    // --- Copy/paste round-trip at the Content level ---------------------
    let mut content = Content::with_text("start ");
    content.perform(Action::Move(Motion::DocumentEnd));
    content.perform(Action::Edit(Edit::Paste(Arc::new(String::from("שלום 世界 👨‍👩‍👧‍👦")))));
    eprintln!("selftest paste round-trip: {:?}", content.text().trim_end());
}

/// Encode an iced window screenshot (RGBA8, physical pixels) as PNG.
fn save_png(shot: &window::Screenshot, path: &str) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut encoder = png::Encoder::new(
        std::io::BufWriter::new(file),
        shot.size.width,
        shot.size.height,
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer
        .write_image_data(&shot.rgba)
        .map_err(|e| e.to_string())?;
    Ok(())
}
