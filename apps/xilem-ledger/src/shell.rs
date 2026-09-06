//! Everything SPEC-10 needs that lives below xilem's view layer.
//!
//! xilem 0.4 has no focus API, no blur/focus events, and masonry's `TextArea`
//! swallows Up/Down/Escape before any ancestor can see them. So we embed xilem
//! in an *external* winit event loop (upstream `external_event_loop.rs`
//! pattern, same as apps/xilem-tray) and add three things:
//!
//! 1. **key interception** in `window_event` — Up/Down (+Shift) stepping,
//!    Escape-revert, Cmd+Z/Cmd+Shift+Z form undo/redo and Cmd+Shift+C/V row
//!    TSV copy/paste are handled here and *not* forwarded to masonry;
//! 2. **focus observation** — after every event we read
//!    `RenderRoot::focused_widget()` and diff it, which is the only way to get
//!    blur (= "normalise and format on blur") and a tab-order log;
//! 3. a **wrapper `AppDriver`** that applies requested focus moves with
//!    `RenderRoot::focus_on` (Enter-moves-down) and runs the self-test.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use masonry_winit::app::{AppDriver, DriverCtx, MasonryState, MasonryUserEvent};
use xilem::WindowId;
use xilem::masonry::core::keyboard::{Key, KeyState, KeyboardEvent, Modifiers as KbModifiers};
use xilem::masonry::core::{ErasedAction, TextEvent, WidgetId};
use xilem::tokio::sync::mpsc::UnboundedSender;
use xilem::winit::application::ApplicationHandler;
use xilem::winit::event::{ElementState, WindowEvent};
use xilem::winit::event_loop::ActiveEventLoop;
use xilem::winit::keyboard::{Key as WKey, ModifiersState, NamedKey as WNamed};

use crate::model::{Locale, parse_decimal};
use crate::views::{CellKey, Reg};

/// Events flowing from the shell layer into xilem app state.
#[derive(Clone, Debug)]
pub enum Ev {
    /// Focus moved. `from` must be committed (blur), `to` becomes active.
    Focus {
        from: Option<CellKey>,
        to: Option<CellKey>,
    },
    /// Arrow stepping on the focused numeric cell (already multiplied).
    Step { key: CellKey, up: bool, big: bool },
    Revert(CellKey),
    Undo,
    Redo,
    CopyRow,
    PasteRow(String),
    ToggleLocale,
    SetDraft(CellKey, String),
    Tick,
}

/// State shared between the shell layer and xilem app state.
#[derive(Default)]
pub struct Shared {
    pub tx: Mutex<Option<UnboundedSender<Ev>>>,
    /// Requested focus target, applied by the wrapper driver.
    pub want_focus: Mutex<Option<CellKey>>,
    pub reg: Reg,
    /// Display snapshot written by `app_logic` on every rebuild; the self-test
    /// asserts against it, which makes the assertions end-to-end (they see
    /// what the widget tree was told to show).
    pub mirror: Mutex<std::collections::HashMap<&'static str, String>>,
    /// Self-test bookkeeping.
    pub tick: AtomicUsize,
    pub done_step: AtomicUsize,
    pub pass: AtomicUsize,
    pub fail: AtomicUsize,
    pub selftest: bool,
    /// Scripted *demo* mode: runs the same interactions as the self-test up to
    /// step N and then stops, so a screenshot can be taken at a deterministic
    /// point. Needed because sibling research apps float always-on-top windows
    /// over this one, which makes synthetic clicks unreliable.
    pub demo: Option<usize>,
}

impl Shared {
    pub fn send(&self, ev: Ev) {
        if let Some(tx) = self.tx.lock().unwrap().as_ref() {
            let _ = tx.send(ev);
        }
    }
    pub fn get(&self, k: &str) -> String {
        self.mirror.lock().unwrap().get(k).cloned().unwrap_or_default()
    }
    fn check(&self, what: &str, got: impl std::fmt::Debug, want: impl std::fmt::Debug) {
        let ok = format!("{got:?}") == format!("{want:?}");
        if ok {
            self.pass.fetch_add(1, Ordering::SeqCst);
            println!("PASS {what}: {got:?}");
        } else {
            self.fail.fetch_add(1, Ordering::SeqCst);
            println!("FAIL {what}: got {got:?} want {want:?}");
        }
    }
}

// --- MARK: wrapper driver ---

pub struct WrapperDriver {
    inner: Box<dyn AppDriver>,
    window: WindowId,
    shared: Arc<Shared>,
    booted: bool,
}

impl WrapperDriver {
    fn sync(&mut self, ctx: &mut DriverCtx<'_, '_>) {
        // Apply a requested focus move (Enter-moves-down, autofocus, self-test).
        let want = self.shared.want_focus.lock().unwrap().take();
        if let Some(key) = want {
            let id = self.shared.reg.lock().unwrap().by_key.get(&key).copied();
            if let Some(id) = id {
                ctx.render_root(self.window).focus_on(Some(id));
            }
        }
        if self.shared.selftest {
            self.run_selftest(ctx);
        }
        if self.shared.demo.is_some() {
            self.run_demo(ctx);
        }
    }

    fn type_text(&self, ctx: &mut DriverCtx<'_, '_>, text: &str) {
        for ch in text.chars() {
            let ev = KeyboardEvent {
                state: KeyState::Down,
                key: Key::Character(ch.to_string().into()),
                ..Default::default()
            };
            ctx.render_root(self.window)
                .handle_text_event(TextEvent::Keyboard(ev));
        }
    }

    fn press(&self, ctx: &mut DriverCtx<'_, '_>, key: Key, mods: KbModifiers) {
        let ev = KeyboardEvent {
            state: KeyState::Down,
            key,
            modifiers: mods,
            ..Default::default()
        };
        ctx.render_root(self.window)
            .handle_text_event(TextEvent::Keyboard(ev));
    }


    /// Same interactions as the self-test, but stops at `demo` instead of
    /// exiting, and takes no assertions — used to produce screenshots.
    fn run_demo(&mut self, ctx: &mut DriverCtx<'_, '_>) {
        use crate::model::Field;
        use xilem::masonry::core::keyboard::NamedKey;
        let last = self.shared.demo.unwrap_or(0);
        let tick = self.shared.tick.load(Ordering::SeqCst);
        let step = self.shared.done_step.load(Ordering::SeqCst);
        if tick <= step || step > last {
            return;
        }
        self.shared.done_step.store(step + 1, Ordering::SeqCst);
        let focus = |me: &Self, ctx: &mut DriverCtx<'_, '_>, k: CellKey| {
            let id = me.shared.reg.lock().unwrap().by_key.get(&k).copied();
            if let Some(id) = id {
                ctx.render_root(me.window).focus_on(Some(id));
            }
        };
        match step {
            0 => focus(self, ctx, CellKey::Cell(0, Field::Price)),
            1 => {
                self.press(ctx, Key::Character("a".into()), KbModifiers::META);
                self.type_text(ctx, "1234.5");
            }
            2 => {}
            3 => self.press(ctx, Key::Named(NamedKey::Tab), KbModifiers::default()),
            4 => self.shared.send(Ev::ToggleLocale),
            5 => self.shared.send(Ev::ToggleLocale),
            6 => focus(self, ctx, CellKey::Cell(2, Field::Desc)),
            7 => {
                self.press(ctx, Key::Character("a".into()), KbModifiers::META);
                self.press(ctx, Key::Named(NamedKey::Backspace), KbModifiers::default());
                self.press(ctx, Key::Named(NamedKey::Tab), KbModifiers::default());
            }
            8 => focus(self, ctx, CellKey::Cell(3, Field::Cat)),
            9 => {
                self.press(ctx, Key::Character("a".into()), KbModifiers::META);
                self.type_text(ctx, "S");
            }
            _ => {}
        }
        println!("DEMO step {step} done");
    }

    /// One scripted step per tick. Each step either performs an action or
    /// asserts on the display mirror produced by the previous step.
    fn run_selftest(&mut self, ctx: &mut DriverCtx<'_, '_>) {
        use xilem::masonry::core::keyboard::NamedKey;
        let tick = self.shared.tick.load(Ordering::SeqCst);
        let step = self.shared.done_step.load(Ordering::SeqCst);
        if tick <= step {
            return;
        }
        self.shared.done_step.store(step + 1, Ordering::SeqCst);
        let s = self.shared.clone();
        match step {
            0 => {
                // Focus the Unit price cell of row 0 and select its contents.
                let id = s.reg.lock().unwrap().by_key.get(&CellKey::Cell(0, crate::model::Field::Price)).copied();
                if let Some(id) = id {
                    ctx.render_root(self.window).focus_on(Some(id));
                }
                println!("STEP focus row0/Unit price -> {:?}", id);
            }
            1 => {
                self.press(ctx, Key::Character("a".into()), KbModifiers::META);
                self.type_text(ctx, "1234.5");
                println!("STEP typed '1234.5' into row0/Unit price");
            }
            2 => {
                self.press(ctx, Key::Named(NamedKey::Tab), KbModifiers::default());
                println!("STEP pressed Tab (blur commits + formats)");
            }
            3 => {
                s.check("price[0] after '1234.5'+Tab (en-US)", s.get("price0"), "1,234.50");
                s.check("amount[0] recomputed", s.get("amount0"), "1,234.50");
            }
            4 => s.send(Ev::ToggleLocale),
            5 => {
                s.check("price[0] after locale toggle (fr-FR)", s.get("price0"), "1 234,50");
                s.check("total formatted fr-FR", s.get("total").contains(','), true);
                s.send(Ev::ToggleLocale);
            }
            6 => {
                s.check("price[0] back in en-US", s.get("price0"), "1,234.50");
                s.check(
                    "paste '$1,234.56'",
                    parse_decimal("$1,234.56", Locale::EnUs).map(|d| d.to_string()),
                    Some("1234.56".to_string()),
                );
                s.check(
                    "paste '1.234,56 EUR'",
                    parse_decimal("1.234,56 EUR", Locale::EnUs).map(|d| d.to_string()),
                    Some("1234.56".to_string()),
                );
                s.check(
                    "paste '(12.50)' accounting negative",
                    parse_decimal("(12.50)", Locale::EnUs).map(|d| d.to_string()),
                    Some("-12.50".to_string()),
                );
            }
            7 => {
                s.send(Ev::Step {
                    key: CellKey::Cell(0, crate::model::Field::Price),
                    up: true,
                    big: true,
                });
            }
            8 => {
                s.check("Shift+Up steps price by 0.10", s.get("price0"), "1,234.60");
                s.send(Ev::Undo);
            }
            9 => {
                s.check("form-level undo restores price", s.get("price0"), "1,234.50");
                s.send(Ev::SetDraft(CellKey::Cell(2, crate::model::Field::Desc), String::new()));
            }
            10 => {
                s.send(Ev::Focus {
                    from: Some(CellKey::Cell(2, crate::model::Field::Desc)),
                    to: None,
                });
            }
            11 => {
                s.check("empty description is an error", s.get("errors"), "1");
                s.check("Save disabled while invalid", s.get("save"), "disabled");
                s.check("row 0 TSV", s.get("tsv0").split('\t').count(), 7);
            }
            _ => {
                let (p, f) = (s.pass.load(Ordering::SeqCst), s.fail.load(Ordering::SeqCst));
                println!("SELFTEST DONE pass={p} fail={f}");
                use std::io::Write as _;
                let _ = std::io::stdout().flush();
                std::process::exit(0);
            }
        }
    }
}

impl AppDriver for WrapperDriver {
    fn on_action(
        &mut self,
        window_id: WindowId,
        ctx: &mut DriverCtx<'_, '_>,
        widget_id: WidgetId,
        action: ErasedAction,
    ) {
        self.inner.on_action(window_id, ctx, widget_id, action);
        self.sync(ctx);
    }

    fn on_start(&mut self, state: &mut MasonryState<'_>) {
        self.inner.on_start(state);
    }

    fn on_close_requested(&mut self, window_id: WindowId, ctx: &mut DriverCtx<'_, '_>) {
        self.inner.on_close_requested(window_id, ctx);
    }
}

// --- MARK: application handler ---

pub struct ShellApp {
    masonry_state: MasonryState<'static>,
    driver: WrapperDriver,
    shared: Arc<Shared>,
    mods: ModifiersState,
    last_focus: Option<WidgetId>,
}

impl ShellApp {
    pub fn new(
        masonry_state: MasonryState<'static>,
        inner: Box<dyn AppDriver>,
        window: WindowId,
        shared: Arc<Shared>,
    ) -> Self {
        Self {
            masonry_state,
            driver: WrapperDriver {
                inner,
                window,
                shared: shared.clone(),
                booted: false,
            },
            shared,
            mods: ModifiersState::empty(),
            last_focus: None,
        }
    }

    fn focused_cell(&mut self) -> (Option<WidgetId>, Option<CellKey>) {
        let id = self
            .masonry_state
            .roots()
            .next()
            .and_then(|r| r.focused_widget());
        let key = id.and_then(|i| self.shared.reg.lock().unwrap().by_id.get(&i).copied());
        (id, key)
    }

    /// Poll `RenderRoot::focused_widget()` and turn transitions into events.
    /// This is xilem 0.4's only available focus/blur signal.
    fn poll_focus(&mut self) {
        let (id, key) = self.focused_cell();
        if id != self.last_focus {
            let from = self
                .last_focus
                .and_then(|i| self.shared.reg.lock().unwrap().by_id.get(&i).copied());
            self.last_focus = id;
            let label = match (key, id) {
                (Some(k), _) => k.label(),
                // Focusables that are not ledger cells: the toolbar button,
                // slider, Save button and the per-row Reimbursable checkbox.
                (None, Some(i)) => format!("<non-cell widget {i:?}>"),
                (None, None) => "(no focus)".to_string(),
            };
            println!("FOCUS {label}");
            self.shared.send(Ev::Focus { from, to: key });
        }
    }

    /// Returns true if the key was consumed here and must not reach masonry.
    fn intercept(&mut self, key: &WKey, pressed: bool) -> bool {
        if !pressed {
            return false;
        }
        let (_, cell) = self.focused_cell();
        let cmd = self.mods.super_key() || self.mods.control_key();
        let shift = self.mods.shift_key();
        match key {
            WKey::Named(WNamed::ArrowUp | WNamed::ArrowDown) if !cmd => {
                let Some(k) = cell else { return false };
                let numeric = matches!(k, CellKey::Cell(_, f) if f.numeric()) || k == CellKey::Vat;
                if !numeric {
                    return false;
                }
                self.shared.send(Ev::Step {
                    key: k,
                    up: matches!(key, WKey::Named(WNamed::ArrowUp)),
                    big: shift,
                });
                true
            }
            WKey::Named(WNamed::Escape) => {
                if let Some(k) = cell {
                    self.shared.send(Ev::Revert(k));
                    return true;
                }
                false
            }
            WKey::Character(c) if cmd && c.eq_ignore_ascii_case("z") => {
                self.shared
                    .send(if shift { Ev::Redo } else { Ev::Undo });
                true
            }
            // Row TSV copy/paste. Cmd+C/Cmd+V are already claimed: masonry's
            // TextArea implements Cmd+C on the text selection and masonry_winit
            // itself turns Cmd+V into TextEvent::ClipboardPaste before the
            // widget sees it, so the row-level chords are Cmd+Shift+C/V.
            WKey::Character(c) if cmd && shift && c.eq_ignore_ascii_case("c") => {
                self.shared.send(Ev::CopyRow);
                true
            }
            WKey::Character(c) if cmd && shift && c.eq_ignore_ascii_case("v") => {
                let text = arboard::Clipboard::new()
                    .and_then(|mut c| c.get_text())
                    .unwrap_or_default();
                self.shared.send(Ev::PasteRow(text));
                true
            }
            _ => false,
        }
    }
}

impl ApplicationHandler<MasonryUserEvent> for ShellApp {
    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: xilem::winit::event::StartCause) {
        self.masonry_state.handle_new_events(event_loop, cause);
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.masonry_state
            .handle_resumed(event_loop, &mut self.driver);
        if !self.driver.booted {
            self.driver.booted = true;
        }
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        self.masonry_state.handle_suspended(event_loop);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: xilem::winit::window::WindowId,
        event: WindowEvent,
    ) {
        if let WindowEvent::ModifiersChanged(m) = &event {
            self.mods = m.state();
        }
        if let WindowEvent::KeyboardInput { event: ke, .. } = &event {
            let pressed = ke.state == ElementState::Pressed;
            if self.intercept(&ke.logical_key, pressed) {
                // Consumed by the shell layer; masonry never sees it.
                return;
            }
        }
        self.masonry_state
            .handle_window_event(event_loop, window_id, event, &mut self.driver);
        self.poll_focus();
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: MasonryUserEvent) {
        self.masonry_state
            .handle_user_event(event_loop, event, &mut self.driver);
        self.poll_focus();
    }

    fn device_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        device_id: xilem::winit::event::DeviceId,
        event: xilem::winit::event::DeviceEvent,
    ) {
        self.masonry_state
            .handle_device_event(event_loop, device_id, event, &mut self.driver);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.masonry_state.handle_about_to_wait(event_loop);
    }

    fn exiting(&mut self, event_loop: &ActiveEventLoop) {
        self.masonry_state.handle_exiting(event_loop);
    }

    fn memory_warning(&mut self, event_loop: &ActiveEventLoop) {
        self.masonry_state.handle_memory_warning(event_loop);
    }
}
