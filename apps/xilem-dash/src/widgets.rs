//! Hand-rolled masonry widgets + xilem views for "Pulse".
//!
//! xilem 0.4.0 has no chart, sparkline, drag-and-drop, tooltip or generic
//! pointer-event view, so all of those are implemented here directly against
//! masonry's `Widget` trait and wrapped in custom xilem `View`s.

use std::sync::Arc;

use xilem::core::{MessageContext, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker};
use xilem::masonry::accesskit::{Node, Role};
use xilem::masonry::core::{
    keyboard::{Key, NamedKey},
    render_text, AccessCtx, BoxConstraints, BrushIndex, ChildrenIds, ComposeCtx, EventCtx,
    LayoutCtx, NoAction, PaintCtx, PointerButton, PointerButtonEvent, PointerEvent,
    PointerUpdate, PropertiesMut, PropertiesRef, RegisterCtx, StyleProperty, TextEvent, Update,
    UpdateCtx, Widget, WidgetMut, WidgetPod,
};
use xilem::masonry::kurbo::{
    Affine, BezPath, Circle, Line, Point, Rect, RoundedRect, Size, Vec2,
};
use xilem::masonry::peniko::{Brush, Color};
use xilem::masonry::util::{fill_color, stroke};
use xilem::masonry::vello::Scene;
use xilem::{Pod, ViewCtx, WidgetView};

// --- palette (dark theme to match masonry defaults) ---
pub const CARD_BG: Color = Color::from_rgb8(0x2b, 0x2f, 0x38);
pub const CARD_BG_HOVER: Color = Color::from_rgb8(0x35, 0x3b, 0x47);
pub const ACCENT: Color = Color::from_rgb8(0x53, 0xa8, 0xf4);
pub const ACCENT_DIM: Color = Color::from_rgb8(0x2f, 0x5d, 0x84);
pub const CHART_BG: Color = Color::from_rgb8(0x22, 0x25, 0x2c);
pub const GRID_LINE: Color = Color::from_rgb8(0x3a, 0x3f, 0x4a);
pub const TEXT_DIM: Color = Color::from_rgb8(0xa0, 0xa8, 0xb8);

/// Build a one-line parley text layout and paint it at `origin`.
/// (masonry has no "draw text into a Scene" convenience; this is the minimal
/// version of what the Label widget does internally.)
pub fn draw_text(
    ctx: &mut PaintCtx<'_>,
    scene: &mut Scene,
    text: &str,
    size: f32,
    color: Color,
    origin: Point,
) -> (f64, f64) {
    let (font_ctx, layout_ctx) = ctx.text_contexts();
    let mut builder = layout_ctx.ranged_builder(font_ctx, text, 1.0, true);
    builder.push_default(StyleProperty::FontSize(size));
    let mut layout: xilem::masonry::parley::Layout<BrushIndex> = Default::default();
    builder.build_into(&mut layout, text);
    layout.break_all_lines(None);
    render_text(
        scene,
        Affine::translate(origin.to_vec2()),
        &layout,
        &[Brush::Solid(color)],
        true,
    );
    (layout.width() as f64, layout.height() as f64)
}

// ===========================================================================
// Sparkline: paint-only leaf widget
// ===========================================================================

pub struct Sparkline {
    samples: Vec<f32>,
}

impl Sparkline {
    fn path(&self, size: Size) -> Option<(BezPath, BezPath)> {
        if self.samples.len() < 2 {
            return None;
        }
        let (min, max) = min_max(&self.samples);
        let span = (max - min).max(1e-6);
        let w = size.width;
        let h = size.height;
        let step = w / (self.samples.len() - 1) as f64;
        let mut line = BezPath::new();
        let mut fill = BezPath::new();
        fill.move_to((0.0, h));
        for (i, s) in self.samples.iter().enumerate() {
            let x = i as f64 * step;
            let y = h - ((s - min) / span) as f64 * (h - 2.0) - 1.0;
            if i == 0 {
                line.move_to((x, y));
            } else {
                line.line_to((x, y));
            }
            fill.line_to((x, y));
        }
        fill.line_to((w, h));
        fill.close_path();
        Some((line, fill))
    }
}

impl Widget for Sparkline {
    type Action = NoAction;

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn layout(
        &mut self,
        _ctx: &mut LayoutCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        bc: &BoxConstraints,
    ) -> Size {
        let w = if bc.max().width.is_finite() {
            bc.max().width
        } else {
            140.0
        };
        bc.constrain(Size::new(w, 36.0))
    }

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, scene: &mut Scene) {
        if let Some((line, fill)) = self.path(ctx.size()) {
            fill_color(scene, &fill, ACCENT.multiply_alpha(0.15));
            stroke(scene, &line, ACCENT, 1.5);
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::Image
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }

    fn accepts_pointer_interaction(&self) -> bool {
        false
    }
}

fn min_max(samples: &[f32]) -> (f32, f32) {
    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;
    for &s in samples {
        min = min.min(s);
        max = max.max(s);
    }
    (min, max)
}

/// xilem view for [`Sparkline`].
pub fn sparkline(samples: Vec<f32>) -> SparklineView {
    SparklineView { samples }
}

pub struct SparklineView {
    samples: Vec<f32>,
}

impl ViewMarker for SparklineView {}
impl<State: 'static, Action: 'static> View<State, Action, ViewCtx> for SparklineView {
    type Element = Pod<Sparkline>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut State) -> (Self::Element, Self::ViewState) {
        (
            ctx.create_pod(Sparkline {
                samples: self.samples.clone(),
            }),
            (),
        )
    }

    fn rebuild(
        &self,
        prev: &Self,
        (): &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _: &mut State,
    ) {
        if prev.samples != self.samples {
            element.widget.samples = self.samples.clone();
            element.ctx.request_paint_only();
        }
    }

    fn teardown(&self, (): &mut Self::ViewState, ctx: &mut ViewCtx, element: Mut<'_, Self::Element>) {
        ctx.teardown_leaf(element);
    }

    fn message(
        &self,
        (): &mut Self::ViewState,
        _message: &mut MessageContext,
        _element: Mut<'_, Self::Element>,
        _state: &mut State,
    ) -> MessageResult<Action> {
        MessageResult::Stale
    }
}

// ===========================================================================
// Chart: main line chart with hover crosshair + tooltip (widget-local hover)
// ===========================================================================

pub const CHART_CAPACITY: usize = 300;

pub struct Chart {
    samples: Vec<f32>,
    unit: &'static str,
    /// Hover position in local coords. Kept widget-local: the crosshair does
    /// not need to round-trip through app state.
    hover: Option<Point>,
}

impl Widget for Chart {
    type Action = NoAction;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Move(PointerUpdate { current, .. }) => {
                self.hover = Some(ctx.local_position(current.position));
                ctx.request_paint_only();
            }
            PointerEvent::Leave(_) | PointerEvent::Cancel(_) => {
                self.hover = None;
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
        let max = bc.max();
        let w = if max.width.is_finite() { max.width } else { 640.0 };
        let h = if max.height.is_finite() { max.height } else { 240.0 };
        bc.constrain(Size::new(w, h))
    }

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, scene: &mut Scene) {
        let size = ctx.size();
        let bg = RoundedRect::from_rect(size.to_rect(), 6.0);
        fill_color(scene, &bg, CHART_BG);

        // horizontal grid lines
        for i in 1..4 {
            let y = size.height * i as f64 / 4.0;
            stroke(scene, &Line::new((0.0, y), (size.width, y)), GRID_LINE, 1.0);
        }

        if self.samples.len() < 2 {
            draw_text(ctx, scene, "waiting for data…", 13.0, TEXT_DIM, Point::new(12.0, 12.0));
            return;
        }

        let (min, max) = min_max(&self.samples);
        let span = (max - min).max(1e-6);
        let inset = 8.0;
        let h = size.height - 2.0 * inset;
        let step = size.width / (CHART_CAPACITY - 1) as f64;
        let n = self.samples.len();
        // Right-anchored: newest sample at the right edge, scrolls leftwards.
        let x_of = |i: usize| size.width - (n - 1 - i) as f64 * step;
        let y_of = |s: f32| inset + (1.0 - ((s - min) / span) as f64) * h;

        let mut path = BezPath::new();
        for (i, &s) in self.samples.iter().enumerate() {
            let p = (x_of(i), y_of(s));
            if i == 0 {
                path.move_to(p);
            } else {
                path.line_to(p);
            }
        }
        stroke(scene, &path, ACCENT, 2.0);

        // min/max labels
        draw_text(ctx, scene, &format!("{max:.1}{}", self.unit), 11.0, TEXT_DIM, Point::new(6.0, 4.0));
        draw_text(
            ctx,
            scene,
            &format!("{min:.1}{}", self.unit),
            11.0,
            TEXT_DIM,
            Point::new(6.0, size.height - 18.0),
        );

        // hover crosshair + tooltip
        if let Some(hover) = self.hover {
            // nearest sample index to the cursor x
            let rel = ((size.width - hover.x) / step).round() as isize;
            let idx = n as isize - 1 - rel;
            if idx >= 0 && (idx as usize) < n {
                let idx = idx as usize;
                let sx = x_of(idx);
                let sy = y_of(self.samples[idx]);
                stroke(scene, &Line::new((sx, 0.0), (sx, size.height)), TEXT_DIM, 1.0);
                stroke(scene, &Line::new((0.0, sy), (size.width, sy)), GRID_LINE, 1.0);
                fill_color(scene, &Circle::new((sx, sy), 4.0), ACCENT);

                let text = format!("#{idx}  {:.2}{}", self.samples[idx], self.unit);
                // draw once offscreen? No — measure via a throwaway layout by
                // drawing after computing box from a rough estimate is ugly;
                // instead draw text after the box using measured extents:
                // first compute extents with a dry run at an offscreen point.
                let (tw, th) = draw_text(ctx, scene, &text, 12.0, Color::TRANSPARENT, Point::new(-1000.0, -1000.0));
                let pad = 6.0;
                let mut bx = sx + 12.0;
                let mut by = sy - th - 2.0 * pad - 8.0;
                if bx + tw + 2.0 * pad > size.width {
                    bx = sx - tw - 2.0 * pad - 12.0;
                }
                by = by.clamp(0.0, (size.height - th - 2.0 * pad).max(0.0));
                let tooltip = RoundedRect::new(bx, by, bx + tw + 2.0 * pad, by + th + 2.0 * pad, 4.0);
                fill_color(scene, &tooltip, Color::from_rgb8(0x10, 0x12, 0x16).multiply_alpha(0.92));
                stroke(scene, &tooltip, ACCENT_DIM, 1.0);
                draw_text(ctx, scene, &text, 12.0, Color::WHITE, Point::new(bx + pad, by + pad));
            }
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::Image
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
}

/// xilem view for [`Chart`].
pub fn chart(samples: Vec<f32>, unit: &'static str) -> ChartView {
    ChartView { samples, unit }
}

pub struct ChartView {
    samples: Vec<f32>,
    unit: &'static str,
}

impl ViewMarker for ChartView {}
impl<State: 'static, Action: 'static> View<State, Action, ViewCtx> for ChartView {
    type Element = Pod<Chart>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut State) -> (Self::Element, Self::ViewState) {
        (
            ctx.create_pod(Chart {
                samples: self.samples.clone(),
                unit: self.unit,
                hover: None,
            }),
            (),
        )
    }

    fn rebuild(
        &self,
        prev: &Self,
        (): &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _: &mut State,
    ) {
        if prev.samples != self.samples || prev.unit != self.unit {
            element.widget.samples = self.samples.clone();
            element.widget.unit = self.unit;
            element.ctx.request_paint_only();
        }
    }

    fn teardown(&self, (): &mut Self::ViewState, ctx: &mut ViewCtx, element: Mut<'_, Self::Element>) {
        ctx.teardown_leaf(element);
    }

    fn message(
        &self,
        (): &mut Self::ViewState,
        _message: &mut MessageContext,
        _element: Mut<'_, Self::Element>,
        _state: &mut State,
    ) -> MessageResult<Action> {
        MessageResult::Stale
    }
}

// ===========================================================================
// CardFrame: interactive container — click / double-click / drag / hover
// elevation animation / Escape catching / window-rect reporting.
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

pub type GeomRegistry = Arc<std::sync::Mutex<std::collections::HashMap<u64, Rect>>>;

pub struct CardFrame {
    child: WidgetPod<dyn Widget>,
    interactive: bool,
    selected: bool,
    drop_hint: bool,
    plain: bool,
    fill_width: bool,
    fill_height: bool,
    corner: f64,
    padding: f64,
    /// press-local + press-window positions while the primary button is down.
    press: Option<(Point, Point)>,
    last_window: Point,
    dragging: bool,
    /// 0..1 hover elevation animation progress.
    hover_t: f64,
    /// 1..0 "just dropped" flash animation.
    flash_t: f64,
    /// Report this widget's window rect into a shared registry (used by the
    /// kanban board for drop-target hit testing; unused in the dashboard).
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
                    self.last_window = window;
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
                        self.last_window = window;
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
        // exponential ease towards target elevation
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
        // center child horizontally, top-align vertically
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
        if self.hover_t > 0.01 {
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
    plain: bool,
    fill_width: bool,
    fill_height: bool,
    padding: f64,
    flash_seq: u64,
    geom: Option<(u64, GeomRegistry)>,
}

impl<State, V> CardFrameView<State, V> {
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
    pub fn drop_hint(mut self, hint: bool) -> Self {
        self.drop_hint = hint;
        self
    }
    /// Fill the given cell in both axes (grid cells, flex-allotted areas).
    pub fn fill(mut self) -> Self {
        self.fill_width = true;
        self.fill_height = true;
        self
    }
    /// Hug the child's size in both axes.
    pub fn hug(mut self) -> Self {
        self.fill_width = false;
        self.fill_height = false;
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
                plain: self.plain,
                fill_width: self.fill_width,
                fill_height: self.fill_height,
                corner: 8.0,
                padding: self.padding,
                press: None,
                last_window: Point::ZERO,
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
