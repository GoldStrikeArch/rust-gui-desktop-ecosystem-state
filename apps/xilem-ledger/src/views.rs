//! Two small pieces of xilem `View` plumbing that 0.4 does not provide.
//!
//! * [`num_label`] — a label that can request OpenType features. xilem's stock
//!   `label` view exposes only `FontSize`/`FontWeight`/`FontStack`; parley 0.6
//!   *does* have `StyleProperty::FontFeatures`, but nothing at the view layer
//!   reaches it, so this is a ~70-line re-implementation of the label view
//!   that adds `tnum`/`lnum` (tabular + lining figures).
//! * [`cell`] — a transparent pass-through view that publishes the `WidgetId`
//!   of the `TextArea` inside a `text_input` into a shared registry. xilem 0.4
//!   has no focus API of any kind; the registry is what lets the driver focus
//!   a specific cell (Enter-moves-down) and name the focused cell in the
//!   focus log.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use xilem::core::{MessageContext, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::core::{ArcStr, StyleProperty, WidgetId};
use xilem::masonry::parley::style::{FontSettings, FontStack, GenericFamily};
use xilem::masonry::properties::Padding;
use xilem::masonry::widgets;
use xilem::{Pod, TextAlign, ViewCtx, WidgetView};

use crate::model::Field;

// --- MARK: cell registry ---

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum CellKey {
    Cell(usize, Field),
    Vat,
}

impl CellKey {
    pub fn label(self) -> String {
        match self {
            Self::Cell(r, f) => format!("row {r} / {}", f.name()),
            Self::Vat => "VAT %".to_string(),
        }
    }
}

#[derive(Default)]
pub struct Registry {
    pub by_key: HashMap<CellKey, WidgetId>,
    pub by_id: HashMap<WidgetId, CellKey>,
}

pub type Reg = Arc<Mutex<Registry>>;

// --- MARK: num_label ---

/// `"tnum" 1` = tabular figures (equal advance per digit),
/// `"lnum" 1` = lining figures (no old-style descenders).
const TABULAR: &str = "\"tnum\" 1, \"lnum\" 1";

/// A label that requests tabular figures. `mono` additionally pins the font
/// stack to the generic monospace family, which guarantees equal digit
/// advances even if the system UI face has no `tnum` table.
pub fn num_label(text: impl Into<ArcStr>) -> NumLabel {
    NumLabel {
        text: text.into(),
        size: 13.0,
        align: TextAlign::End,
        mono: false,
    }
}

#[must_use = "View values do nothing unless provided to Xilem."]
pub struct NumLabel {
    text: ArcStr,
    size: f32,
    align: TextAlign,
    mono: bool,
}

impl NumLabel {
    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }
    pub fn align(mut self, align: TextAlign) -> Self {
        self.align = align;
        self
    }
    pub fn mono(mut self, mono: bool) -> Self {
        self.mono = mono;
        self
    }

    fn styles(&self) -> [StyleProperty; 3] {
        [
            StyleProperty::FontSize(self.size),
            StyleProperty::FontFeatures(FontSettings::Source(Cow::Borrowed(TABULAR))),
            StyleProperty::FontStack(if self.mono {
                FontStack::Single(GenericFamily::Monospace.into())
            } else {
                FontStack::Single(GenericFamily::SystemUi.into())
            }),
        ]
    }
}

impl ViewMarker for NumLabel {}
impl<State, Action> View<State, Action, ViewCtx> for NumLabel {
    type Element = Pod<widgets::Label>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut State) -> (Self::Element, Self::ViewState) {
        let mut w = widgets::Label::new(self.text.clone()).with_text_alignment(self.align);
        for s in self.styles() {
            w = w.with_style(s);
        }
        let mut pod = ctx.create_pod(w);
        // masonry's default theme gives every Label 2px of horizontal padding,
        // which would show up as a gap on both sides of the decimal separator
        // once the number is split into two boxes.
        pod.new_widget.properties.insert(Padding::all(0.0));
        (pod, ())
    }

    fn rebuild(
        &self,
        prev: &Self,
        (): &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _: &mut State,
    ) {
        if prev.text != self.text {
            widgets::Label::set_text(&mut element, self.text.clone());
        }
        if prev.align != self.align {
            widgets::Label::set_text_alignment(&mut element, self.align);
        }
        if prev.size != self.size || prev.mono != self.mono {
            for s in self.styles() {
                widgets::Label::insert_style(&mut element, s);
            }
        }
    }

    fn teardown(&self, (): &mut Self::ViewState, _: &mut ViewCtx, _: Mut<'_, Self::Element>) {}

    fn message(
        &self,
        (): &mut Self::ViewState,
        message: &mut MessageContext,
        _: Mut<'_, Self::Element>,
        _: &mut State,
    ) -> MessageResult<Action> {
        let _ = message;
        MessageResult::Stale
    }
}

// --- MARK: cell ---

/// Wrap a `text_input` view so its inner `TextArea`'s `WidgetId` is published
/// into `reg`. Fully transparent: same element, same message path.
pub fn cell<State, V>(key: CellKey, reg: Reg, child: V) -> CellView<V>
where
    State: 'static,
    V: WidgetView<State, Widget = widgets::TextInput>,
{
    CellView { key, reg, child }
}

#[must_use = "View values do nothing unless provided to Xilem."]
pub struct CellView<V> {
    key: CellKey,
    reg: Reg,
    child: V,
}

impl<V> ViewMarker for CellView<V> {}
impl<State, V> View<State, (), ViewCtx> for CellView<V>
where
    State: 'static,
    V: WidgetView<State, Widget = widgets::TextInput>,
{
    type Element = V::Element;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let (element, state) = View::<State, (), _>::build(&self.child, ctx, app_state);
        // The focusable widget is the TextArea *inside* the TextInput.
        let id = element.new_widget.widget.area_pod().id();
        let mut reg = self.reg.lock().unwrap();
        reg.by_key.insert(self.key, id);
        reg.by_id.insert(id, self.key);
        (element, state)
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) {
        View::<State, (), _>::rebuild(&self.child, &prev.child, view_state, ctx, element, app_state);
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
    ) {
        View::<State, (), _>::teardown(&self.child, view_state, ctx, element);
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageContext,
        element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<()> {
        self.child.message(view_state, message, element, app_state)
    }
}
