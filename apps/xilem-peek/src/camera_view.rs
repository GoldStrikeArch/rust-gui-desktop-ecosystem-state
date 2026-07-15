//! Custom masonry widget + xilem view for the live camera preview.
//!
//! Why not the stock `image()` view? Two reasons, both texture-path relevant:
//! 1. `widgets::Image::set_image_data` calls `request_layout()` on every
//!    frame; for a fixed-size preview `request_paint_only()` is enough, so a
//!    30 fps stream does not re-run layout 30 times a second.
//! 2. Honest FPS: `paint()` here runs exactly once per window frame in which
//!    a new camera frame was actually encoded into the vello scene (masonry
//!    caches per-widget scene fragments), so counting paints counts frames
//!    *presented*, not frames captured. The counter is a shared atomic that a
//!    1 Hz probe task turns into the on-screen FPS number.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use xilem::core::{MessageContext, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::accesskit::{Node, Role};
use xilem::masonry::core::{
    AccessCtx, BoxConstraints, ChildrenIds, LayoutCtx, NoAction, PaintCtx, PropertiesMut,
    PropertiesRef, RegisterCtx, Widget,
};
use xilem::masonry::kurbo::{Affine, Size};
use xilem::masonry::peniko::{Color, ImageBrush, ImageData};
use xilem::masonry::util::fill_color;
use xilem::masonry::vello::Scene;
use xilem::{Pod, ViewCtx};

const PREVIEW_BG: Color = Color::from_rgb8(0x14, 0x16, 0x1a);

// --- MARK: WIDGET

pub struct CameraPreview {
    frame: Option<ImageBrush>,
    presented: Arc<AtomicU64>,
}

impl Widget for CameraPreview {
    type Action = NoAction;

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn layout(
        &mut self,
        _ctx: &mut LayoutCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        bc: &BoxConstraints,
    ) -> Size {
        // The app wraps this in a fixed sized_box; be sane if it doesn't.
        let w = if bc.max().width.is_finite() {
            bc.max().width
        } else {
            480.0
        };
        bc.constrain(Size::new(w, w * 0.75))
    }

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, scene: &mut Scene) {
        let size = ctx.size();
        fill_color(scene, &size.to_rect(), PREVIEW_BG);
        if let Some(brush) = &self.frame {
            let (iw, ih) = (brush.image.width as f64, brush.image.height as f64);
            if iw > 0.0 && ih > 0.0 {
                // Uniform "contain" fit, centered.
                let scale = (size.width / iw).min(size.height / ih);
                let tx = (size.width - iw * scale) * 0.5;
                let ty = (size.height - ih * scale) * 0.5;
                scene.draw_image(brush, Affine::translate((tx, ty)) * Affine::scale(scale));
                // One paint == one re-encode of this fragment == one window
                // frame in which this camera frame is presented.
                self.presented.fetch_add(1, Ordering::Relaxed);
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

    fn accepts_pointer_interaction(&self) -> bool {
        false
    }
}

// --- MARK: VIEW

/// Live camera preview view. `frame` is the latest decoded RGBA frame;
/// `presented` is incremented on every paint of a frame.
pub fn camera_preview(frame: Option<ImageData>, presented: Arc<AtomicU64>) -> CameraPreviewView {
    CameraPreviewView { frame, presented }
}

pub struct CameraPreviewView {
    frame: Option<ImageData>,
    presented: Arc<AtomicU64>,
}

impl ViewMarker for CameraPreviewView {}
impl<State: 'static, Action: 'static> View<State, Action, ViewCtx> for CameraPreviewView {
    type Element = Pod<CameraPreview>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut State) -> (Self::Element, Self::ViewState) {
        (
            ctx.create_pod(CameraPreview {
                frame: self.frame.clone().map(ImageBrush::new),
                presented: self.presented.clone(),
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
        // ImageData PartialEq compares Blob ids (cheap) — every camera frame
        // is a fresh Blob, so this is the per-frame hot path.
        if prev.frame != self.frame {
            let same_geometry = match (&prev.frame, &self.frame) {
                (Some(a), Some(b)) => a.width == b.width && a.height == b.height,
                _ => false,
            };
            element.widget.frame = self.frame.clone().map(ImageBrush::new);
            if same_geometry {
                element.ctx.request_paint_only();
            } else {
                element.ctx.request_layout();
            }
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
