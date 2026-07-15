//! Hand-rolled masonry widgets + xilem views for "Grid".
//!
//! xilem 0.4.0 has no table widget, no generic click/pointer view (button
//! draws chrome and cannot report modifier keys), and no drag primitive.
//! Two small widgets fill the gaps:
//!
//! * [`RowFrame`] — interactive row/header-cell container. Reports primary
//!   clicks together with the shift/cmd modifier state (needed for range /
//!   toggle selection; masonry's `PointerState.modifiers` carries them, but
//!   only a custom widget can read them). Paints the row background, the
//!   selection highlight + accent bar, a hover tint and a hairline bottom
//!   border.
//! * [`DragHandle`] — column-resize divider. Captures the pointer on press
//!   and reports horizontal drag deltas in window coordinates; the app
//!   updates the column width in state and the view rebuild relayouts the
//!   header and every loaded row.
//!
//! The `View` plumbing follows the pattern established in xilem-dash /
//! xilem-board (build/rebuild/teardown/message + ViewId routing).

use xilem::core::{MessageContext, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker};
use xilem::masonry::accesskit::{Node, Role};
use xilem::masonry::core::{
    AccessCtx, BoxConstraints, ChildrenIds, EventCtx, LayoutCtx, PaintCtx, PointerButton,
    PointerButtonEvent, PointerEvent, PointerUpdate, PropertiesMut, PropertiesRef, RegisterCtx,
    Update, UpdateCtx, Widget, WidgetMut, WidgetPod,
};
use xilem::masonry::kurbo::{Point, Rect, Size};
use xilem::masonry::peniko::Color;
use xilem::masonry::util::fill_color;
use xilem::masonry::vello::Scene;
use xilem::{Pod, ViewCtx, WidgetView};

// --- palette (dark theme to match masonry defaults) ---
pub const BG_EVEN: Color = Color::from_rgb8(0x1e, 0x21, 0x27);
pub const BG_ODD: Color = Color::from_rgb8(0x23, 0x27, 0x2e);
pub const HEADER_BG: Color = Color::from_rgb8(0x2b, 0x2f, 0x38);
pub const ACCENT: Color = Color::from_rgb8(0x53, 0xa8, 0xf4);
pub const GRID_LINE: Color = Color::from_rgb8(0x3a, 0x3f, 0x4a);
pub const TEXT_DIM: Color = Color::from_rgb8(0xa0, 0xa8, 0xb8);
pub const CHIP_OK: Color = Color::from_rgb8(0x2e, 0x6b, 0x40);
pub const CHIP_WARN: Color = Color::from_rgb8(0x8a, 0x6d, 0x1f);
pub const CHIP_ERR: Color = Color::from_rgb8(0x8a, 0x2f, 0x33);

// ===========================================================================
// RowFrame
// ===========================================================================

/// Events reported by a [`RowFrame`] to app state.
#[derive(Debug)]
pub enum RowEvent {
    Clicked { shift: bool, cmd: bool },
}

pub struct RowFrame {
    child: WidgetPod<dyn Widget>,
    bg: Color,
    selected: bool,
    header: bool,
    hovered: bool,
    vpad: f64,
}

impl RowFrame {
    pub fn set_bg(this: &mut WidgetMut<'_, Self>, bg: Color) {
        if this.widget.bg != bg {
            this.widget.bg = bg;
            this.ctx.request_paint_only();
        }
    }

    pub fn set_selected(this: &mut WidgetMut<'_, Self>, selected: bool) {
        if this.widget.selected != selected {
            this.widget.selected = selected;
            this.ctx.request_paint_only();
        }
    }

    pub fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}

impl Widget for RowFrame {
    type Action = RowEvent;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        if let PointerEvent::Down(PointerButtonEvent {
            button: Some(PointerButton::Primary),
            state,
            ..
        }) = event
        {
            ctx.set_handled();
            ctx.submit_action::<RowEvent>(RowEvent::Clicked {
                shift: state.modifiers.shift(),
                cmd: state.modifiers.meta() || state.modifiers.ctrl(),
            });
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        if let Update::HoveredChanged(hovered) = event {
            self.hovered = *hovered;
            ctx.request_paint_only();
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
        let child_bc = bc.loosen().shrink((0.0, 2.0 * self.vpad));
        let child_size = ctx.run_layout(&mut self.child, &child_bc);
        let w = if bc.max().width.is_finite() {
            bc.max().width.max(child_size.width)
        } else {
            child_size.width
        };
        let size = bc.constrain(Size::new(w, child_size.height + 2.0 * self.vpad));
        ctx.place_child(&mut self.child, Point::new(0.0, self.vpad));
        size
    }

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, scene: &mut Scene) {
        let size = ctx.size();
        let r = size.to_rect();
        fill_color(scene, &r, self.bg);
        if self.selected {
            fill_color(scene, &r, ACCENT.multiply_alpha(0.20));
            fill_color(scene, &Rect::new(0.0, 0.0, 3.0, size.height), ACCENT);
        } else if self.hovered {
            fill_color(scene, &r, Color::WHITE.multiply_alpha(0.05));
        }
        let line = if self.header {
            Rect::new(0.0, size.height - 2.0, size.width, size.height)
        } else {
            Rect::new(0.0, size.height - 1.0, size.width, size.height)
        };
        fill_color(scene, &line, GRID_LINE.multiply_alpha(if self.header { 1.0 } else { 0.6 }));
    }

    fn accessibility_role(&self) -> Role {
        if self.header {
            Role::Button
        } else {
            Role::Row
        }
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

// --- xilem view for RowFrame ---

type RowCallback<State> = Box<dyn Fn(&mut State, RowEvent) + Send + Sync + 'static>;

/// Interactive row container view. `on_event` receives [`RowEvent`]s.
pub fn row_frame<State, V>(
    child: V,
    on_event: impl Fn(&mut State, RowEvent) + Send + Sync + 'static,
) -> RowFrameView<State, V>
where
    State: 'static,
    V: WidgetView<State>,
{
    RowFrameView {
        child,
        callback: Box::new(on_event),
        bg: BG_EVEN,
        selected: false,
        header: false,
        vpad: 4.0,
    }
}

#[must_use = "View values do nothing unless provided to Xilem."]
pub struct RowFrameView<State, V> {
    child: V,
    callback: RowCallback<State>,
    bg: Color,
    selected: bool,
    header: bool,
    vpad: f64,
}

impl<State, V> RowFrameView<State, V> {
    pub fn bg(mut self, bg: Color) -> Self {
        self.bg = bg;
        self
    }
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
    pub fn header(mut self) -> Self {
        self.header = true;
        self
    }
    pub fn vpad(mut self, vpad: f64) -> Self {
        self.vpad = vpad;
        self
    }
}

const ROW_CHILD_ID: ViewId = ViewId::new(0);

impl<State, V> ViewMarker for RowFrameView<State, V> {}
impl<State, V> View<State, (), ViewCtx> for RowFrameView<State, V>
where
    State: 'static,
    V: WidgetView<State>,
{
    type Element = Pod<RowFrame>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let (child, child_state) = ctx.with_id(ROW_CHILD_ID, |ctx| {
            View::<State, (), _>::build(&self.child, ctx, app_state)
        });
        let element = ctx.with_action_widget(|ctx| {
            ctx.create_pod(RowFrame {
                child: child.new_widget.erased().to_pod(),
                bg: self.bg,
                selected: self.selected,
                header: self.header,
                hovered: false,
                vpad: self.vpad,
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
        if prev.bg != self.bg {
            RowFrame::set_bg(&mut element, self.bg);
        }
        if prev.selected != self.selected {
            RowFrame::set_selected(&mut element, self.selected);
        }
        ctx.with_id(ROW_CHILD_ID, |ctx| {
            let mut child = RowFrame::child_mut(&mut element);
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
        ctx.with_id(ROW_CHILD_ID, |ctx| {
            let mut child = RowFrame::child_mut(&mut element);
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
            Some(ROW_CHILD_ID) => {
                let mut child = RowFrame::child_mut(&mut element);
                self.child
                    .message(view_state, message, child.downcast(), app_state)
            }
            None => match message.take_message::<RowEvent>() {
                Some(event) => {
                    (self.callback)(app_state, *event);
                    MessageResult::Action(())
                }
                None => MessageResult::Stale,
            },
            _ => MessageResult::Stale,
        }
    }
}

// ===========================================================================
// DragHandle
// ===========================================================================

/// Events reported by a [`DragHandle`] to app state.
#[derive(Debug)]
pub enum HandleEvent {
    Pressed,
    /// Total horizontal delta (logical px, window space) since the press.
    Dragged { dx: f64 },
    Released,
}

pub struct DragHandle {
    /// Window-space x position at press time, while the pointer is captured.
    press_x: Option<f64>,
    hovered: bool,
    width: f64,
    height: f64,
}

impl Widget for DragHandle {
    type Action = HandleEvent;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                state,
                ..
            }) => {
                let local = ctx.local_position(state.position);
                let window = ctx.to_window(local);
                self.press_x = Some(window.x);
                ctx.capture_pointer();
                ctx.set_handled();
                ctx.submit_action::<HandleEvent>(HandleEvent::Pressed);
            }
            PointerEvent::Move(PointerUpdate { current, .. }) => {
                if ctx.is_active() {
                    if let Some(press_x) = self.press_x {
                        let local = ctx.local_position(current.position);
                        let window = ctx.to_window(local);
                        ctx.submit_action::<HandleEvent>(HandleEvent::Dragged {
                            dx: window.x - press_x,
                        });
                    }
                }
            }
            PointerEvent::Up(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                ..
            }) => {
                if ctx.is_active() {
                    ctx.release_pointer();
                    self.press_x = None;
                    ctx.submit_action::<HandleEvent>(HandleEvent::Released);
                }
            }
            PointerEvent::Cancel(_) => {
                if self.press_x.take().is_some() {
                    ctx.submit_action::<HandleEvent>(HandleEvent::Released);
                }
            }
            _ => {}
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            Update::HoveredChanged(hovered) => {
                self.hovered = *hovered;
                ctx.request_paint_only();
            }
            Update::ActiveChanged(_) => {
                ctx.request_paint_only();
            }
            _ => {}
        }
    }

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn layout(
        &mut self,
        _ctx: &mut LayoutCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        bc: &BoxConstraints,
    ) -> Size {
        bc.constrain(Size::new(self.width, self.height))
    }

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, scene: &mut Scene) {
        let size = ctx.size();
        let cx = size.width / 2.0;
        let color = if ctx.is_active() {
            ACCENT
        } else if self.hovered {
            TEXT_DIM
        } else {
            GRID_LINE
        };
        let w = if ctx.is_active() || self.hovered { 1.5 } else { 0.75 };
        fill_color(
            scene,
            &Rect::new(cx - w, 3.0, cx + w, size.height - 3.0),
            color,
        );
    }

    fn accessibility_role(&self) -> Role {
        Role::Splitter
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[])
    }
}

// --- xilem view for DragHandle ---

type HandleCallback<State> = Box<dyn Fn(&mut State, HandleEvent) + Send + Sync + 'static>;

/// Column-resize drag handle view. `on_event` receives [`HandleEvent`]s.
pub fn drag_handle<State>(
    width: f64,
    height: f64,
    on_event: impl Fn(&mut State, HandleEvent) + Send + Sync + 'static,
) -> DragHandleView<State>
where
    State: 'static,
{
    DragHandleView {
        callback: Box::new(on_event),
        width,
        height,
    }
}

#[must_use = "View values do nothing unless provided to Xilem."]
pub struct DragHandleView<State> {
    callback: HandleCallback<State>,
    width: f64,
    height: f64,
}

impl<State> ViewMarker for DragHandleView<State> {}
impl<State> View<State, (), ViewCtx> for DragHandleView<State>
where
    State: 'static,
{
    type Element = Pod<DragHandle>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let element = ctx.with_action_widget(|ctx| {
            ctx.create_pod(DragHandle {
                press_x: None,
                hovered: false,
                width: self.width,
                height: self.height,
            })
        });
        (element, ())
    }

    fn rebuild(
        &self,
        _prev: &Self,
        _view_state: &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        _element: Mut<'_, Self::Element>,
        _app_state: &mut State,
    ) {
    }

    fn teardown(
        &self,
        _view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
    ) {
        ctx.teardown_leaf(element);
    }

    fn message(
        &self,
        _view_state: &mut Self::ViewState,
        message: &mut MessageContext,
        _element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<()> {
        match message.take_first() {
            None => match message.take_message::<HandleEvent>() {
                Some(event) => {
                    (self.callback)(app_state, *event);
                    MessageResult::Action(())
                }
                None => MessageResult::Stale,
            },
            _ => MessageResult::Stale,
        }
    }
}
