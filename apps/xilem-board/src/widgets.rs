//! Hand-rolled masonry widgets + xilem views for "Board".
//!
//! xilem 0.4.0 has no drag-and-drop, no generic pointer-event view, no
//! double-click view, no "focus this widget" view API and no way to catch
//! Escape from a text input. All of that is implemented here directly against
//! masonry's `Widget` trait:
//!
//! * [`CardFrame`] — interactive container reporting clicks / double-clicks /
//!   drags (window coords) to app state, painting card chrome, playing a
//!   hover-elevation and a drop-flash animation, catching bubbled Escape key
//!   events, and reporting its window rect into a shared registry used for
//!   drop-target hit-testing.
//! * [`AutoFocus`] — wrapper that moves text focus to the inner
//!   `TextArea` of a freshly created `text_input` (xilem 0.4 has no
//!   autofocus API; focus can only be requested from an `EventCtx`, so this
//!   fires on the first pointer event that bubbles through after creation).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use xilem::core::{MessageContext, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker};
use xilem::masonry::accesskit::{Node, Role};
use xilem::masonry::core::{
    keyboard::{Key, NamedKey},
    AccessCtx, BoxConstraints, ChildrenIds, ComposeCtx, EventCtx, LayoutCtx, NoAction, PaintCtx,
    PointerButton, PointerButtonEvent, PointerEvent, PointerUpdate, PropertiesMut, PropertiesRef,
    RegisterCtx, TextEvent, Update, UpdateCtx, Widget, WidgetId, WidgetMut, WidgetPod,
};
use xilem::masonry::kurbo::{Affine, Point, Rect, RoundedRect, Size, Vec2};
use xilem::masonry::peniko::Color;
use xilem::masonry::util::{fill_color, stroke};
use xilem::masonry::vello::Scene;
use xilem::masonry::widgets::TextInput;
use xilem::{Pod, ViewCtx, WidgetView};

// --- palette (dark theme to match masonry defaults) ---
pub const CARD_BG: Color = Color::from_rgb8(0x2b, 0x2f, 0x38);
pub const CARD_BG_HOVER: Color = Color::from_rgb8(0x35, 0x3b, 0x47);
pub const ACCENT: Color = Color::from_rgb8(0x53, 0xa8, 0xf4);
pub const ACCENT_DIM: Color = Color::from_rgb8(0x2f, 0x5d, 0x84);
pub const COLUMN_BG: Color = Color::from_rgb8(0x22, 0x25, 0x2c);
pub const GRID_LINE: Color = Color::from_rgb8(0x3a, 0x3f, 0x4a);
pub const TEXT_DIM: Color = Color::from_rgb8(0xa0, 0xa8, 0xb8);

/// Shared widget-rect registry: widgets report their window-space rectangles
/// here from the compose pass; app state hit-tests it during drags.
pub type GeomRegistry = Arc<Mutex<HashMap<u64, Rect>>>;

// ===========================================================================
// CardFrame
// ===========================================================================

/// Events reported by a [`CardFrame`] to app state.
#[derive(Debug)]
pub enum CardEvent {
    Clicked,
    DoubleClicked,
    /// Drag started (pointer moved past a threshold while pressed).
    DragStart {
        /// Press position, window coords (logical).
        window: Point,
        /// Widget origin in window coords at drag start.
        origin: Point,
        /// Widget size at drag start.
        size: Size,
        /// Press position relative to the widget origin.
        grab: Vec2,
    },
    DragMove {
        window: Point,
    },
    DragEnd {
        window: Point,
    },
    DragCancelled,
    /// Escape pressed while a descendant (e.g. an inline edit box) had focus.
    EscapePressed,
}

pub struct CardFrame {
    child: WidgetPod<dyn Widget>,
    interactive: bool,
    selected: bool,
    drop_hint: bool,
    dimmed: bool,
    plain: bool,
    fill_width: bool,
    fill_height: bool,
    corner: f64,
    padding: f64,
    /// press-local + press-window positions while the primary button is down.
    press: Option<(Point, Point)>,
    dragging: bool,
    /// 0..1 hover elevation animation progress.
    hover_t: f64,
    /// 1..0 "just dropped" flash animation.
    flash_t: f64,
    /// Report this widget's window rect into a shared registry, keyed by an
    /// app-provided stable id.
    geom: Option<(u64, GeomRegistry)>,
}

impl CardFrame {
    pub fn set_selected(this: &mut WidgetMut<'_, Self>, selected: bool) {
        if this.widget.selected != selected {
            this.widget.selected = selected;
            this.ctx.request_paint_only();
        }
    }

    pub fn set_drop_hint(this: &mut WidgetMut<'_, Self>, hint: bool) {
        if this.widget.drop_hint != hint {
            this.widget.drop_hint = hint;
            this.ctx.request_paint_only();
        }
    }

    pub fn set_dimmed(this: &mut WidgetMut<'_, Self>, dimmed: bool) {
        if this.widget.dimmed != dimmed {
            this.widget.dimmed = dimmed;
            this.ctx.request_paint_only();
        }
    }

    pub fn flash(this: &mut WidgetMut<'_, Self>) {
        this.widget.flash_t = 1.0;
        this.ctx.request_anim_frame();
        this.ctx.request_paint_only();
    }

    pub fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}

impl Widget for CardFrame {
    type Action = CardEvent;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        if !self.interactive {
            return;
        }
        match event {
            PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                state,
                ..
            }) => {
                if state.count >= 2 {
                    // Deliberately do not capture: the Up event will then
                    // bubble through whatever the rebuild put under the
                    // cursor (used by AutoFocus for the inline editor).
                    ctx.set_handled();
                    ctx.submit_action::<CardEvent>(CardEvent::DoubleClicked);
                } else {
                    let local = ctx.local_position(state.position);
                    let window = ctx.to_window(local);
                    self.press = Some((local, window));
                    self.dragging = false;
                    ctx.capture_pointer();
                    // Stop the event here so an ancestor CardFrame cannot
                    // steal the pointer capture (masonry: last capture wins).
                    ctx.set_handled();
                }
            }
            PointerEvent::Move(PointerUpdate { current, .. }) => {
                if ctx.is_active() {
                    if let Some((press_local, press_window)) = self.press {
                        let local = ctx.local_position(current.position);
                        let window = ctx.to_window(local);
                        if !self.dragging && (window - press_window).hypot() > 5.0 {
                            self.dragging = true;
                            ctx.submit_action::<CardEvent>(CardEvent::DragStart {
                                window: press_window,
                                origin: ctx.window_origin(),
                                size: ctx.size(),
                                grab: press_local.to_vec2(),
                            });
                        }
                        if self.dragging {
                            ctx.submit_action::<CardEvent>(CardEvent::DragMove { window });
                        }
                    }
                }
            }
            PointerEvent::Up(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                state,
                ..
            }) => {
                if ctx.is_active() {
                    ctx.release_pointer();
                    let local = ctx.local_position(state.position);
                    let window = ctx.to_window(local);
                    if self.dragging {
                        ctx.submit_action::<CardEvent>(CardEvent::DragEnd { window });
                    } else if self.press.is_some() {
                        ctx.submit_action::<CardEvent>(CardEvent::Clicked);
                    }
                    self.press = None;
                    self.dragging = false;
                }
            }
            PointerEvent::Cancel(_) => {
                if self.dragging {
                    ctx.submit_action::<CardEvent>(CardEvent::DragCancelled);
                }
                self.press = None;
                self.dragging = false;
            }
            _ => {}
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        // Text events bubble from the focused descendant (the inline edit
        // input); this is the only hook we have for Escape-to-cancel.
        if let TextEvent::Keyboard(key_event) = event {
            if key_event.key == Key::Named(NamedKey::Escape) && !key_event.state.is_up() {
                ctx.set_handled();
                ctx.submit_action::<CardEvent>(CardEvent::EscapePressed);
            }
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            Update::HoveredChanged(_) | Update::ActiveChanged(_) => {
                ctx.request_anim_frame();
            }
            _ => {}
        }
    }

    fn on_anim_frame(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        interval: u64,
    ) {
        let dt = (interval as f64 * 1e-9).min(0.1);
        let target = if self.plain {
            0.0
        } else if ctx.is_hovered() || ctx.is_active() {
            1.0
        } else {
            0.0
        };
        let mut still = false;
        // exponential ease towards the target elevation
        let d = target - self.hover_t;
        if d.abs() > 0.01 {
            self.hover_t += d * (dt * 12.0).min(1.0);
            still = true;
        } else {
            self.hover_t = target;
        }
        if self.flash_t > 0.0 {
            self.flash_t = (self.flash_t - dt * 2.5).max(0.0);
            still = true;
        }
        if still {
            ctx.request_anim_frame();
        }
        ctx.request_paint_only();
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
    }

    fn layout(
        &mut self,
        ctx: &mut LayoutCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        bc: &BoxConstraints,
    ) -> Size {
        let pad = self.padding;
        let child_bc = bc.loosen().shrink((2.0 * pad, 2.0 * pad));
        let child_size = ctx.run_layout(&mut self.child, &child_bc);
        let max = bc.max();
        // NOTE: masonry's Flex hands non-flex children its own loosened max
        // in both axes (and inside a Portal that max is the viewport size),
        // so "fill whatever is finite" is not a usable heuristic — fill
        // behaviour must be chosen explicitly per use site.
        let w = if self.fill_width && max.width.is_finite() {
            max.width
        } else {
            child_size.width + 2.0 * pad
        };
        let h = if self.fill_height && max.height.is_finite() {
            max.height
        } else {
            child_size.height + 2.0 * pad
        };
        let size = bc.constrain(Size::new(w, h));
        let x = pad + ((size.width - 2.0 * pad - child_size.width) / 2.0).max(0.0);
        ctx.place_child(&mut self.child, Point::new(x, pad));
        size
    }

    fn compose(&mut self, ctx: &mut ComposeCtx<'_>) {
        if let Some((key, registry)) = &self.geom {
            let origin = ctx.window_origin();
            let size = ctx.size();
            registry
                .lock()
                .unwrap()
                .insert(*key, Rect::from_origin_size(origin, size));
        }
    }

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, scene: &mut Scene) {
        if self.plain {
            return;
        }
        let rect = ctx.size().to_rect();
        let rr = RoundedRect::from_rect(rect, self.corner);

        // animated elevation shadow
        if self.hover_t > 0.01 && !self.dimmed {
            let shadow_alpha = (0.5 * self.hover_t) as f32;
            scene.draw_blurred_rounded_rect(
                Affine::IDENTITY,
                rect + Vec2::new(0.0, 3.0 + 3.0 * self.hover_t),
                Color::BLACK.multiply_alpha(shadow_alpha),
                self.corner,
                6.0 + 6.0 * self.hover_t,
            );
        }

        let base = lerp_color(CARD_BG, CARD_BG_HOVER, self.hover_t);
        fill_color(scene, &rr, base);
        if self.flash_t > 0.0 {
            fill_color(scene, &rr, ACCENT.multiply_alpha(0.35 * self.flash_t as f32));
        }
        if self.selected {
            stroke(scene, &rr, ACCENT, 2.0);
        } else if self.drop_hint {
            stroke(scene, &rr, ACCENT_DIM, 2.0);
        } else {
            stroke(scene, &rr, GRID_LINE, 1.0);
        }
    }

    fn post_paint(&mut self, ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, scene: &mut Scene) {
        // Dim the drag-source card (its ghost is following the cursor).
        if self.dimmed {
            let rr = RoundedRect::from_rect(ctx.size().to_rect(), self.corner);
            fill_color(scene, &rr, COLUMN_BG.multiply_alpha(0.7));
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child.id()])
    }
}

fn lerp_color(a: Color, b: Color, t: f64) -> Color {
    let t = t.clamp(0.0, 1.0) as f32;
    let ac = a.components;
    let bc = b.components;
    Color::new([
        ac[0] + (bc[0] - ac[0]) * t,
        ac[1] + (bc[1] - ac[1]) * t,
        ac[2] + (bc[2] - ac[2]) * t,
        ac[3] + (bc[3] - ac[3]) * t,
    ])
}

// --- xilem view for CardFrame ---

type CardCallback<State> = Box<dyn Fn(&mut State, CardEvent) + Send + Sync + 'static>;

/// Interactive card container view. `on_event` receives [`CardEvent`]s.
pub fn card_frame<State, V>(
    child: V,
    on_event: impl Fn(&mut State, CardEvent) + Send + Sync + 'static,
) -> CardFrameView<State, V>
where
    State: 'static,
    V: WidgetView<State>,
{
    CardFrameView {
        child,
        callback: Box::new(on_event),
        interactive: true,
        selected: false,
        drop_hint: false,
        dimmed: false,
        plain: false,
        fill_width: true,
        fill_height: false,
        padding: 10.0,
        flash_seq: 0,
        geom: None,
    }
}

#[must_use = "View values do nothing unless provided to Xilem."]
pub struct CardFrameView<State, V> {
    child: V,
    callback: CardCallback<State>,
    interactive: bool,
    selected: bool,
    drop_hint: bool,
    dimmed: bool,
    plain: bool,
    fill_width: bool,
    fill_height: bool,
    padding: f64,
    flash_seq: u64,
    geom: Option<(u64, GeomRegistry)>,
}

impl<State, V> CardFrameView<State, V> {
    /// Fill the given cell in both axes (grid cells, flex-allotted areas).
    pub fn fill(mut self) -> Self {
        self.fill_width = true;
        self.fill_height = true;
        self
    }
    /// Hug the child's size in both axes (small chrome like the ✕ button).
    pub fn hug(mut self) -> Self {
        self.fill_width = false;
        self.fill_height = false;
        self
    }
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
    pub fn drop_hint(mut self, hint: bool) -> Self {
        self.drop_hint = hint;
        self
    }
    pub fn dimmed(mut self, dimmed: bool) -> Self {
        self.dimmed = dimmed;
        self
    }
    /// Draw no background/border and handle no pointer events; still catches
    /// Escape and reports geometry.
    pub fn plain(mut self) -> Self {
        self.plain = true;
        self.interactive = false;
        self
    }
    pub fn padding(mut self, padding: f64) -> Self {
        self.padding = padding;
        self
    }
    /// Any change to a non-zero value triggers the flash animation.
    pub fn flash_seq(mut self, seq: u64) -> Self {
        self.flash_seq = seq;
        self
    }
    pub fn report_geometry(mut self, key: u64, registry: GeomRegistry) -> Self {
        self.geom = Some((key, registry));
        self
    }
}

const CARD_CHILD_ID: ViewId = ViewId::new(0);

impl<State, V> ViewMarker for CardFrameView<State, V> {}
impl<State, V> View<State, (), ViewCtx> for CardFrameView<State, V>
where
    State: 'static,
    V: WidgetView<State>,
{
    type Element = Pod<CardFrame>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let (child, child_state) = ctx.with_id(CARD_CHILD_ID, |ctx| {
            View::<State, (), _>::build(&self.child, ctx, app_state)
        });
        let element = ctx.with_action_widget(|ctx| {
            ctx.create_pod(CardFrame {
                child: child.new_widget.erased().to_pod(),
                interactive: self.interactive,
                selected: self.selected,
                drop_hint: self.drop_hint,
                dimmed: self.dimmed,
                plain: self.plain,
                fill_width: self.fill_width,
                fill_height: self.fill_height,
                corner: 8.0,
                padding: self.padding,
                press: None,
                dragging: false,
                hover_t: 0.0,
                flash_t: if self.flash_seq > 0 { 1.0 } else { 0.0 },
                geom: self.geom.clone(),
            })
        });
        (element, child_state)
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) {
        if prev.selected != self.selected {
            CardFrame::set_selected(&mut element, self.selected);
        }
        if prev.drop_hint != self.drop_hint {
            CardFrame::set_drop_hint(&mut element, self.drop_hint);
        }
        if prev.dimmed != self.dimmed {
            CardFrame::set_dimmed(&mut element, self.dimmed);
        }
        if prev.flash_seq != self.flash_seq && self.flash_seq > 0 {
            CardFrame::flash(&mut element);
        }
        ctx.with_id(CARD_CHILD_ID, |ctx| {
            let mut child = CardFrame::child_mut(&mut element);
            View::<State, (), _>::rebuild(
                &self.child,
                &prev.child,
                view_state,
                ctx,
                child.downcast(),
                app_state,
            );
        });
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        ctx.with_id(CARD_CHILD_ID, |ctx| {
            let mut child = CardFrame::child_mut(&mut element);
            View::<State, (), _>::teardown(&self.child, view_state, ctx, child.downcast());
        });
        ctx.teardown_leaf(element);
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageContext,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<()> {
        match message.take_first() {
            Some(CARD_CHILD_ID) => {
                let mut child = CardFrame::child_mut(&mut element);
                self.child
                    .message(view_state, message, child.downcast(), app_state)
            }
            None => match message.take_message::<CardEvent>() {
                Some(event) => {
                    (self.callback)(app_state, *event);
                    MessageResult::Action(())
                }
                // Wrong message type; stale view.
                None => MessageResult::Stale,
            },
            // Unexpected id path; stale view.
            _ => MessageResult::Stale,
        }
    }
}

// ===========================================================================
// AutoFocus: focus a freshly created text input.
// ===========================================================================

/// Pass-through wrapper that transfers text focus to the inner `TextArea` of
/// the wrapped `text_input` view. xilem 0.4 has no focus API and masonry only
/// allows `set_focus` from an `EventCtx`, so this fires on the first pointer
/// event that bubbles through it after creation — in practice the mouse-up of
/// the double-click that revealed the editor, or the next mouse move.
pub struct AutoFocus {
    child: WidgetPod<dyn Widget>,
    target: WidgetId,
    done: bool,
}

impl Widget for AutoFocus {
    type Action = NoAction;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &PointerEvent,
    ) {
        if !self.done {
            self.done = true;
            ctx.set_focus(self.target);
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
    }

    fn layout(
        &mut self,
        ctx: &mut LayoutCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        bc: &BoxConstraints,
    ) -> Size {
        let size = ctx.run_layout(&mut self.child, bc);
        ctx.place_child(&mut self.child, Point::ZERO);
        size
    }

    fn paint(&mut self, _ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, _scene: &mut Scene) {}

    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child.id()])
    }
}

/// Wrap a `text_input` view so it grabs focus once it appears.
pub fn auto_focus<State, V>(child: V) -> AutoFocusView<V>
where
    State: 'static,
    V: WidgetView<State, Widget = TextInput>,
{
    AutoFocusView { child }
}

#[must_use = "View values do nothing unless provided to Xilem."]
pub struct AutoFocusView<V> {
    child: V,
}

const FOCUS_CHILD_ID: ViewId = ViewId::new(0);

impl<V> ViewMarker for AutoFocusView<V> {}
impl<State, V> View<State, (), ViewCtx> for AutoFocusView<V>
where
    State: 'static,
    V: WidgetView<State, Widget = TextInput>,
{
    type Element = Pod<AutoFocus>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let (child, child_state) = ctx.with_id(FOCUS_CHILD_ID, |ctx| {
            View::<State, (), _>::build(&self.child, ctx, app_state)
        });
        // The focusable widget is the TextArea *inside* the TextInput.
        let target = child.new_widget.widget.area_pod().id();
        let element = ctx.create_pod(AutoFocus {
            child: child.new_widget.erased().to_pod(),
            target,
            done: false,
        });
        (element, child_state)
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) {
        ctx.with_id(FOCUS_CHILD_ID, |ctx| {
            let mut child = element.ctx.get_mut(&mut element.widget.child);
            View::<State, (), _>::rebuild(
                &self.child,
                &prev.child,
                view_state,
                ctx,
                child.downcast(),
                app_state,
            );
        });
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        ctx.with_id(FOCUS_CHILD_ID, |ctx| {
            let mut child = element.ctx.get_mut(&mut element.widget.child);
            View::<State, (), _>::teardown(&self.child, view_state, ctx, child.downcast());
        });
        ctx.teardown_leaf(element);
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageContext,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<()> {
        match message.take_first() {
            Some(FOCUS_CHILD_ID) => {
                let mut child = element.ctx.get_mut(&mut element.widget.child);
                self.child
                    .message(view_state, message, child.downcast(), app_state)
            }
            _ => MessageResult::Stale,
        }
    }
}
