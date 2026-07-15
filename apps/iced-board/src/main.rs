//! "Board" — kanban task board (SPEC-3), iced 0.14.
//!
//! Drag-and-drop is hand-rolled from core primitives (iced 0.14 has no
//! built-in general cross-container card-reorder widget; the third-party
//! `iced_drop` now supports 0.14 but was not used in this experiment):
//! - `mouse_area` per card: `on_press` arms a drag; while a drag is active it
//!   instead reports `on_move` (local y → insert above/below this card).
//! - A `mouse_area` around each column and a fixed "tail zone" after the last
//!   card catch drops on empty space (insert at end).
//! - A global `event::listen_with` subscription — alive only while dragging —
//!   tracks the cursor (ghost position) and the button release (drop).
//! - While active, the dragged card is *removed from the model* and lives in
//!   the `Drag` struct; the insertion indicator is a slim accent bar injected
//!   into the target column at the target index; releasing inserts the card
//!   at the target (which starts as the source position, so a drop outside
//!   any target puts the card back).
//! - The ghost is a `pin` layered over the UI in a root `stack`, following
//!   the cursor. It contains no interactive widgets, so events pass through.
//!
//! Inline edit: `mouse_area::on_double_click` (built-in double-click
//! detection) swaps the card for a `text_input`; Enter commits via
//! `on_submit`; Esc is caught by `event::listen_with` because `text_input`
//! captures the Escape key (so `keyboard::listen()` would never see it).
//!
//! Drop animation: iced 0.14's built-in `Animation` (scale + shadow via
//! `float`), with `window::frames()` subscribed only while animating.

use iced::event;
use iced::keyboard;
use iced::mouse;
use iced::time::Instant;
use iced::widget::{
    button, column, container, float, mouse_area, operation, pin, row,
    scrollable, space, stack, text, text_input,
};
use iced::window;
use iced::{
    Animation, Border, Center, Color, Element, Event, Fill, Point, Shadow,
    Subscription, Task, Theme, Vector,
};

const CARD_HEIGHT: f32 = 56.0;
const DRAG_THRESHOLD: f32 = 8.0;

pub fn main() -> iced::Result {
    iced::application::timed(
        Board::new,
        Board::update,
        Board::subscription,
        Board::view,
    )
    .title(|_: &Board| String::from("Board (iced)"))
    .window_size((900.0, 600.0))
    .run()
}

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

struct Card {
    id: u64,
    text: String,
}

struct Lane {
    name: &'static str,
    cards: Vec<Card>,
}

/// An in-flight drag. `target` is (column, insertion index) into the model
/// *without* the dragged card (it is removed once the drag activates).
struct Drag {
    card: Card,
    source: (usize, usize),
    target: (usize, usize),
    origin: Option<Point>,
    cursor: Point,
    active: bool,
}

struct Board {
    lanes: Vec<Lane>,
    next_id: u64,
    drag: Option<Drag>,
    /// Column index of the open "+ Add card" input, and its text.
    adding: Option<(usize, String)>,
    /// Card id being edited in place, and the edit buffer.
    editing: Option<(u64, String)>,
    /// Scale/shadow pop on the card that just landed.
    drop_anim: Option<(u64, Animation<bool>)>,
    now: Instant,
}

#[derive(Debug, Clone)]
enum Message {
    // Drag & drop
    CardPressed(usize, usize),
    DragOverCard(usize, usize, f32),
    DragOverEnd(usize),
    PointerMoved(Point),
    PointerReleased,
    // Inline edit
    EditStart(u64),
    EditChanged(String),
    EditCommit,
    // Add / delete
    AddStart(usize),
    AddChanged(String),
    AddCommit,
    DeleteCard(u64),
    /// Esc pressed while an input is open.
    CancelInput,
    Animate,
}

impl Board {
    fn new() -> (Self, Task<Message>) {
        let lanes = vec![
            Lane {
                name: "Todo",
                cards: vec![
                    Card { id: 0, text: "Ship the drop indicator".into() },
                    Card { id: 1, text: "Write FRICTION.md".into() },
                ],
            },
            Lane {
                name: "Doing",
                cards: vec![Card {
                    id: 2,
                    text: "Hand-roll drag & drop".into(),
                }],
            },
            Lane {
                name: "Done",
                cards: vec![Card {
                    id: 3,
                    text: "Pin the ghost to the cursor".into(),
                }],
            },
        ];

        (
            Self {
                lanes,
                next_id: 4,
                drag: None,
                adding: None,
                editing: None,
                drop_anim: None,
                now: Instant::now(),
            },
            Task::none(),
        )
    }

    fn update(&mut self, message: Message, now: Instant) -> Task<Message> {
        self.now = now;

        match message {
            Message::CardPressed(lane, index) => {
                self.drag = Some(Drag {
                    card: Card {
                        id: self.lanes[lane].cards[index].id,
                        text: String::new(), // filled when the drag activates
                    },
                    source: (lane, index),
                    target: (lane, index),
                    origin: None,
                    cursor: Point::ORIGIN,
                    active: false,
                });
            }
            Message::PointerMoved(position) => {
                if let Some(drag) = &mut self.drag {
                    drag.cursor = position;

                    match drag.origin {
                        None => drag.origin = Some(position),
                        Some(origin) => {
                            if !drag.active
                                && origin.distance(position) > DRAG_THRESHOLD
                            {
                                drag.active = true;

                                // Lift the card out of the model; from now on
                                // all indices refer to the remaining cards.
                                let (lane, index) = drag.source;
                                drag.card =
                                    self.lanes[lane].cards.remove(index);
                            }
                        }
                    }
                }
            }
            Message::DragOverCard(lane, index, y) => {
                if let Some(drag) = &mut self.drag
                    && drag.active
                {
                    let after = usize::from(y > CARD_HEIGHT / 2.0);
                    drag.target = (lane, index + after);
                }
            }
            Message::DragOverEnd(lane) => {
                if let Some(drag) = &mut self.drag
                    && drag.active
                {
                    drag.target = (lane, self.lanes[lane].cards.len());
                }
            }
            Message::PointerReleased => {
                if let Some(drag) = self.drag.take()
                    && drag.active
                {
                    let (lane, index) = drag.target;
                    let index = index.min(self.lanes[lane].cards.len());
                    let id = drag.card.id;

                    self.lanes[lane].cards.insert(index, drag.card);

                    // Pop the landed card (scale + shadow settle).
                    let mut animation =
                        Animation::new(false).quick().easing(
                            iced::animation::Easing::EaseOut,
                        );
                    animation.go_mut(true, now);
                    self.drop_anim = Some((id, animation));
                }
            }
            Message::EditStart(id) => {
                let current = self
                    .lanes
                    .iter()
                    .flat_map(|lane| &lane.cards)
                    .find(|card| card.id == id)
                    .map(|card| card.text.clone())
                    .unwrap_or_default();

                self.editing = Some((id, current));
                self.adding = None;
                self.drag = None; // the double-click's press armed a drag

                return operation::focus("edit-input");
            }
            Message::EditChanged(value) => {
                if let Some((_, buffer)) = &mut self.editing {
                    *buffer = value;
                }
            }
            Message::EditCommit => {
                if let Some((id, buffer)) = self.editing.take() {
                    let value = buffer.trim();

                    if value.is_empty() {
                        // Ignore empty commits: keep the editor open.
                        self.editing = Some((id, buffer.clone()));
                    } else if let Some(card) = self
                        .lanes
                        .iter_mut()
                        .flat_map(|lane| &mut lane.cards)
                        .find(|card| card.id == id)
                    {
                        card.text = value.to_owned();
                    }
                }
            }
            Message::AddStart(lane) => {
                self.adding = Some((lane, String::new()));
                self.editing = None;

                return operation::focus("add-input");
            }
            Message::AddChanged(value) => {
                if let Some((_, buffer)) = &mut self.adding {
                    *buffer = value;
                }
            }
            Message::AddCommit => {
                if let Some((lane, buffer)) = &mut self.adding {
                    let value = buffer.trim();

                    // Empty input is ignored (input stays open).
                    if !value.is_empty() {
                        let card = Card {
                            id: self.next_id,
                            text: value.to_owned(),
                        };
                        self.next_id += 1;

                        let lane = *lane;
                        self.lanes[lane].cards.push(card);
                        self.adding = Some((lane, String::new()));

                        return operation::focus("add-input");
                    }
                }
            }
            Message::DeleteCard(id) => {
                for lane in &mut self.lanes {
                    lane.cards.retain(|card| card.id != id);
                }
            }
            Message::CancelInput => {
                self.adding = None;
                self.editing = None;
            }
            Message::Animate => {
                // `now` was refreshed above; interpolations pick it up.
            }
        }

        Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subs = Vec::new();

        // Cursor + release tracking, only while a drag is armed.
        if self.drag.is_some() {
            subs.push(event::listen_with(drag_listener));
        }

        // Esc for the inline inputs. `text_input` *captures* the Escape key,
        // so `keyboard::listen()` (ignored events only) never sees it; we
        // must use the unfiltered `event::listen_with`.
        if self.adding.is_some() || self.editing.is_some() {
            subs.push(event::listen_with(escape_listener));
        }

        // Per-frame redraws only while the drop animation runs.
        if self
            .drop_anim
            .as_ref()
            .is_some_and(|(_, animation)| animation.is_animating(self.now))
        {
            subs.push(window::frames().map(|_| Message::Animate));
        }

        Subscription::batch(subs)
    }

    fn view(&self) -> Element<'_, Message> {
        let board = row((0..self.lanes.len()).map(|lane| self.lane(lane)))
            .spacing(14)
            .padding(14);

        let mut layers = stack![container(board).width(Fill).height(Fill)];

        // Drag ghost: follows the cursor, purely visual.
        if let Some(drag) = &self.drag
            && drag.active
        {
            layers = layers.push(
                pin(container(text(&drag.card.text).size(14))
                    .padding([10, 12])
                    .width(240)
                    .height(CARD_HEIGHT)
                    .style(ghost_style))
                .x(drag.cursor.x + 10.0)
                .y(drag.cursor.y - CARD_HEIGHT / 2.0),
            );
        }

        layers.into()
    }

    fn lane(&self, index: usize) -> Element<'_, Message> {
        let lane = &self.lanes[index];
        let dragging = self.drag.as_ref().is_some_and(|drag| drag.active);
        let target = self
            .drag
            .as_ref()
            .filter(|drag| drag.active)
            .map(|drag| drag.target);

        let header = row![
            text(lane.name).size(15),
            space::horizontal(),
            text(lane.cards.len()).size(13).style(text::secondary),
        ]
        .align_y(Center);

        // Card list, with the insertion indicator injected at the target.
        let mut cards = column![].spacing(8);

        for (position, card) in lane.cards.iter().enumerate() {
            if target == Some((index, position)) {
                cards = cards.push(drop_indicator());
            }

            cards = cards.push(self.card(index, position, card, dragging));
        }

        if target == Some((index, lane.cards.len())) {
            cards = cards.push(drop_indicator());
        }

        // Fixed-height tail: catches "insert at end" when hovering the empty
        // space after the last card (and is the whole surface when empty).
        // Only wired up while a drag is active to avoid idle message spam.
        let mut tail = mouse_area(space().width(Fill).height(48));

        if dragging {
            tail = tail
                .on_enter(Message::DragOverEnd(index))
                .on_move(move |_| Message::DragOverEnd(index));
        }

        cards = cards.push(tail);

        let list = scrollable(cards.width(Fill))
            .height(Fill)
            .spacing(4);

        let footer: Element<'_, Message> = match &self.adding {
            Some((lane_index, buffer)) if *lane_index == index => {
                text_input("Describe the task…", buffer)
                    .id("add-input")
                    .on_input(Message::AddChanged)
                    .on_submit(Message::AddCommit)
                    .padding(8)
                    .size(14)
                    .into()
            }
            _ => button(text("+ Add card").size(14))
                .on_press(Message::AddStart(index))
                .style(button::text)
                .width(Fill)
                .into(),
        };

        let body = column![header, list, footer].spacing(10);

        let lane_view = container(body)
            .padding(10)
            .width(Fill)
            .height(Fill)
            .style(lane_style);

        // Column-level drop target: entering anywhere in the column defaults
        // the target to "end of this column"; per-card `on_move` then refines
        // it. Also makes empty columns valid drop targets.
        let mut area = mouse_area(lane_view);

        if dragging {
            area = area.on_enter(Message::DragOverEnd(index));
        }

        area.into()
    }

    fn card<'a>(
        &'a self,
        lane: usize,
        position: usize,
        card: &'a Card,
        dragging: bool,
    ) -> Element<'a, Message> {
        // Inline editor replaces the card body while editing.
        let body: Element<'_, Message> = match &self.editing {
            Some((id, buffer)) if *id == card.id => {
                text_input("Task", buffer)
                    .id("edit-input")
                    .on_input(Message::EditChanged)
                    .on_submit(Message::EditCommit)
                    .padding(6)
                    .size(14)
                    .into()
            }
            _ => row![
                text(&card.text).size(14).width(Fill),
                button(text("✕").size(12))
                    .on_press(Message::DeleteCard(card.id))
                    .style(button::text)
                    .padding([2, 6]),
            ]
            .spacing(6)
            .align_y(Center)
            .into(),
        };

        let boxed = container(body)
            .padding([8, 10])
            .width(Fill)
            .height(CARD_HEIGHT)
            .align_y(Center)
            .style(card_style);

        // Settle animation on the card that just landed.
        let landed = self
            .drop_anim
            .as_ref()
            .filter(|(id, _)| *id == card.id)
            .map(|(_, animation)| animation.interpolate(1.0, 0.0, self.now))
            .unwrap_or(0.0);

        let boxed = float(boxed)
            .scale(1.0 + 0.05 * landed)
            .style(move |_theme| float::Style {
                shadow: Shadow {
                    color: Color::BLACK.scale_alpha(0.35 * landed),
                    offset: Vector::new(0.0, 4.0 * landed),
                    blur_radius: 14.0 * landed,
                },
                ..float::Style::default()
            });

        let mut area = mouse_area(boxed).interaction(mouse::Interaction::Grab);

        if dragging {
            // While a drag is active, cards only report hover position so the
            // insertion point can be computed (above/below the midline).
            area = area.on_move(move |point| {
                Message::DragOverCard(lane, position, point.y)
            });
        } else {
            area = area
                .on_press(Message::CardPressed(lane, position))
                .on_double_click(Message::EditStart(card.id));
        }

        area.into()
    }
}

fn drop_indicator<'a>() -> Element<'a, Message> {
    container(space().width(Fill).height(4.0))
        .style(|theme: &Theme| container::Style {
            background: Some(
                theme.extended_palette().primary.strong.color.into(),
            ),
            border: Border::default().rounded(2.0),
            ..container::Style::default()
        })
        .into()
}

/// Global events while a drag is armed: ghost position + drop.
fn drag_listener(
    event: Event,
    _status: event::Status,
    _window: window::Id,
) -> Option<Message> {
    match event {
        Event::Mouse(mouse::Event::CursorMoved { position }) => {
            Some(Message::PointerMoved(position))
        }
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
            Some(Message::PointerReleased)
        }
        _ => None,
    }
}

/// Esc while an inline input is open (even though `text_input` captures it).
fn escape_listener(
    event: Event,
    _status: event::Status,
    _window: window::Id,
) -> Option<Message> {
    match event {
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(keyboard::key::Named::Escape),
            ..
        }) => Some(Message::CancelInput),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Styles
// ---------------------------------------------------------------------------

fn lane_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();

    container::Style {
        background: Some(palette.background.weakest.color.into()),
        border: Border {
            color: palette.background.strong.color,
            width: 1.0,
            radius: 12.0.into(),
        },
        ..container::Style::default()
    }
}

fn card_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();

    container::Style {
        background: Some(palette.background.weak.color.into()),
        border: Border {
            color: palette.background.strong.color,
            width: 1.0,
            radius: 8.0.into(),
        },
        ..container::Style::default()
    }
}

fn ghost_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();

    container::Style {
        background: Some(palette.background.weak.color.scale_alpha(0.93).into()),
        border: Border {
            color: palette.primary.strong.color,
            width: 1.0,
            radius: 8.0.into(),
        },
        shadow: Shadow {
            color: Color::BLACK.scale_alpha(0.4),
            offset: Vector::new(0.0, 6.0),
            blur_radius: 18.0,
        },
        ..container::Style::default()
    }
}
