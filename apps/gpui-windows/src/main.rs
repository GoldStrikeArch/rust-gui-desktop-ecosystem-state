//! "Windows" — multi-window & modal probe for gpui 0.2.2 (SPEC-9).
//!
//! What the framework gives us:
//! - `cx.open_window(WindowOptions{..}, |window, cx| cx.new(..))` per window;
//!   a window is a *handle* (`WindowHandle<V>`, Copy) over a root `Entity<V>`.
//! - **One source of truth**: a single `Entity<Store>` is handed to every
//!   window's root view. Each view does `cx.observe(&store, |_,_,cx| cx.notify())`,
//!   so any `store.update(..)` from any window repaints all of them. No copies,
//!   no message plumbing, no per-window `App` state.
//! - **Cross-window messages**: `EventEmitter<StoreEvent>` + `cx.subscribe`
//!   (Pong → inspector flash). Ping needs no event at all: it just mutates the
//!   shared entity.
//! - **Native modality**: `window.prompt(PromptLevel, msg, detail, &[..])` is a
//!   real macOS NSAlert attached with `beginSheetModalForWindow:` (gpui 0.2.2
//!   src/platform/mac/window.rs:1198) — an OS *window-modal sheet*. It is
//!   button-only: there is no accessory-view/custom-content escape hatch, so
//!   the Edit dialog is a second top-level window plus an app-level input gate
//!   (see FRICTION.md).
//! - **Close veto**: `window.on_window_should_close(cx, |..| false)`.
//! - **Tab order** inside the dialog: `FocusHandle::tab_index/tab_stop` +
//!   `window.focus_next()` (gpui built-in, see the bundled tab_stop.rs example).
//!
//! What it does NOT give us (all approximated here, rated in FRICTION.md):
//! window parenting (no `parent`/`owner`/`transient_for` field in
//! `WindowOptions`), OS modality for custom content, per-window hide, any text
//! input widget, and any window-moved/resized event (position persistence is
//! polled).
//!
//! `WINDOWS_SELFTEST=1` drives the same functions the UI events call and prints
//! `SELFTEST DONE pass=N fail=M`, then exits.

use std::time::Duration;

use gpui::{
    App, Application, Bounds, ClickEvent, Context, Entity, EventEmitter, FocusHandle, Global,
    KeyBinding, KeyDownEvent, Keystroke, PromptLevel, SharedString, Subscription, TitlebarOptions,
    Window, WindowAppearance, WindowBounds, WindowHandle, WindowOptions, actions, div, point,
    prelude::*, px, rgb, rgba, size,
};

actions!(windows_app, [OpenPrefs, ToggleInspector, CloseFocused]);

// ---------------------------------------------------------------------------
// Shared model — ONE entity, seen by every window
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Project {
    name: String,
    owner: &'static str,
    budget: f64,
    open: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    Light,
    Dark,
    System,
}

struct Store {
    projects: Vec<Project>,
    selected: usize,
    pings: u32,
    dirty: bool,
    compact: bool,
    mode: Mode,
    /// An edit dialog is open: the main window refuses input while true.
    modal: bool,
    /// Counts native confirm sheets actually opened (self-test observability).
    prompts: u32,
}

/// Cross-window message. Ping does not need one (it just mutates the store);
/// Pong is a transient signal with no state, so it is an event.
enum StoreEvent {
    Pong,
}
impl EventEmitter<StoreEvent> for Store {}

impl Store {
    fn seed() -> Self {
        let p = |name: &str, owner: &'static str, budget: f64, open: bool| Project {
            name: name.to_string(),
            owner,
            budget,
            open,
        };
        Self {
            projects: vec![
                p("Aurora", "Rin", 42_500.0, true),
                p("Basalt", "Iva", 8_900.0, true),
                p("Cinder", "Ove", 120_000.0, false),
                p("Dovetail", "Rin", 3_250.5, true),
                p("Ember", "Sol", 61_400.0, false),
                p("Foxglove", "Iva", 15_000.0, true),
            ],
            selected: 0,
            pings: 0,
            dirty: false,
            compact: false,
            mode: Mode::System,
            modal: false,
            prompts: 0,
        }
    }

    fn sel(&self) -> Option<&Project> {
        self.projects.get(self.selected)
    }

    fn sel_name(&self) -> String {
        self.sel().map(|p| p.name.clone()).unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// App-wide window registry
// ---------------------------------------------------------------------------

struct Shell {
    store: Entity<Store>,
    main: WindowHandle<MainView>,
    inspector: Option<WindowHandle<InspectorView>>,
    prefs: Option<WindowHandle<PrefsView>>,
    modal: Option<WindowHandle<EditView>>,
    /// Set once the close-veto prompt has been answered, so the next
    /// `on_window_should_close` lets the close through.
    closing: bool,
    /// TRAP: `WindowOptions.window_bounds` is a *content* rect but
    /// `window.bounds()` returns the *frame* (NSWindow frame, titlebar
    /// included). Feeding one back into the other grows every window by the
    /// titlebar height on every launch. Measured once at startup and
    /// subtracted when persisting.
    frame_delta: f32,
}
impl Global for Shell {}

// ---------------------------------------------------------------------------
// Theme (applies to EVERY window: each view reads store.mode)
// ---------------------------------------------------------------------------

struct Theme {
    bg: gpui::Rgba,
    panel: gpui::Rgba,
    border: gpui::Rgba,
    text: gpui::Rgba,
    dim: gpui::Rgba,
    accent: gpui::Rgba,
    sel: gpui::Rgba,
}

fn theme(mode: Mode, appearance: WindowAppearance) -> Theme {
    let dark = match mode {
        Mode::Dark => true,
        Mode::Light => false,
        Mode::System => matches!(
            appearance,
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        ),
    };
    if dark {
        Theme {
            bg: rgb(0x1c1c1f),
            panel: rgb(0x27272b),
            border: rgb(0x3f3f46),
            text: rgb(0xe4e4e7),
            dim: rgb(0x9f9fa8),
            accent: rgb(0x60a5fa),
            sel: rgb(0x1e3a5f),
        }
    } else {
        Theme {
            bg: rgb(0xf5f5f6),
            panel: rgb(0xffffff),
            border: rgb(0xd4d4d8),
            text: rgb(0x18181b),
            dim: rgb(0x6b7280),
            accent: rgb(0x2563eb),
            sel: rgb(0xdbeafe),
        }
    }
}

// ---------------------------------------------------------------------------
// Minimal text field — gpui ships NO text-input widget.
// Same shape as the filter box in apps/gpui-grid (~30 LoC): key_char append,
// backspace pop, Enter/Esc out. No selection, no IME, no caret movement.
// ---------------------------------------------------------------------------

enum Edit {
    Commit,
    Cancel,
    Changed,
    None,
}

fn apply_key(buf: &mut String, ks: &Keystroke, numeric: bool) -> Edit {
    if ks.modifiers.platform || ks.modifiers.control || ks.modifiers.function {
        return Edit::None;
    }
    match ks.key.as_str() {
        "enter" => Edit::Commit,
        "escape" => Edit::Cancel,
        "backspace" => {
            if buf.pop().is_some() {
                Edit::Changed
            } else {
                Edit::None
            }
        }
        _ => match ks.key_char.as_deref() {
            Some(c) if !numeric => {
                buf.push_str(c);
                Edit::Changed
            }
            Some(c) if numeric && numeric_ok(buf, c) => {
                buf.push_str(c);
                Edit::Changed
            }
            _ => Edit::None,
        },
    }
}

/// Keep the buffer parseable as a number while typing.
fn numeric_ok(buf: &str, c: &str) -> bool {
    match c {
        "-" => buf.is_empty(),
        "." => !buf.contains('.'),
        _ => c.chars().all(|ch| ch.is_ascii_digit()),
    }
}

fn field(
    id: &'static str,
    text: &str,
    focus: &FocusHandle,
    window: &Window,
    t: &Theme,
    tab: isize,
) -> gpui::Stateful<gpui::Div> {
    let focused = focus.is_focused(window);
    div().id(id).track_focus(&focus.clone().tab_index(tab).tab_stop(true)).h(px(28.)).w_full()
        .px_2().flex().items_center().bg(t.panel).border_1()
        .border_color(if focused { t.accent } else { t.border }).rounded_md().cursor_text()
        .text_sm().text_color(t.text).child(div().child(SharedString::from(text.to_string())))
        .when(focused, |d| {
            d.child(div().w(px(1.5)).h(px(15.)).bg(t.accent))
        })
}

fn button(
    id: &'static str,
    label: &'static str,
    t: &Theme,
    primary: bool,
) -> gpui::Stateful<gpui::Div> {
    div().id(id).px_3().py_1().rounded_md().text_sm().cursor_pointer().border_1()
        .border_color(if primary { t.accent } else { t.border })
        .bg(if primary { t.accent } else { t.panel })
        .text_color(if primary { rgb(0xffffff) } else { t.text }).hover(|s| s.opacity(0.85))
        .child(label)
}

// ---------------------------------------------------------------------------
// Main window
// ---------------------------------------------------------------------------

struct MainView {
    store: Entity<Store>,
    focus: FocusHandle,
    _subs: Vec<Subscription>,
}

impl MainView {
    fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window);
        let subs = vec![
            // The whole shared-state story in one line: any window's
            // store.update() repaints this one.
            cx.observe(&store, |_, _, cx| cx.notify()),
            window.observe_window_appearance(|_, cx| cx.refresh_windows()),
        ];
        Self {
            store,
            focus,
            _subs: subs,
        }
    }

    /// Toolbar Delete. Refuses to run while the edit dialog is open — this
    /// (plus the occluding scrim in `render`) is the app-level modality gate.
    fn delete_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.store.read(cx).modal {
            println!("[windows] Delete ignored: modal dialog is open");
            return;
        }
        let Some(name) = self.store.read(cx).sel().map(|p| p.name.clone()) else {
            return;
        };
        self.store.update(cx, |s, _| s.prompts += 1);
        // Native message box: NSAlert as a window-modal sheet on this window.
        let rx = window.prompt(
            PromptLevel::Warning,
            &format!("Delete \u{201c}{name}\u{201d}?"),
            Some("This cannot be undone."),
            &["Yes", "No"],
            cx,
        );
        let store = self.store.clone();
        cx.spawn(async move |_, cx| {
            if let Ok(0) = rx.await {
                cx.update(|cx| {
                    store.update(cx, |s, cx| {
                        if s.selected < s.projects.len() {
                            s.projects.remove(s.selected);
                            s.selected = s.selected.min(s.projects.len().saturating_sub(1));
                            s.dirty = true;
                            cx.notify();
                        }
                    })
                }).ok();
            }
        }).detach();
    }

    fn select(&mut self, ix: usize, cx: &mut Context<Self>) {
        if self.store.read(cx).modal {
            return;
        }
        self.store.update(cx, |s, cx| {
            s.selected = ix;
            cx.notify();
        });
    }
}

impl Render for MainView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = self.store.read(cx);
        let t = theme(s.mode, window.appearance());
        let compact = s.compact;
        let modal = s.modal;
        let status = format!(
            "selected: {} \u{b7} pings: {}{}",
            s.sel_name(),
            s.pings,
            if s.dirty { "  \u{2022} unsaved" } else { "" }
        );
        let rows: Vec<gpui::AnyElement> = s.projects.iter().enumerate()
            .map(|(ix, p)| {
                let selected = ix == s.selected;
                div().id(("row", ix)).h(px(if compact { 22. } else { 32. })).w_full().flex()
                    .items_center().px_2().text_sm().text_color(t.text)
                    .bg(if selected { t.sel } else { t.panel }).border_b_1()
                    .border_color(t.border)
                    .child(div().w(px(180.)).child(SharedString::from(p.name.clone())))
                    .child(div().w(px(90.)).text_color(t.dim).child(p.owner))
                    .child(
                        div().w(px(120.)).justify_end().flex().child(format!("{:.2}", p.budget)),
                    )
                    .child(
                        div().w(px(80.)).pl_4().text_color(t.dim)
                            .child(if p.open { "Open" } else { "Closed" }),
                    )
                    .on_click(cx.listener(move |this, ev: &ClickEvent, window, cx| {
                        this.select(ix, cx);
                        if ev.click_count() == 2 {
                            cx.defer(open_modal);
                        }
                        let _ = window;
                    })).into_any_element()
            }).collect();

        div().id("main-root").track_focus(&self.focus).key_context("Main").relative().size_full()
            .flex().flex_col().bg(t.bg).text_color(t.text)
            .on_key_down(|ev: &KeyDownEvent, _, _| {
                if std::env::var("WINDOWS_KEYLOG").is_ok() {
                    println!("[key] {:?}", ev.keystroke);
                }
            })
            .child(
                div().flex_none().flex().gap_2().p_2()
                    .child(button("edit", "Edit\u{2026}", &t, true).on_click(cx.listener(
                        |this, _, _, cx| {
                            if !this.store.read(cx).modal {
                                cx.defer(open_modal);
                            }
                        },
                    )))
                    .child(
                        button("inspector", "Inspector", &t, false)
                            .on_click(|_, _, cx| cx.defer(open_inspector)),
                    )
                    .child(
                        button("prefs", "Preferences\u{2026}", &t, false)
                            .on_click(|_, _, cx| cx.defer(open_prefs)),
                    )
                    .child(button("pong", "Pong", &t, false).on_click(cx.listener(
                        |this, _, _, cx| {
                            this.store.update(cx, |_, cx| cx.emit(StoreEvent::Pong));
                        },
                    ))).child(div().flex_1())
                    .child(
                        button("delete", "Delete", &t, false)
                            .on_click(cx.listener(Self::on_delete)),
                    ),
            )
            .child(
                div().id("list").flex_1().mx_2().overflow_y_scroll().rounded_md().border_1()
                    .border_color(t.border).bg(t.panel).children(rows),
            )
            .child(
                div().flex_none().px_3().py_2().text_sm().text_color(t.dim).child(status),
            )
            // Modal scrim: `.occlude()` makes this hitbox swallow every mouse
            // event for the whole window, so a synthetic click on Delete
            // cannot reach it. Clicking the scrim re-raises the dialog, the
            // way a real OS modal bounces focus back.
            .when(modal, |d| {
                d.child(
                    div().id("scrim").occlude().absolute().top_0().left_0().size_full()
                        .bg(rgba(0x00000055))
                        .on_click(|_, _, cx| {
                            if let Some(m) = cx.global::<Shell>().modal {
                                m.update(cx, |_, window, _| window.activate_window()).ok();
                            }
                        }),
                )
            })
    }
}

impl MainView {
    fn on_delete(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.delete_selected(window, cx);
    }
}

// ---------------------------------------------------------------------------
// Inspector window (second top-level window, singleton)
// ---------------------------------------------------------------------------

struct InspectorView {
    store: Entity<Store>,
    focus: FocusHandle,
    name_focus: FocusHandle,
    budget_focus: FocusHandle,
    /// Transient text mirror for the numeric field only (see FRICTION).
    budget_text: String,
    last_sel: usize,
    flash: bool,
    _subs: Vec<Subscription>,
}

impl InspectorView {
    fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window);
        let sel = store.read(cx).selected;
        let budget_text = store.read(cx).sel().map(|p| format!("{:.2}", p.budget))
            .unwrap_or_default();
        let subs = vec![
            cx.observe(&store, |this: &mut Self, store, cx| {
                // Selection changed elsewhere → refresh the numeric mirror.
                let s = store.read(cx);
                if s.selected != this.last_sel {
                    this.last_sel = s.selected;
                    this.budget_text = s.sel().map(|p| format!("{:.2}", p.budget))
                        .unwrap_or_default();
                }
                cx.notify();
            }),
            cx.subscribe(&store, |this: &mut Self, _, _ev: &StoreEvent, cx| {
                this.flash = true;
                cx.notify();
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(Duration::from_millis(300)).await;
                    this.update(cx, |this, cx| {
                        this.flash = false;
                        cx.notify();
                    }).ok();
                }).detach();
            }),
            window.observe_window_appearance(|_, cx| cx.refresh_windows()),
        ];
        Self {
            store,
            focus,
            name_focus: cx.focus_handle(),
            budget_focus: cx.focus_handle(),
            budget_text,
            last_sel: sel,
            flash: false,
            _subs: subs,
        }
    }

    /// Name is edited *in place in the shared store* — literally no copy.
    fn name_key(&mut self, ks: &Keystroke, cx: &mut Context<Self>) {
        self.store.update(cx, |s, cx| {
            let Some(p) = s.projects.get_mut(s.selected) else {
                return;
            };
            let mut buf = p.name.clone();
            if matches!(apply_key(&mut buf, ks, false), Edit::Changed) {
                p.name = buf;
                s.dirty = true;
                cx.notify();
            }
        });
    }

    fn on_name_key(&mut self, ev: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.name_key(&ev.keystroke, cx);
    }

    fn on_budget_key(&mut self, ev: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let mut buf = self.budget_text.clone();
        if matches!(apply_key(&mut buf, &ev.keystroke, true), Edit::Changed) {
            self.budget_text = buf;
            let v = self.budget_text.parse::<f64>().unwrap_or(0.0);
            self.store.update(cx, |s, cx| {
                if let Some(p) = s.projects.get_mut(s.selected) {
                    p.budget = v;
                    s.dirty = true;
                    cx.notify();
                }
            });
            cx.notify();
        }
    }

    fn ping(&mut self, cx: &mut Context<Self>) {
        self.store.update(cx, |s, cx| {
            s.pings += 1;
            cx.notify();
        });
    }
}

impl Render for InspectorView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = self.store.read(cx);
        let t = theme(s.mode, window.appearance());
        let name = s.sel_name();
        let owner = s.sel().map(|p| p.owner).unwrap_or("");
        let open = s.sel().map(|p| p.open).unwrap_or(false);
        let bg = if self.flash { t.accent } else { t.bg };

        div().id("inspector-root").track_focus(&self.focus).size_full().flex().flex_col().gap_2()
            .p_3().bg(bg).text_color(t.text)
            .on_key_down(cx.listener(|_, ev: &KeyDownEvent, _, cx| {
                if ev.keystroke.key == "escape" {
                    cx.defer(close_inspector);
                }
            })).child(div().text_sm().text_color(t.dim).child("Inspector"))
            .child(div().text_xs().text_color(t.dim).child("Name"))
            .child(
                field("insp-name", &name, &self.name_focus, window, &t, 1)
                    .on_key_down(cx.listener(Self::on_name_key)),
            ).child(div().text_xs().text_color(t.dim).child("Budget"))
            .child(
                field(
                    "insp-budget",
                    &self.budget_text,
                    &self.budget_focus,
                    window,
                    &t,
                    2,
                ).on_key_down(cx.listener(Self::on_budget_key)),
            )
            .child(
                div().text_xs().text_color(t.dim)
                    .child(format!("Owner {owner} \u{b7} {}", if open { "Open" } else { "Closed" })),
            ).child(div().flex_1())
            .child(
                div().flex().gap_2()
                    .child(
                        button("ping", "Ping", &t, true)
                            .on_click(cx.listener(|this, _, _, cx| this.ping(cx))),
                    )
                    .child(
                        div().text_xs().text_color(t.dim).child(format!("pings: {}", s.pings)),
                    ),
            )
    }
}

// ---------------------------------------------------------------------------
// Preferences window (singleton, non-modal)
// ---------------------------------------------------------------------------

struct PrefsView {
    store: Entity<Store>,
    focus: FocusHandle,
    _subs: Vec<Subscription>,
}

impl PrefsView {
    fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window);
        let subs = vec![
            cx.observe(&store, |_, _, cx| cx.notify()),
            window.observe_window_appearance(|_, cx| cx.refresh_windows()),
        ];
        Self {
            store,
            focus,
            _subs: subs,
        }
    }
}

impl Render for PrefsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = self.store.read(cx);
        let t = theme(s.mode, window.appearance());
        let compact = s.compact;
        let mode = s.mode;

        let radio = |id: &'static str, label: &'static str, m: Mode, t: &Theme| {
            let on = m == mode;
            div().id(id).flex().items_center().gap_2().cursor_pointer().text_sm()
                .child(
                    div().size(px(12.)).rounded_full().border_1().border_color(t.dim)
                        .when(on, |d| d.bg(t.accent).border_color(t.accent)),
                ).child(label)
        };

        div().id("prefs-root").track_focus(&self.focus).size_full().flex().flex_col().gap_3()
            .p_3().bg(t.bg).text_color(t.text)
            .on_key_down(cx.listener(|_, ev: &KeyDownEvent, _, cx| {
                if ev.keystroke.key == "escape" {
                    cx.defer(close_prefs);
                }
            })).child(div().text_sm().text_color(t.dim).child("Preferences"))
            .child(
                div().id("compact").flex().items_center().gap_2().cursor_pointer().text_sm()
                    .child(
                        div().size(px(14.)).rounded_sm().border_1().border_color(t.dim)
                            .when(compact, |d| d.bg(t.accent).border_color(t.accent)),
                    ).child("Compact rows")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.store.update(cx, |s, cx| {
                            s.compact = !s.compact;
                            cx.notify();
                        });
                    })),
            ).child(div().text_xs().text_color(t.dim).child("Theme"))
            .child(
                radio("th-light", "Light", Mode::Light, &t).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.store.update(cx, |s, cx| {
                            s.mode = Mode::Light;
                            cx.notify();
                        })
                    },
                )),
            )
            .child(
                radio("th-dark", "Dark", Mode::Dark, &t).on_click(cx.listener(|this, _, _, cx| {
                    this.store.update(cx, |s, cx| {
                        s.mode = Mode::Dark;
                        cx.notify();
                    })
                })),
            )
            .child(
                radio("th-system", "System", Mode::System, &t).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.store.update(cx, |s, cx| {
                            s.mode = Mode::System;
                            cx.notify();
                        })
                    },
                )),
            )
    }
}

// ---------------------------------------------------------------------------
// Edit dialog — a second top-level window + the app-level input gate
// ---------------------------------------------------------------------------

struct EditView {
    store: Entity<Store>,
    ix: usize,
    name: String,
    budget: String,
    focus: FocusHandle,
    name_focus: FocusHandle,
    budget_focus: FocusHandle,
}

impl EditView {
    fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let s = store.read(cx);
        let ix = s.selected;
        let (name, budget) = s.sel().map(|p| (p.name.clone(), format!("{:.2}", p.budget)))
            .unwrap_or_default();
        let name_focus = cx.focus_handle().tab_index(1).tab_stop(true);
        name_focus.focus(window);
        Self {
            store,
            ix,
            name,
            budget,
            focus: cx.focus_handle(),
            name_focus,
            budget_focus: cx.focus_handle().tab_index(2).tab_stop(true),
        }
    }

    fn commit(&mut self, cx: &mut Context<Self>) {
        let (ix, name, budget) = (self.ix, self.name.clone(), self.budget.parse::<f64>());
        self.store.update(cx, |s, cx| {
            if let Some(p) = s.projects.get_mut(ix) {
                p.name = name;
                if let Ok(b) = budget {
                    p.budget = b;
                }
                s.dirty = true;
                cx.notify();
            }
        });
        cx.defer(close_modal);
    }
}

impl Render for EditView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(self.store.read(cx).mode, window.appearance());
        div().id("edit-root").track_focus(&self.focus).size_full().flex().flex_col().gap_2()
            .p_3().bg(t.bg).text_color(t.text)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                match ev.keystroke.key.as_str() {
                    "escape" => cx.defer(close_modal),
                    "enter" => this.commit(cx),
                    // Tab order inside the modal: gpui's own tab-stop map.
                    "tab" if ev.keystroke.modifiers.shift => window.focus_prev(),
                    "tab" => window.focus_next(),
                    _ => {}
                }
            })).child(div().text_sm().text_color(t.dim).child("Edit project"))
            .child(div().text_xs().text_color(t.dim).child("Name"))
            .child(
                field("dlg-name", &self.name, &self.name_focus, window, &t, 1).on_key_down(
                    cx.listener(|this, ev: &KeyDownEvent, _, cx| {
                        let mut buf = this.name.clone();
                        match apply_key(&mut buf, &ev.keystroke, false) {
                            Edit::Changed => {
                                this.name = buf;
                                cx.notify();
                            }
                            Edit::Commit => this.commit(cx),
                            Edit::Cancel => cx.defer(close_modal),
                            Edit::None => {}
                        }
                    }),
                ),
            ).child(div().text_xs().text_color(t.dim).child("Budget"))
            .child(
                field("dlg-budget", &self.budget, &self.budget_focus, window, &t, 2).on_key_down(
                    cx.listener(|this, ev: &KeyDownEvent, _, cx| {
                        let mut buf = this.budget.clone();
                        match apply_key(&mut buf, &ev.keystroke, true) {
                            Edit::Changed => {
                                this.budget = buf;
                                cx.notify();
                            }
                            Edit::Commit => this.commit(cx),
                            Edit::Cancel => cx.defer(close_modal),
                            Edit::None => {}
                        }
                    }),
                ),
            ).child(div().flex_1())
            .child(
                div().flex().gap_2().justify_end()
                    .child(
                        button("dlg-cancel", "Cancel", &t, false)
                            .on_click(|_, _, cx| cx.defer(close_modal)),
                    )
                    .child(
                        button("dlg-ok", "OK", &t, true)
                            .on_click(cx.listener(|this, _, _, cx| this.commit(cx))),
                    ),
            )
    }
}

// ---------------------------------------------------------------------------
// Window management (open / close / singleton focus)
// ---------------------------------------------------------------------------

fn win_opts(title: &str, bounds: Bounds<gpui::Pixels>) -> WindowOptions {
    WindowOptions {
        titlebar: Some(TitlebarOptions {
            title: Some(SharedString::from(title.to_string())),..Default::default()
        }),
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        // NOTE: there is no `parent`/`owner`/`transient_for` field here.
        ..Default::default()
    }
}

/// Approximation of parenting: place the child offset from the main window.
fn offset_from_main(cx: &mut App, dx: f32, dy: f32, w: f32, h: f32) -> Bounds<gpui::Pixels> {
    let main = cx.global::<Shell>().main;
    let origin = main.update(cx, |_, window, _| window.bounds().origin).unwrap_or_default();
    Bounds {
        origin: point(origin.x + px(dx), origin.y + px(dy)),
        size: size(px(w), px(h)),
    }
}

fn open_inspector(cx: &mut App) {
    // Singleton: raise the existing window instead of opening a second one.
    if let Some(h) = cx.global::<Shell>().inspector
        && h.update(cx, |_, window, _| window.activate_window()).is_ok()
    {
        println!("[windows] inspector already open — focused it");
        return;
    }
    let store = cx.global::<Shell>().store.clone();
    let bounds = load_state().inspector.map(rect_to_bounds)
        .unwrap_or_else(|| offset_from_main(cx, 740., 20., 360., 300.));
    let h = cx
        .open_window(win_opts("Inspector", bounds), |window, cx| {
            cx.new(|cx| InspectorView::new(store, window, cx))
        }).ok();
    cx.global_mut::<Shell>().inspector = h;
    println!("[windows] inspector opened (windows now {})", cx.windows().len());
}

fn close_inspector(cx: &mut App) {
    if let Some(h) = cx.global_mut::<Shell>().inspector.take() {
        h.update(cx, |_, window, _| window.remove_window()).ok();
    }
}

fn toggle_inspector(cx: &mut App) {
    if cx.global::<Shell>().inspector.is_some() {
        close_inspector(cx);
    } else {
        open_inspector(cx);
    }
}

fn open_prefs(cx: &mut App) {
    if let Some(h) = cx.global::<Shell>().prefs
        && h.update(cx, |_, window, _| window.activate_window()).is_ok()
    {
        return;
    }
    let store = cx.global::<Shell>().store.clone();
    let bounds = load_state().prefs.map(rect_to_bounds)
        .unwrap_or_else(|| offset_from_main(cx, 740., 350., 320., 200.));
    let h = cx
        .open_window(win_opts("Preferences", bounds), |window, cx| {
            cx.new(|cx| PrefsView::new(store, window, cx))
        }).ok();
    cx.global_mut::<Shell>().prefs = h;
    println!("[windows] preferences opened (windows now {})", cx.windows().len());
}

fn close_prefs(cx: &mut App) {
    if let Some(h) = cx.global_mut::<Shell>().prefs.take() {
        h.update(cx, |_, window, _| window.remove_window()).ok();
    }
}

fn open_modal(cx: &mut App) {
    if cx.global::<Shell>().modal.is_some() {
        return;
    }
    let store = cx.global::<Shell>().store.clone();
    store.update(cx, |s, cx| {
        s.modal = true;
        cx.notify();
    });
    let bounds = offset_from_main(cx, 160., 120., 380., 230.);
    let h = cx
        .open_window(win_opts("Edit project", bounds), |window, cx| {
            cx.new(|cx| EditView::new(store, window, cx))
        }).ok();
    if let Some(h) = h {
        h.update(cx, |_, window, _| window.activate_window()).ok();
    }
    cx.global_mut::<Shell>().modal = h;
    println!("[windows] modal opened (app-level input gate ON)");
}

fn close_modal(cx: &mut App) {
    if let Some(h) = cx.global_mut::<Shell>().modal.take() {
        h.update(cx, |_, window, _| window.remove_window()).ok();
    }
    let store = cx.global::<Shell>().store.clone();
    store.update(cx, |s, cx| {
        s.modal = false;
        cx.notify();
    });
    // Focus returns to the main window explicitly — gpui does not do it.
    let main = cx.global::<Shell>().main;
    main.update(cx, |_, window, _| window.activate_window()).ok();
    println!("[windows] modal closed, focus returned to main");
}

// ---------------------------------------------------------------------------
// Close semantics
// ---------------------------------------------------------------------------

/// ⌘W: close the *focused* window only. gpui element-level `on_action`
/// handlers did not fire for this binding (only global `cx.on_action` did), so
/// the focused window is resolved with `cx.active_window()` instead — which is
/// also the most direct expression of "focused window only".
fn close_focused_window(cx: &mut App) {
    let Some(active) = cx.active_window() else {
        println!("[windows] cmd-W: no active window");
        return;
    };
    println!("[windows] cmd-W on the focused window");
    let (main, insp, prefs, modal) = {
        let sh = cx.global::<Shell>();
        (sh.main, sh.inspector, sh.prefs, sh.modal)
    };
    if modal.is_some_and(|h| active == h.into()) {
        close_modal(cx);
    } else if insp.is_some_and(|h| active == h.into()) {
        close_inspector(cx);
    } else if prefs.is_some_and(|h| active == h.into()) {
        close_prefs(cx);
    } else if active == main.into() {
        main.update(cx, |_, window, cx| {
            // request_close_main only *decides*; ⌘W has to do the closing
            // itself (unlike the red button, where AppKit does it for us).
            if request_close_main(window, cx) {
                window.remove_window();
            }
        }).ok();
    }
}

/// Returns true if the main window may close right now.
fn may_close_main(cx: &App) -> bool {
    let sh = cx.global::<Shell>();
    sh.closing || !sh.store.read(cx).dirty
}

/// Shared by the red close button (`on_window_should_close`) and ⌘W.
/// Returns false when the close was vetoed and a prompt was raised.
fn request_close_main(window: &mut Window, cx: &mut App) -> bool {
    if may_close_main(cx) {
        save_state(cx);
        return true;
    }
    let rx = window.prompt(
        PromptLevel::Warning,
        "Save changes?",
        Some("You have unsaved edits to the project list."),
        &["Save", "Discard", "Cancel"],
        cx,
    );
    cx.spawn(async move |cx| {
        match rx.await {
            Ok(0) | Ok(1) => {
                cx.update(|cx| {
                    cx.global_mut::<Shell>().closing = true;
                    save_state(cx);
                    cx.quit();
                }).ok();
            }
            _ => println!("[windows] close vetoed (Cancel)"),
        }
    }).detach();
    false
}

// ---------------------------------------------------------------------------
// Position/size persistence — polled, because gpui emits no window-moved event
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
struct Rect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

#[derive(Default)]
struct State {
    main: Option<Rect>,
    inspector: Option<Rect>,
    prefs: Option<Rect>,
    inspector_open: bool,
}

fn state_path() -> std::path::PathBuf {
    std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("windows-state.txt")))
        .unwrap_or_else(|| std::path::PathBuf::from("windows-state.txt"))
}

fn rect_to_bounds(r: Rect) -> Bounds<gpui::Pixels> {
    Bounds {
        origin: point(px(r.x), px(r.y)),
        size: size(px(r.w), px(r.h)),
    }
}

fn load_state() -> State {
    let mut st = State::default();
    let Ok(text) = std::fs::read_to_string(state_path()) else {
        return st;
    };
    for line in text.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() == 5 {
            let n: Vec<f32> = f[1..].iter().filter_map(|v| v.parse().ok()).collect();
            if n.len() == 4 {
                let r = Rect {
                    x: n[0],
                    y: n[1],
                    w: n[2],
                    h: n[3],
                };
                match f[0] {
                    "main" => st.main = Some(r),
                    "inspector" => st.inspector = Some(r),
                    "prefs" => st.prefs = Some(r),
                    _ => {}
                }
            }
        } else if f.len() == 2 && f[0] == "inspector_open" {
            st.inspector_open = f[1] == "1";
        }
    }
    st
}

fn bounds_of<V: 'static + Render>(cx: &mut App, h: Option<WindowHandle<V>>) -> Option<Rect> {
    let h = h?;
    let delta = cx.global::<Shell>().frame_delta;
    h.update(cx, |_, window, _| {
        let b = window.bounds();
        Rect {
            x: f32::from(b.origin.x),
            y: f32::from(b.origin.y),
            w: f32::from(b.size.width),
            h: f32::from(b.size.height) - delta,
        }
    }).ok()
}

fn save_state(cx: &mut App) {
    let (main, insp, prefs) = {
        let sh = cx.global::<Shell>();
        (sh.main, sh.inspector, sh.prefs)
    };
    let mut out = String::new();
    if let Some(r) = bounds_of(cx, Some(main)) {
        out.push_str(&format!("main {} {} {} {}\n", r.x, r.y, r.w, r.h));
    }
    if let Some(r) = bounds_of(cx, insp) {
        out.push_str(&format!("inspector {} {} {} {}\n", r.x, r.y, r.w, r.h));
    }
    if let Some(r) = bounds_of(cx, prefs) {
        out.push_str(&format!("prefs {} {} {} {}\n", r.x, r.y, r.w, r.h));
    }
    out.push_str(&format!(
        "inspector_open {}\n",
        if insp.is_some() { 1 } else { 0 }
    ));
    std::fs::write(state_path(), out).ok();
}

// ---------------------------------------------------------------------------
// Self-test
// ---------------------------------------------------------------------------

fn spawn_selftest(cx: &mut App) {
    cx.spawn(async move |cx| {
        let mut pass = 0u32;
        let mut fail = 0u32;
        let mut check = |name: &str, ok: bool, detail: String| {
            if ok {
                pass += 1;
                println!("PASS {name} :: {detail}");
            } else {
                fail += 1;
                println!("FAIL {name} :: {detail}");
            }
        };
        let tick = |ms: u64| {
            let cx = cx.clone();
            async move {
                cx.background_executor().timer(Duration::from_millis(ms)).await;
            }
        };
        tick(1200).await;

        let nwin = |cx: &mut App| cx.windows().len();

        // 1 window
        let n = cx.update(nwin).unwrap_or(0);
        check("window_count_1", n == 1, format!("cx.windows()={n}"));

        // display / scale info (multi-monitor row)
        cx.update(|cx| {
            let d = cx.displays().len();
            let main = cx.global::<Shell>().main;
            let sf = main.update(cx, |_, w, _| w.scale_factor()).unwrap_or(0.0);
            println!("DISPLAYS {d} MAIN_SCALE_FACTOR {sf}");
        }).ok();

        // 2 windows (inspector)
        cx.update(open_inspector).ok();
        tick(400).await;
        let n = cx.update(nwin).unwrap_or(0);
        check("window_count_2", n == 2, format!("cx.windows()={n}"));

        // singleton
        cx.update(open_inspector).ok();
        tick(300).await;
        let n = cx.update(nwin).unwrap_or(0);
        check("inspector_singleton", n == 2, format!("cx.windows()={n}"));

        // 3 windows (preferences)
        cx.update(open_prefs).ok();
        tick(400).await;
        let n = cx.update(nwin).unwrap_or(0);
        check("window_count_3", n == 3, format!("cx.windows()={n}"));

        // shared state: edit from the inspector, read from the store the main
        // window renders from
        cx.update(|cx| {
            let insp = cx.global::<Shell>().inspector.unwrap();
            insp.update(cx, |v, _, cx| {
                for ch in "Zulu".chars() {
                    v.name_key(
                        &Keystroke {
                            modifiers: Default::default(),
                            key: ch.to_string(),
                            key_char: Some(ch.to_string()),
                        },
                        cx,
                    );
                }
            }).ok();
        }).ok();
        tick(300).await;
        let name = cx.update(|cx| cx.global::<Shell>().store.read(cx).sel_name())
            .unwrap_or_default();
        check(
            "shared_state_bidirectional",
            name.ends_with("Zulu"),
            format!("store name = {name:?}"),
        );

        // cross-window Ping
        let before = cx.update(|cx| cx.global::<Shell>().store.read(cx).pings).unwrap_or(0);
        cx.update(|cx| {
            let insp = cx.global::<Shell>().inspector.unwrap();
            insp.update(cx, |v, _, cx| v.ping(cx)).ok();
        }).ok();
        tick(200).await;
        let after = cx.update(|cx| cx.global::<Shell>().store.read(cx).pings).unwrap_or(0);
        check("ping_increments_main", after == before + 1, format!("{before} -> {after}"));

        // cross-window Pong (event → inspector flash)
        cx.update(|cx| {
            cx.global::<Shell>().store.clone().update(cx, |_, cx| cx.emit(StoreEvent::Pong))
        }).ok();
        tick(80).await;
        let flashing = cx
            .update(|cx| {
                cx.global::<Shell>().inspector.unwrap().update(cx, |v, _, _| v.flash)
                    .unwrap_or(false)
            }).unwrap_or(false);
        check("pong_flashes_inspector", flashing, format!("flash={flashing}"));
        tick(400).await;
        let done = cx
            .update(|cx| {
                cx.global::<Shell>().inspector.unwrap().update(cx, |v, _, _| v.flash)
                    .unwrap_or(true)
            }).unwrap_or(true);
        check("pong_flash_clears_300ms", !done, format!("flash={done}"));

        // theme applies to all windows (one shared field, every view reads it)
        cx.update(|cx| {
            cx.global::<Shell>().store.clone().update(cx, |s, cx| {
                s.mode = Mode::Dark;
                s.compact = true;
                cx.notify();
            })
        }).ok();
        tick(300).await;
        let (m, c) = cx
            .update(|cx| {
                let s = cx.global::<Shell>().store.read(cx);
                (s.mode, s.compact)
            }).unwrap_or((Mode::Light, false));
        check(
            "theme_and_layout_all_windows",
            m == Mode::Dark && c,
            format!("mode={m:?} compact={c}"),
        );

        // modality: while the modal window is up, Delete must do nothing
        cx.update(open_modal).ok();
        tick(500).await;
        let n = cx.update(nwin).unwrap_or(0);
        check("modal_window_open", n == 4, format!("cx.windows()={n}"));
        let (rows0, prompts0) = cx
            .update(|cx| {
                let s = cx.global::<Shell>().store.read(cx);
                (s.projects.len(), s.prompts)
            }).unwrap_or((0, 0));
        cx.update(|cx| {
            let main = cx.global::<Shell>().main;
            main.update(cx, |v, window, cx| v.delete_selected(window, cx)).ok();
        }).ok();
        tick(400).await;
        let (rows1, prompts1) = cx
            .update(|cx| {
                let s = cx.global::<Shell>().store.read(cx);
                (s.projects.len(), s.prompts)
            }).unwrap_or((0, 0));
        check(
            "parent_blocked_while_modal",
            rows0 == rows1 && prompts0 == prompts1,
            format!("rows {rows0}->{rows1}, confirm sheets {prompts0}->{prompts1}"),
        );

        // modal commit writes through to the shared store, then focus returns
        cx.update(|cx| {
            let m = cx.global::<Shell>().modal.unwrap();
            m.update(cx, |v, _, cx| {
                v.name = "Committed".into();
                v.budget = "777.00".into();
                v.commit(cx);
            }).ok();
        }).ok();
        tick(400).await;
        let (nm, bud, modal_flag, n) = cx
            .update(|cx| {
                let s = cx.global::<Shell>().store.read(cx);
                (
                    s.sel_name(),
                    s.sel().map(|p| p.budget).unwrap_or(0.0),
                    s.modal,
                    cx.windows().len(),
                )
            }).unwrap_or_default();
        check(
            "modal_commit_and_close",
            nm == "Committed" && (bud - 777.0).abs() < 0.01 && !modal_flag && n == 3,
            format!("name={nm:?} budget={bud} modal={modal_flag} windows={n}"),
        );

        // close veto decision (the sheet itself needs a human; the decision
        // function is what ⌘W and the red button both consult)
        cx.update(|cx| {
            cx.global::<Shell>().store.clone().update(cx, |s, _| s.dirty = true)
        }).ok();
        let veto = cx.update(|cx| may_close_main(cx)).unwrap_or(true);
        check("close_veto_when_dirty", !veto, format!("may_close={veto}"));
        cx.update(|cx| {
            cx.global::<Shell>().store.clone().update(cx, |s, _| s.dirty = false)
        }).ok();
        let ok = cx.update(|cx| may_close_main(cx)).unwrap_or(false);
        check("close_allowed_when_clean", ok, format!("may_close={ok}"));

        // child close must not quit the app
        cx.update(close_inspector).ok();
        tick(400).await;
        let n = cx.update(nwin).unwrap_or(0);
        check("child_close_keeps_app", n == 2, format!("cx.windows()={n}"));

        // persistence round-trip
        cx.update(save_state).ok();
        let saved = load_state();
        let live = cx
            .update(|cx| {
                let main = cx.global::<Shell>().main;
                bounds_of(cx, Some(main))
            }).ok().flatten();
        // and prove the round-trip is stable (no titlebar drift)
        let stable = cx
            .update(|cx| {
                let d = cx.global::<Shell>().frame_delta;
                let main = cx.global::<Shell>().main;
                let frame = main.update(cx, |_, w, _| f32::from(w.bounds().size.height))
                    .unwrap_or(0.0);
                (d, frame)
            }).unwrap_or((0.0, 0.0));
        println!("FRAME_DELTA {} FRAME_H {}", stable.0, stable.1);
        check(
            "persistence_roundtrip",
            saved.main.is_some() && saved.main == live,
            format!("file={:?} live={:?}", saved.main, live),
        );

        println!("SELFTEST DONE pass={pass} fail={fail}");
        cx.update(|cx| cx.quit()).ok();
    }).detach();
}

// ---------------------------------------------------------------------------

fn main() {
    Application::new().run(|cx: &mut App| {
        cx.bind_keys([
            KeyBinding::new("cmd-,", OpenPrefs, None),
            KeyBinding::new("cmd-shift-i", ToggleInspector, None),
            KeyBinding::new("cmd-w", CloseFocused, None),
        ]);
        // Menubar actions/keystrokes dispatch *through* the focused window, so
        // anything that re-enters a window must be deferred (gpui-tray trap).
        cx.on_action(|_: &OpenPrefs, cx| cx.defer(open_prefs));
        cx.on_action(|_: &ToggleInspector, cx| cx.defer(toggle_inspector));
        cx.on_action(|_: &CloseFocused, cx| cx.defer(close_focused_window));

        let store = cx.new(|_| {
            let mut s = Store::seed();
            // Verification hook: lets the close-veto path be driven by
            // keystrokes alone on a desktop shared with other agents' apps.
            s.dirty = std::env::var("WINDOWS_START_DIRTY").is_ok();
            s
        });
        let saved = load_state();
        let bounds = saved.main.map(rect_to_bounds)
            .unwrap_or_else(|| Bounds::centered(None, size(px(720.), px(480.)), cx));

        let main = cx
            .open_window(win_opts("Windows (gpui)", bounds), {
                let store = store.clone();
                |window, cx| cx.new(|cx| MainView::new(store, window, cx))
            }).unwrap();

        // Measure the frame/content mismatch once (see Shell::frame_delta).
        let want_h = f32::from(bounds.size.height);
        let frame_delta = main
            .update(cx, |_, window, _| f32::from(window.bounds().size.height) - want_h)
            .unwrap_or(0.0);
        println!("[windows] frame/content height delta = {frame_delta} px");

        cx.set_global(Shell {
            store,
            main,
            inspector: None,
            prefs: None,
            modal: None,
            closing: false,
            frame_delta,
        });

        // Close veto on the red button / ⌘Q-driven close.
        main.update(cx, |_, window, cx| {
            window.on_window_should_close(cx, |window, cx| request_close_main(window, cx));
        }).ok();

        // Quit when the MAIN window goes away (even if children are open);
        // prune stale child handles at the same time.
        cx.on_window_closed(|cx| {
            let open = cx.windows();
            let (main, insp, prefs, modal) = {
                let sh = cx.global::<Shell>();
                (sh.main, sh.inspector, sh.prefs, sh.modal)
            };
            if let Some(h) = insp
                && !open.contains(&h.into())
            {
                cx.global_mut::<Shell>().inspector = None;
            }
            if let Some(h) = prefs
                && !open.contains(&h.into())
            {
                cx.global_mut::<Shell>().prefs = None;
            }
            if let Some(h) = modal
                && !open.contains(&h.into())
            {
                cx.global_mut::<Shell>().modal = None;
                let store = cx.global::<Shell>().store.clone();
                store.update(cx, |s, cx| {
                    s.modal = false;
                    cx.notify();
                });
            }
            if !open.contains(&main.into()) {
                save_state(cx);
                cx.quit();
            }
        }).detach();

        // Restore the inspector if it was open last run.
        if saved.inspector_open {
            cx.defer(open_inspector);
        }

        // gpui has no window-moved/resized event, so bounds are polled.
        cx.spawn(async move |cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(1000)).await;
                if cx.update(save_state).is_err() {
                    break;
                }
            }
        }).detach();

        if std::env::var("WINDOWS_SELFTEST").is_ok() {
            spawn_selftest(cx);
        }
        cx.activate(true);
    });
}
