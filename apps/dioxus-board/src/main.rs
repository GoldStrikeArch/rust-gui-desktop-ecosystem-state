//! "Board" — kanban task board. Dioxus 0.7 desktop (wry/tao webview).
//!
//! Drag & drop is hand-rolled on bubbling mouse events with Rust-side drag
//! state in a Signal (no HTML5 draggable):
//!   * card onmousedown arms a potential drag; a 6px movement threshold on the
//!     root's onmousemove turns it into a real drag (so click/double-click on
//!     cards still work);
//!   * while dragging, every card grows two absolutely-positioned invisible
//!     "half" overlays (top/bottom, extended 5px past the card to cover the
//!     8px gaps); mousemove over a half sets the insertion target to
//!     (column, index) or (column, index+1) — this sidesteps the fact that a
//!     Dioxus MouseEvent carries offset coordinates but NOT the target
//!     element's size, so "above/below the midpoint?" cannot be answered from
//!     the event alone;
//!   * a flex-grow "endzone" at the bottom of each column catches drops into
//!     empty space / empty columns;
//!   * root onmouseup commits (remove + index-fixup + insert), onmouseleave
//!     cancels. The ghost is a fixed-position div fed cursor client
//!     coordinates from the root's onmousemove; the drop indicator is a real
//!     element inserted into the flow at the target index.
//!
//! Drop animation: the moved card's `bump` counter is part of its rsx key, so
//! the drop recreates the element and its CSS `settle` keyframe replays.
//! Repaint is purely signal-write -> VDOM diff -> DOM patch; there is no
//! frame loop, and mousemoves only write the drag signal when something
//! (ghost position / target) actually changed.

use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
use dioxus::prelude::*;

fn main() {
    dioxus::LaunchBuilder::desktop()
        .with_cfg(
            Config::new().with_window(
                WindowBuilder::new()
                    .with_title("Board (dioxus)")
                    .with_inner_size(LogicalSize::new(900.0, 600.0))
                    .with_resizable(true),
            ),
        )
        .launch(App);
}

#[derive(Clone, PartialEq)]
struct CardData {
    id: u64,
    text: String,
    bump: u32, // incremented on every drop; part of the rsx key -> replays the settle animation
}

struct ColumnData {
    name: &'static str,
    cards: Vec<CardData>,
}

#[derive(Clone, PartialEq)]
struct Drag {
    id: u64,
    from: (usize, usize),
    text: String, // snapshot for the ghost
    started: bool,
    x: f64,
    y: f64,
    ox: f64,
    oy: f64,
    target: Option<(usize, usize)>, // (column, insertion index 0..=len)
}

fn card(id: u64, text: &str) -> CardData {
    CardData { id, text: text.to_string(), bump: 0 }
}

#[component]
fn App() -> Element {
    let mut columns = use_signal(|| {
        vec![
            ColumnData {
                name: "Todo",
                cards: vec![card(1, "Write FRICTION.md notes"), card(2, "Measure repaint cost at 60 Hz")],
            },
            ColumnData { name: "Doing", cards: vec![card(3, "Hand-roll mouse-event drag & drop")] },
            ColumnData { name: "Done", cards: vec![card(4, "Crib project setup from iteration 1")] },
        ]
    });
    let mut next_id = use_signal(|| 5u64);
    let mut adding = use_signal(|| Option::<usize>::None);
    let mut add_text = use_signal(String::new);
    let mut editing = use_signal(|| Option::<u64>::None);
    let mut edit_text = use_signal(String::new);
    let mut drag = use_signal(|| Option::<Drag>::None);

    // Sets the insertion target, writing the signal only when it changes
    // (a bare .write() per mousemove would re-render the whole tree).
    let mut set_target = move |t: (usize, usize)| {
        let needs = matches!(&*drag.peek(), Some(d) if d.started && d.target != Some(t));
        if needs {
            drag.write().as_mut().unwrap().target = Some(t);
        }
    };

    let mut commit_edit = move |cid: u64| {
        let t = edit_text.peek().trim().to_string();
        if !t.is_empty() {
            let mut cols = columns.write();
            'outer: for col in cols.iter_mut() {
                for c in col.cards.iter_mut() {
                    if c.id == cid {
                        c.text = t;
                        break 'outer;
                    }
                }
            }
        }
        editing.set(None);
    };

    // ---- render-time view state ------------------------------------------
    let drag_v = drag.read().clone();
    let dragging = drag_v.as_ref().map_or(false, |d| d.started);
    let target = drag_v.as_ref().and_then(|d| if d.started { d.target } else { None });
    let src_id = drag_v.as_ref().filter(|d| d.started).map(|d| d.id);
    let editing_v = *editing.read();
    let adding_v = *adding.read();
    let ghost = drag_v
        .as_ref()
        .filter(|d| d.started)
        .map(|d| (d.x + 12.0, d.y + 8.0, d.text.clone()));

    rsx! {
        style { {CSS} }
        div {
            class: if dragging { "root dragging" } else { "root" },
            onmousemove: move |evt| {
                if drag.peek().is_some() {
                    let p = evt.client_coordinates();
                    let mut dw = drag.write();
                    let d = dw.as_mut().unwrap();
                    d.x = p.x;
                    d.y = p.y;
                    if !d.started && (p.x - d.ox).hypot(p.y - d.oy) > 6.0 {
                        d.started = true;
                    }
                }
            },
            onmouseup: move |_| {
                let d = drag.peek().clone();
                if d.is_some() {
                    drag.set(None);
                }
                if let Some(d) = d {
                    if !d.started {
                        return;
                    }
                    if let Some((tc, mut ti)) = d.target {
                        let (fc, fi) = d.from;
                        let mut cols = columns.write();
                        // Sanity: the card must still be where the drag began.
                        if fi < cols[fc].cards.len() && cols[fc].cards[fi].id == d.id {
                            let mut moved = cols[fc].cards.remove(fi);
                            if tc == fc && ti > fi {
                                ti -= 1;
                            }
                            moved.bump += 1;
                            let ti = ti.min(cols[tc].cards.len());
                            cols[tc].cards.insert(ti, moved);
                        }
                    }
                }
            },
            onmouseleave: move |_| {
                if drag.peek().is_some() {
                    drag.set(None); // cursor left the window: cancel
                }
            },

            div { class: "toprow",
                h1 { "Board" }
                span { class: "hint",
                    "drag cards between columns \u{00b7} double-click to edit \u{00b7} Enter commits, Esc cancels"
                }
            }

            div { class: "board",
                for (ci, col) in columns.read().iter().enumerate() {
                    div { key: "{col.name}", class: "column",
                        div { class: "colhead",
                            span { class: "colname", "{col.name}" }
                            span { class: "colcount", "{col.cards.len()}" }
                        }
                        div { class: "colbody",
                            // Drop indicator before the first card. The other
                            // indicator positions render *after* card i (as
                            // target i+1) so the keyed card div stays the
                            // first node of the for-body — rsx only honors
                            // keys on the first node in a block.
                            if target == Some((ci, 0)) {
                                div { class: "dropline" }
                            }
                            for (i, c) in col.cards.iter().enumerate() {
                                div {
                                    key: "{c.id}-{c.bump}",
                                    class: if src_id == Some(c.id) { "card dragsrc" } else if c.bump > 0 { "card dropped" } else { "card" },
                                    onmousedown: {
                                        let cid = c.id;
                                        let txt = c.text.clone();
                                        move |evt: MouseEvent| {
                                            if editing.peek().is_some() {
                                                return;
                                            }
                                            let p = evt.client_coordinates();
                                            drag.set(Some(Drag {
                                                id: cid,
                                                from: (ci, i),
                                                text: txt.clone(),
                                                started: false,
                                                x: p.x,
                                                y: p.y,
                                                ox: p.x,
                                                oy: p.y,
                                                target: None,
                                            }));
                                        }
                                    },
                                    ondoubleclick: {
                                        let cid = c.id;
                                        let txt = c.text.clone();
                                        move |_| {
                                            edit_text.set(txt.clone());
                                            editing.set(Some(cid));
                                        }
                                    },
                                    if editing_v == Some(c.id) {
                                        input {
                                            class: "editinput",
                                            value: "{edit_text}",
                                            oninput: move |evt| edit_text.set(evt.value()),
                                            onmousedown: |evt| evt.stop_propagation(),
                                            onkeydown: {
                                                let cid = c.id;
                                                move |evt: KeyboardEvent| {
                                                    if evt.key() == Key::Enter {
                                                        commit_edit(cid);
                                                    } else if evt.key() == Key::Escape {
                                                        editing.set(None);
                                                    }
                                                }
                                            },
                                            onblur: move |_| editing.set(None),
                                            onmounted: move |evt| async move {
                                                let _ = evt.set_focus(true).await;
                                            },
                                        }
                                    } else {
                                        div { class: "cardrow",
                                            span { class: "cardtext", "{c.text}" }
                                            button {
                                                class: "xbtn",
                                                onmousedown: |evt| evt.stop_propagation(),
                                                onclick: move |_| {
                                                    columns.write()[ci].cards.remove(i);
                                                },
                                                "\u{2715}"
                                            }
                                        }
                                    }
                                    // Invisible drop-target halves, only while a
                                    // drag is live (they sit above the card body).
                                    if dragging && src_id != Some(c.id) {
                                        div { class: "half top", onmousemove: move |_| set_target((ci, i)) }
                                        div { class: "half bot", onmousemove: move |_| set_target((ci, i + 1)) }
                                    }
                                }
                                if target == Some((ci, i + 1)) {
                                    div { class: "dropline" }
                                }
                            }
                            // Fills the remaining column space; dropping here
                            // appends to the column (also makes empty columns
                            // valid drop targets).
                            div {
                                class: "endzone",
                                onmousemove: move |_| {
                                    let len = columns.peek()[ci].cards.len();
                                    set_target((ci, len));
                                },
                            }
                        }
                        if adding_v == Some(ci) {
                            input {
                                class: "addinput",
                                value: "{add_text}",
                                placeholder: "Card text\u{2026}",
                                oninput: move |evt| add_text.set(evt.value()),
                                onkeydown: move |evt: KeyboardEvent| {
                                    if evt.key() == Key::Enter {
                                        let t = add_text.peek().trim().to_string();
                                        if !t.is_empty() {
                                            let id = *next_id.peek();
                                            next_id += 1;
                                            columns.write()[ci].cards.push(CardData { id, text: t, bump: 0 });
                                            add_text.set(String::new());
                                            adding.set(None);
                                        }
                                    } else if evt.key() == Key::Escape {
                                        adding.set(None);
                                    }
                                },
                                onmounted: move |evt| async move {
                                    let _ = evt.set_focus(true).await;
                                },
                            }
                        } else {
                            button {
                                class: "addbtn",
                                onclick: move |_| {
                                    add_text.set(String::new());
                                    adding.set(Some(ci));
                                },
                                "+ Add card"
                            }
                        }
                    }
                }
            }

            if let Some((gx, gy, gt)) = ghost {
                div { class: "ghost", style: "left: {gx}px; top: {gy}px;", "{gt}" }
            }
        }
    }
}

const CSS: &str = r#"
* { margin: 0; padding: 0; box-sizing: border-box; }
html, body { height: 100%; }
body {
  font-family: system-ui, -apple-system, sans-serif; overflow: hidden;
  background: #eef1f5; color: #1c2733;
  -webkit-user-select: none; user-select: none;
}
.root { height: 100vh; display: flex; flex-direction: column; padding: 14px; gap: 10px; }
.dragging, .dragging .card { cursor: grabbing !important; }
.toprow { display: flex; align-items: baseline; gap: 12px; }
h1 { font-size: 17px; }
.hint { font-size: 11px; color: #7b8a9e; }
.board { flex: 1; display: flex; gap: 12px; min-height: 0; }
.column {
  flex: 1; min-width: 0; min-height: 0; display: flex; flex-direction: column;
  background: #e2e6ec; border-radius: 10px; padding: 10px;
}
.colhead { display: flex; justify-content: space-between; align-items: center; margin-bottom: 8px; }
.colname { font-weight: 600; font-size: 14px; }
.colcount {
  background: #c9d1dd; border-radius: 10px; padding: 1px 9px;
  font-size: 12px; font-variant-numeric: tabular-nums;
}
/* Each column body scrolls independently. */
.colbody {
  flex: 1; min-height: 0; overflow-y: auto;
  display: flex; flex-direction: column; gap: 8px; padding: 2px;
}
.endzone { flex: 1; min-height: 36px; }
.card {
  position: relative; background: #fff; border-radius: 8px; padding: 9px 10px;
  box-shadow: 0 1px 2px rgba(16,24,40,.12); cursor: grab;
  transition: box-shadow .15s ease, transform .15s ease, opacity .15s ease;
}
.card:hover { box-shadow: 0 3px 8px rgba(16,24,40,.22); }
.card.dragsrc { opacity: .35; }
.card.dropped { animation: settle .35s ease; }
@keyframes settle {
  0%   { transform: scale(1.05); box-shadow: 0 10px 22px rgba(16,24,40,.3); }
  100% { transform: scale(1); }
}
.cardrow { display: flex; align-items: flex-start; gap: 8px; }
.cardtext { flex: 1; font-size: 13.5px; line-height: 1.35; word-break: break-word; }
.xbtn {
  border: none; background: transparent; color: #8a97a8; cursor: pointer;
  font-size: 12px; border-radius: 4px; padding: 1px 5px;
}
.xbtn:hover { background: #f6d9d9; color: #b42318; }
.half { position: absolute; left: 0; right: 0; z-index: 5; }
.half.top { top: -5px; height: calc(50% + 5px); }
.half.bot { bottom: -5px; height: calc(50% + 5px); }
.dropline {
  height: 4px; border-radius: 2px; background: #4c8dff; margin: 0 2px;
  animation: pop .12s ease;
}
@keyframes pop { from { transform: scaleX(.6); opacity: .4; } }
.ghost {
  position: fixed; z-index: 99; pointer-events: none; max-width: 250px;
  background: #fff; border-radius: 8px; padding: 9px 10px; font-size: 13.5px;
  box-shadow: 0 12px 26px rgba(16,24,40,.35); transform: rotate(2deg); opacity: .95;
}
.addbtn {
  border: none; background: transparent; color: #5b6b7f; text-align: left;
  padding: 7px 8px; margin-top: 8px; border-radius: 8px; cursor: pointer; font-size: 13px;
}
.addbtn:hover { background: #d4dae3; color: #1c2733; }
.addinput, .editinput {
  width: 100%; font: inherit; outline: none;
  -webkit-user-select: text; user-select: text;
}
.addinput {
  margin-top: 8px; padding: 7px 8px; font-size: 13.5px;
  border: 2px solid #4c8dff; border-radius: 8px; background: #fff;
}
.editinput {
  font-size: 13.5px; line-height: 1.35; padding: 1px 3px;
  border: 1px solid #4c8dff; border-radius: 4px;
}
"#;
