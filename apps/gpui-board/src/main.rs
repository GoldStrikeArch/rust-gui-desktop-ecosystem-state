//! "Board" — kanban task board in gpui 0.2.2 (SPEC-3).
//!
//! Drag-and-drop uses gpui's native primitives (the same ones Zed uses for
//! tab/panel dragging):
//! - `on_drag(payload, ghost_ctor)` starts a typed drag and paints the ghost
//!   entity under the cursor every frame,
//! - `on_drag_move::<CardDrag>` fires on *every* registered listener for every
//!   mouse move while a CardDrag is active (capture phase, with the listener's
//!   own bounds) — each card zone does its own containment test and computes
//!   the insertion index from the cursor's top/bottom half,
//! - `on_drop::<CardDrag>` on each column body commits the move.
//!
//! There is no text-input widget in gpui; the inline add/edit inputs are the
//! same minimal hand-rolled `on_key_down` input as iteration 1 (gpui-app):
//! plain typing + backspace + enter/escape, no IME/selection/clipboard.
//!
//! Repaint strategy: event-driven `cx.notify()`; while a drag is active gpui
//! itself refreshes the window on every mouse move so the ghost can follow the
//! cursor. The oneshot drop-settle animation requests frames for ~250 ms.

use std::time::Duration;

use gpui::{
    Animation, AnimationExt as _, App, Application, Bounds, ClickEvent, Context, DragMoveEvent,
    FocusHandle, KeyDownEvent, SharedString, TitlebarOptions, Window, WindowBounds, WindowOptions,
    div, ease_in_out, prelude::*, px, rgb, rgba, size,
};

const ACCENT: u32 = 0x3b82f6;

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

struct Card {
    id: u64,
    text: SharedString,
}

/// Typed drag payload; gpui routes drag/drop events by this type.
#[derive(Clone)]
struct CardDrag {
    card_id: u64,
    text: SharedString,
}

/// Floating preview painted under the cursor while dragging a card.
struct CardGhost {
    text: SharedString,
}

impl Render for CardGhost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(244.))
            .p_2()
            .rounded_md()
            .bg(rgba(0xffffffee))
            .border_1()
            .border_color(rgb(ACCENT))
            .shadow_lg()
            .text_sm()
            .text_color(rgb(0x0f172a))
            .child(self.text.clone())
    }
}

struct BoardApp {
    columns: Vec<(&'static str, Vec<Card>)>,
    next_id: u64,
    /// (column, insertion index) the drag currently points at.
    drop_target: Option<(usize, usize)>,
    /// Column whose "+ Add card" input is open.
    adding: Option<usize>,
    /// Card being edited in place (double-click).
    editing: Option<u64>,
    /// Shared text buffer for the single active inline input.
    input: String,
    input_focus: FocusHandle,
    /// (card, epoch) that just landed from a drag — drives the settle animation.
    drop_anim: Option<(u64, u64)>,
    epoch: u64,
}

impl BoardApp {
    fn new(cx: &mut Context<Self>) -> Self {
        let mut next_id = 0;
        let mut card = |text: &str| {
            let c = Card { id: next_id, text: SharedString::from(text.to_string()) };
            next_id += 1;
            c
        };
        let columns = vec![
            ("Todo", vec![card("Ship Pulse dashboard"), card("Write FRICTION notes")]),
            ("Doing", vec![card("Build gpui kanban board")]),
            ("Done", vec![card("Read gpui 0.2.2 examples")]),
        ];
        Self {
            columns,
            next_id,
            drop_target: None,
            adding: None,
            editing: None,
            input: String::new(),
            input_focus: cx.focus_handle(),
            drop_anim: None,
            epoch: 0,
        }
    }

    fn locate(&self, id: u64) -> Option<(usize, usize)> {
        self.columns.iter().enumerate().find_map(|(c, (_, cards))| {
            cards.iter().position(|card| card.id == id).map(|i| (c, i))
        })
    }

    fn set_drop_target(&mut self, target: (usize, usize), cx: &mut Context<Self>) {
        if self.drop_target != Some(target) {
            self.drop_target = Some(target);
            cx.notify();
        }
    }

    /// Commit the active drag at `drop_target` (called from a column's on_drop).
    fn finish_drop(&mut self, drag: &CardDrag, cx: &mut Context<Self>) {
        let Some((to_col, mut ix)) = self.drop_target.take() else {
            return;
        };
        let Some((from_col, from_ix)) = self.locate(drag.card_id) else {
            return;
        };
        let card = self.columns[from_col].1.remove(from_ix);
        if from_col == to_col && from_ix < ix {
            ix -= 1;
        }
        let ix = ix.min(self.columns[to_col].1.len());
        self.columns[to_col].1.insert(ix, card);
        self.epoch += 1;
        self.drop_anim = Some((drag.card_id, self.epoch));
        cx.notify();
    }

    fn delete_card(&mut self, id: u64, cx: &mut Context<Self>) {
        for (_, cards) in &mut self.columns {
            cards.retain(|c| c.id != id);
        }
        if self.editing == Some(id) {
            self.editing = None;
            self.input.clear();
        }
        cx.notify();
    }

    fn start_add(&mut self, col: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.adding = Some(col);
        self.editing = None;
        self.input.clear();
        self.input_focus.focus(window);
        cx.notify();
    }

    fn start_edit(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((c, i)) = self.locate(id) {
            self.input = self.columns[c].1[i].text.to_string();
            self.editing = Some(id);
            self.adding = None;
            self.input_focus.focus(window);
            cx.notify();
        }
    }

    fn cancel_input(&mut self, cx: &mut Context<Self>) {
        self.adding = None;
        self.editing = None;
        self.input.clear();
        cx.notify();
    }

    fn commit_input(&mut self, cx: &mut Context<Self>) {
        let text = self.input.trim().to_string();
        if text.is_empty() {
            return; // empty input is ignored per spec
        }
        if let Some(col) = self.adding {
            let id = self.next_id;
            self.next_id += 1;
            self.columns[col].1.push(Card { id, text: text.into() });
            self.adding = None;
        } else if let Some(id) = self.editing {
            if let Some((c, i)) = self.locate(id) {
                self.columns[c].1[i].text = text.into();
            }
            self.editing = None;
        }
        self.input.clear();
        cx.notify();
    }

    /// Minimal hand-rolled text editing (same approach as iteration 1):
    /// gpui ships no text-input widget; a production one is ~750 lines via
    /// `EntityInputHandler` (see the bundled input.rs example).
    fn on_input_key(&mut self, ev: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let ks = &ev.keystroke;
        if ks.modifiers.platform || ks.modifiers.control || ks.modifiers.function {
            return;
        }
        match ks.key.as_str() {
            "enter" => self.commit_input(cx),
            "escape" => self.cancel_input(cx),
            "backspace" => {
                self.input.pop();
                cx.notify();
            }
            _ => {
                if let Some(c) = &ks.key_char {
                    self.input.push_str(c);
                    cx.notify();
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

impl BoardApp {
    /// The shared inline input, styled like a card (used for add + edit).
    fn render_input(&self, id: (&'static str, u64), placeholder: &'static str,
        cx: &mut Context<Self>) -> gpui::AnyElement {
        div()
            .id(id)
            .track_focus(&self.input_focus)
            .on_key_down(cx.listener(Self::on_input_key))
            .flex()
            .items_center()
            .p_2()
            .min_h(px(34.))
            .rounded_md()
            .bg(gpui::white())
            .border_2()
            .border_color(rgb(ACCENT))
            .cursor_text()
            .text_sm()
            .child(if self.input.is_empty() {
                div().text_color(rgb(0x94a3b8)).child(placeholder)
            } else {
                div().text_color(rgb(0x0f172a)).child(self.input.clone())
            })
            .child(div().w(px(1.5)).h(px(14.)).bg(rgb(0x0f172a))) // caret
            .into_any_element()
    }

    fn render_card(&self, col_ix: usize, ix: usize, is_last: bool, dragging: bool,
        cx: &mut Context<Self>) -> gpui::AnyElement {
        let card = &self.columns[col_ix].1[ix];
        let id = card.id;
        let len = self.columns[col_ix].1.len();

        let line_top = dragging && self.drop_target == Some((col_ix, ix));
        let line_bottom = dragging && is_last && self.drop_target == Some((col_ix, len));
        let indicator = |top: bool| {
            div()
                .absolute()
                .left(px(2.))
                .right(px(2.))
                .h(px(3.))
                .rounded_full()
                .bg(rgb(ACCENT))
                .when(top, |d| d.top(px(-1.5)))
                .when(!top, |d| d.bottom(px(-1.5)))
        };

        let body: gpui::AnyElement = if self.editing == Some(id) {
            self.render_input(("edit", id), "Card text…", cx)
        } else {
            let drag_text = card.text.clone();
            let inner = div()
                .id(("card", id))
                .group("card")
                .flex()
                .items_start()
                .gap_1()
                .p_2()
                .rounded_md()
                .bg(gpui::white())
                .border_1()
                .border_color(rgb(0xcbd5e1))
                .shadow_sm()
                .cursor_grab()
                .hover(|s| s.border_color(rgb(0x94a3b8)))
                // Drag ghost: gpui paints this entity at the cursor each frame,
                // anchored at the grab offset inside the original card.
                .on_drag(CardDrag { card_id: id, text: drag_text }, |drag, _grab, _, cx| {
                    let text = drag.text.clone();
                    cx.new(|_| CardGhost { text })
                })
                // Double-click to edit in place.
                .on_click(cx.listener(move |this, ev: &ClickEvent, window, cx| {
                    if ev.click_count() == 2 {
                        this.start_edit(id, window, cx);
                    }
                }))
                .child(div().flex_1().text_sm().text_color(rgb(0x0f172a)).child(card.text.clone()))
                .child(
                    div()
                        .id(("del", id))
                        .flex_none()
                        .px_1()
                        .rounded_sm()
                        .text_sm()
                        .text_color(rgb(0x94a3b8))
                        .hover(|s| s.text_color(rgb(0xdc2626)).bg(rgb(0xfee2e2)))
                        .cursor_pointer()
                        .child("✕")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.delete_card(id, cx);
                        })),
                );

            // Drop-settle animation: fade the just-landed card back in.
            // A changed element id ("landed", epoch) restarts the oneshot.
            if self.drop_anim == Some((id, self.epoch)) {
                inner
                    .with_animation(
                        ("landed", self.epoch as usize),
                        Animation::new(Duration::from_millis(260)).with_easing(ease_in_out),
                        |el, delta| el.opacity(0.25 + 0.75 * delta),
                    )
                    .into_any_element()
            } else {
                inner.into_any_element()
            }
        };

        // Wrapper = contiguous drop zone: its vertical half decides whether the
        // insertion point is above (ix) or below (ix + 1) this card.
        div()
            .relative()
            .py_1()
            .on_drag_move::<CardDrag>(cx.listener(move |this, ev: &DragMoveEvent<CardDrag>, _, cx| {
                let pos = ev.event.position;
                if ev.bounds.contains(&pos) {
                    let ins = if pos.y < ev.bounds.center().y { ix } else { ix + 1 };
                    this.set_drop_target((col_ix, ins), cx);
                }
            }))
            .when(line_top, |d| d.child(indicator(true)))
            .when(line_bottom, |d| d.child(indicator(false)))
            .child(body)
            .into_any_element()
    }

    fn render_column(&self, col_ix: usize, dragging: bool, cx: &mut Context<Self>)
        -> gpui::AnyElement {
        let (name, cards) = &self.columns[col_ix];
        let name = *name;
        let count = cards.len();

        let mut card_els: Vec<gpui::AnyElement> = Vec::new();
        for ix in 0..count {
            card_els.push(self.render_card(col_ix, ix, ix + 1 == count, dragging, cx));
        }

        let add_el: gpui::AnyElement = if self.adding == Some(col_ix) {
            self.render_input(("add", col_ix as u64), "New card…", cx)
        } else {
            div()
                .id(("add-btn", col_ix))
                .p_2()
                .rounded_md()
                .text_sm()
                .text_color(rgb(0x64748b))
                .hover(|s| s.bg(rgb(0xcbd5e1)).text_color(rgb(0x0f172a)))
                .cursor_pointer()
                .child("+ Add card")
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.start_add(col_ix, window, cx);
                }))
                .into_any_element()
        };

        div()
            .flex_1()
            .flex()
            .flex_col()
            .rounded_lg()
            .bg(rgb(0xe2e8f0))
            .p_2()
            .gap_1()
            .child(
                div()
                    .flex_none()
                    .flex()
                    .justify_between()
                    .items_center()
                    .px_1()
                    .pb_1()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(rgb(0x334155))
                            .child(name),
                    )
                    .child(
                        div()
                            .px_2()
                            .rounded_full()
                            .bg(rgb(0xcbd5e1))
                            .text_xs()
                            .text_color(rgb(0x334155))
                            .child(format!("{count}")),
                    ),
            )
            .child(
                // Independently scrollable card list; also the drop target for
                // the whole column (empty space maps to "insert at end").
                div()
                    .id(("col-body", col_ix))
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .on_drag_move::<CardDrag>(cx.listener(
                        move |this, ev: &DragMoveEvent<CardDrag>, _, cx| {
                            // Parent zones run before child zones in the capture
                            // phase, so per-card zones overwrite this fallback.
                            if ev.bounds.contains(&ev.event.position) {
                                let end = this.columns[col_ix].1.len();
                                this.set_drop_target((col_ix, end), cx);
                            }
                        },
                    ))
                    .on_drop::<CardDrag>(cx.listener(move |this, d: &CardDrag, _, cx| {
                        this.finish_drop(d, cx);
                    }))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .children(card_els)
                            .when(count == 0, |d| {
                                let hot = dragging && self.drop_target
                                    .is_some_and(|(c, _)| c == col_ix);
                                d.child(
                                    div()
                                        .m_1()
                                        .h(px(48.))
                                        .rounded_md()
                                        .border_2()
                                        .border_dashed()
                                        .border_color(if hot { rgb(ACCENT) } else { rgb(0xcbd5e1) })
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .text_xs()
                                        .text_color(rgb(0x94a3b8))
                                        .child("Drop cards here"),
                                )
                            }),
                    ),
            )
            .child(div().flex_none().child(add_el))
            .into_any_element()
    }
}

impl Render for BoardApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dragging = cx.has_active_drag();

        let mut columns: Vec<gpui::AnyElement> = Vec::new();
        for col_ix in 0..self.columns.len() {
            columns.push(self.render_column(col_ix, dragging, cx));
        }

        div()
            .size_full()
            .flex()
            .gap_3()
            .p_3()
            .bg(rgb(0xf1f5f9))
            .children(columns)
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(900.), px(600.)), cx);

        // gpui apps do not quit when the last window closes unless told to.
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        cx.open_window(
            WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some(SharedString::from("Board (gpui)")),
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(BoardApp::new),
        )
        .unwrap();
        cx.activate(true);
    });
}
