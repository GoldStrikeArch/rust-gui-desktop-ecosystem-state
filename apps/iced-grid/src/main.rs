//! "Grid" — 100k-row data table (SPEC-7), iced 0.14.
//!
//! Architecture notes (research-relevant):
//! - iced 0.14 ships a `table` widget, but it is a *layout* helper: it
//!   eagerly builds one `Element` per cell for every row passed to it
//!   (verified by source-reading `iced_widget/src/table.rs`), with no
//!   virtualization, sorting, selection or column resizing. At 100k rows ×
//!   6 columns that is 600k widgets — unusable. So the table here is
//!   hand-rolled: a `scrollable` whose content is
//!   `[top spacer, ~visible rows, bottom spacer]`, re-windowed from the
//!   `on_scroll` viewport offset. Only ~40 row widgets exist at any time.
//! - Sorting/filtering operate on `visible: Vec<u32>` (indices into the
//!   immutable `rows`), recomputed synchronously in `update` and self-timed
//!   (`FILTER_MS <query_len> <ms>` on stdout, per the spec).
//! - Column resize is a hand-rolled drag: a thin `mouse_area` strip after
//!   each header arms the drag; a global `event::listen_with` subscription
//!   (only alive while resizing) streams cursor deltas — the same pattern as
//!   apps/iced-dash's card drag.
//! - Shift-click range selection needs keyboard modifiers, which
//!   `mouse_area::on_press` does not carry; a permanent `listen_with`
//!   subscription tracks `keyboard::Event::ModifiersChanged` instead.
//!
//! Verification instrumentation: with `GRID_SELFTEST=1`, state transitions
//! (sort, selection, resize, re-windowing) also print evidence lines to
//! stdout so synthetic input can be verified from the captured log.

use std::collections::HashSet;
use std::time::Instant;

use iced::event;
use iced::keyboard;
use iced::mouse;
use iced::widget::{
    column, container, mouse_area, operation, row, scrollable, sensor, space,
    text, text_input,
};
use iced::window;
use iced::{
    Border, Center, Color, Element, Event, Fill, Point, Right, Subscription,
    Task, Theme,
};

const N_ROWS: usize = 100_000;
const ROW_H: f32 = 26.0;
const HEADER_H: f32 = 30.0;
const OVERSCAN: usize = 8; // extra rows rendered above/below the viewport
const MIN_COL_W: f32 = 40.0;
const MAX_COL_W: f32 = 480.0;
const DIVIDER_W: f32 = 7.0; // grab strip between headers

const CATEGORIES: [&str; 8] = [
    "alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel",
];
const ADJECTIVES: [&str; 16] = [
    "brisk", "calm", "dusty", "eager", "fuzzy", "grand", "happy", "icy",
    "jolly", "keen", "lucid", "misty", "noble", "odd", "prime", "quiet",
];
const NOUNS: [&str; 16] = [
    "falcon", "otter", "maple", "comet", "river", "ember", "stone", "cloud",
    "harbor", "meadow", "signal", "circuit", "lantern", "orchid", "summit",
    "walnut",
];

pub fn main() -> iced::Result {
    iced::application(Grid::new, Grid::update, Grid::view)
        .title(|_: &Grid| String::from("Grid (iced)"))
        .window_size((1000.0, 640.0))
        .subscription(Grid::subscription)
        .run()
}

// ---------------------------------------------------------------------------
// Data
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    Ok,
    Warn,
    Err,
}

impl Status {
    fn label(self) -> &'static str {
        match self {
            Status::Ok => "Ok",
            Status::Warn => "Warn",
            Status::Err => "Err",
        }
    }

    fn rank(self) -> u8 {
        match self {
            Status::Ok => 0,
            Status::Warn => 1,
            Status::Err => 2,
        }
    }
}

struct Row {
    id: u32,
    name: String, // all-lowercase "adjective-noun-####"
    category: &'static str,
    value: f64, // displayed with 2 decimals, right-aligned
    date: String, // ISO yyyy-mm-dd (lexicographic order == chronological)
    status: Status,
}

/// Tiny xorshift* PRNG — deterministic data without pulling in `rand`
/// (same generator as apps/iced-dash).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

fn generate_rows() -> Vec<Row> {
    let mut rng = Rng(0xDEAD_BEEF_CAFE_F00D);

    (0..N_ROWS as u32)
        .map(|id| {
            let r = rng.next();
            let adjective = ADJECTIVES[(r >> 8) as usize % ADJECTIVES.len()];
            let noun = NOUNS[(r >> 16) as usize % NOUNS.len()];
            let number = (r >> 24) % 10_000;

            let value = (rng.next() % 100_000_000) as f64 / 100.0;

            let d = rng.next();
            let (year, month, day) =
                (2019 + d % 7, 1 + (d >> 16) % 12, 1 + (d >> 32) % 28);

            let status = match rng.next() % 10 {
                0..=6 => Status::Ok,
                7..=8 => Status::Warn,
                _ => Status::Err,
            };

            Row {
                id,
                name: format!("{adjective}-{noun}-{number:04}"),
                category: CATEGORIES[(r >> 40) as usize % CATEGORIES.len()],
                value,
                date: format!("{year:04}-{month:02}-{day:02}"),
                status,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum SortDir {
    Asc,
    Desc,
}

const COLUMNS: [&str; 6] = ["id", "name", "category", "value", "date", "status"];

struct ResizeDrag {
    column: usize,
    start_width: f32,
    /// Cursor x when the drag started (window coordinates).
    origin: Option<f32>,
}

struct Grid {
    rows: Vec<Row>,
    /// Filtered (+ sorted) indices into `rows`, in display order.
    visible: Vec<u32>,
    query: String,
    sort: Option<(usize, SortDir)>,
    col_widths: [f32; 6],
    /// Vertical scroll offset of the body, from `on_scroll`.
    scroll_y: f32,
    /// Height of the scrollable's viewport, from the `sensor` wrapper.
    viewport_h: f32,
    /// Selected rows (indices into `rows`).
    selected: HashSet<u32>,
    /// Position in `visible` of the last plain click (range anchor).
    anchor: Option<usize>,
    modifiers: keyboard::Modifiers,
    resize: Option<ResizeDrag>,
    selftest: bool,
    /// Last first-visible row printed by the self-test scroll trace.
    traced_first: usize,
}

#[derive(Debug, Clone)]
enum Message {
    QueryChanged(String),
    HeaderClicked(usize),
    ResizePressed(usize),
    PointerMoved(Point),
    PointerReleased,
    ModifiersChanged(keyboard::Modifiers),
    RowClicked(usize), // position in `visible`
    Scrolled(scrollable::Viewport),
    BodyResized(iced::Size),
}

const FILTER_ID: &str = "filter";
const BODY_ID: &str = "body";

impl Grid {
    fn new() -> (Self, Task<Message>) {
        let start = Instant::now();

        let rows = generate_rows();
        let visible: Vec<u32> = (0..rows.len() as u32).collect();

        println!("BUILD_MS {:.2}", start.elapsed().as_secs_f64() * 1000.0);

        (
            Self {
                rows,
                visible,
                query: String::new(),
                sort: None,
                col_widths: [70.0, 220.0, 110.0, 130.0, 130.0, 90.0],
                scroll_y: 0.0,
                viewport_h: 520.0,
                selected: HashSet::new(),
                anchor: None,
                modifiers: keyboard::Modifiers::default(),
                resize: None,
                selftest: std::env::var_os("GRID_SELFTEST").is_some(),
                traced_first: 0,
            },
            operation::focus(FILTER_ID),
        )
    }

    /// Recompute `visible` from the current query + sort. Self-timed.
    fn refresh(&mut self) -> Task<Message> {
        let start = Instant::now();
        let query = self.query.to_lowercase();

        self.visible = (0..self.rows.len() as u32)
            .filter(|&i| {
                query.is_empty()
                    || self.rows[i as usize].name.contains(&query)
            })
            .collect();
        self.apply_sort();

        println!(
            "FILTER_MS {} {:.2}",
            self.query.chars().count(),
            start.elapsed().as_secs_f64() * 1000.0
        );

        // The content shrank/grew: snap back to the top so the window
        // (windowed rows + scrollable offset) stays consistent.
        self.scroll_y = 0.0;
        self.anchor = None;
        operation::scroll_to(
            BODY_ID,
            scrollable::AbsoluteOffset { x: 0.0, y: 0.0 },
        )
    }

    fn apply_sort(&mut self) {
        let Some((column, direction)) = self.sort else {
            return;
        };

        let rows = &self.rows;
        match column {
            0 => self.visible.sort_unstable_by_key(|&i| rows[i as usize].id),
            1 => self
                .visible
                .sort_unstable_by(|&a, &b| {
                    rows[a as usize].name.cmp(&rows[b as usize].name)
                }),
            2 => self.visible.sort_unstable_by(|&a, &b| {
                rows[a as usize]
                    .category
                    .cmp(rows[b as usize].category)
                    .then(rows[a as usize].id.cmp(&rows[b as usize].id))
            }),
            3 => self.visible.sort_unstable_by(|&a, &b| {
                rows[a as usize]
                    .value
                    .total_cmp(&rows[b as usize].value)
            }),
            4 => self.visible.sort_unstable_by(|&a, &b| {
                rows[a as usize]
                    .date
                    .cmp(&rows[b as usize].date)
                    .then(rows[a as usize].id.cmp(&rows[b as usize].id))
            }),
            _ => self.visible.sort_unstable_by(|&a, &b| {
                rows[a as usize]
                    .status
                    .rank()
                    .cmp(&rows[b as usize].status.rank())
                    .then(rows[a as usize].id.cmp(&rows[b as usize].id))
            }),
        }

        if direction == SortDir::Desc {
            self.visible.reverse();
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::QueryChanged(query) => {
                self.query = query;
                return self.refresh();
            }
            Message::HeaderClicked(column) => {
                let direction = match self.sort {
                    Some((c, SortDir::Asc)) if c == column => SortDir::Desc,
                    _ => SortDir::Asc,
                };
                self.sort = Some((column, direction));

                let start = Instant::now();
                self.apply_sort();
                let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
                self.anchor = None;

                if self.selftest {
                    println!(
                        "SORT {} {} first_id={} ms={elapsed_ms:.2}",
                        COLUMNS[column],
                        if direction == SortDir::Asc { "asc" } else { "desc" },
                        self.visible.first().map_or(0, |&i| self.rows[i as usize].id),
                    );
                }
            }
            Message::ResizePressed(column) => {
                self.resize = Some(ResizeDrag {
                    column,
                    start_width: self.col_widths[column],
                    origin: None,
                });
            }
            Message::PointerMoved(position) => {
                if let Some(drag) = &mut self.resize {
                    match drag.origin {
                        None => drag.origin = Some(position.x),
                        Some(origin) => {
                            self.col_widths[drag.column] = (drag.start_width
                                + position.x
                                - origin)
                                .clamp(MIN_COL_W, MAX_COL_W);
                        }
                    }
                }
            }
            Message::PointerReleased => {
                if let Some(drag) = self.resize.take()
                    && self.selftest
                {
                    println!(
                        "RESIZE col={} width={:.0}",
                        COLUMNS[drag.column], self.col_widths[drag.column]
                    );
                }
            }
            Message::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers;
            }
            Message::RowClicked(position) => {
                let Some(&index) = self.visible.get(position) else {
                    return Task::none();
                };

                if self.modifiers.shift()
                    && let Some(anchor) = self.anchor
                {
                    let (from, to) = if anchor <= position {
                        (anchor, position)
                    } else {
                        (position, anchor)
                    };
                    self.selected =
                        self.visible[from..=to].iter().copied().collect();
                } else if self.modifiers.command() || self.modifiers.control()
                {
                    if !self.selected.remove(&index) {
                        self.selected.insert(index);
                    }
                    self.anchor = Some(position);
                } else {
                    self.selected = HashSet::from([index]);
                    self.anchor = Some(position);
                }

                if self.selftest {
                    println!(
                        "SELECT count={} clicked_id={}",
                        self.selected.len(),
                        self.rows[index as usize].id
                    );
                }
            }
            Message::Scrolled(viewport) => {
                self.scroll_y = viewport.absolute_offset().y;
                self.viewport_h = viewport.bounds().height;

                if self.selftest {
                    let first = self.first_visible();
                    if first != self.traced_first {
                        self.traced_first = first;
                        println!(
                            "WINDOW first={} y={:.0}",
                            first, self.scroll_y
                        );
                    }
                }
            }
            Message::BodyResized(size) => {
                self.viewport_h = size.height;
            }
        }

        Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        let modifiers = event::listen_with(|event, _status, _window| {
            match event {
                Event::Keyboard(keyboard::Event::ModifiersChanged(m)) => {
                    Some(Message::ModifiersChanged(m))
                }
                _ => None,
            }
        });

        // Cursor tracking only lives while a header divider drag is active.
        if self.resize.is_some() {
            Subscription::batch([
                modifiers,
                event::listen_with(resize_listener),
            ])
        } else {
            modifiers
        }
    }

    fn first_visible(&self) -> usize {
        ((self.scroll_y / ROW_H) as usize)
            .saturating_sub(OVERSCAN)
            .min(self.visible.len())
    }

    // -----------------------------------------------------------------------
    // View
    // -----------------------------------------------------------------------

    fn view(&self) -> Element<'_, Message> {
        let controls = row![
            text_input("filter by name…", &self.query)
                .id(FILTER_ID)
                .on_input(Message::QueryChanged)
                .width(300),
            text(format!("{} of 100,000 rows", self.visible.len())).size(14),
            space::horizontal(),
            text("click header: sort · drag divider: resize · shift-click: range")
                .size(12)
                .style(text::secondary),
        ]
        .spacing(14)
        .align_y(Center);

        let body = container(
            column![self.header(), self.rows_view()]
                .width(self.table_width())
                .height(Fill),
        )
        .style(panel_style)
        .height(Fill);

        column![controls, body]
            .spacing(10)
            .padding(10)
            .height(Fill)
            .into()
    }

    fn table_width(&self) -> f32 {
        self.col_widths.iter().sum::<f32>()
            + DIVIDER_W * self.col_widths.len() as f32
    }

    fn header(&self) -> Element<'_, Message> {
        let mut cells: Vec<Element<'_, Message>> = Vec::new();

        for (i, &name) in COLUMNS.iter().enumerate() {
            let indicator = match self.sort {
                Some((c, SortDir::Asc)) if c == i => " ▲",
                Some((c, SortDir::Desc)) if c == i => " ▼",
                _ => "",
            };

            cells.push(
                mouse_area(
                    container(text(format!("{name}{indicator}")).size(13))
                        .width(self.col_widths[i])
                        .height(HEADER_H)
                        .padding([6, 8])
                        .style(header_style),
                )
                .on_press(Message::HeaderClicked(i))
                .interaction(mouse::Interaction::Pointer)
                .into(),
            );

            // Divider grab strip: drag to resize column `i`.
            cells.push(
                mouse_area(
                    container(space::vertical())
                        .width(DIVIDER_W)
                        .height(HEADER_H)
                        .style(divider_style),
                )
                .on_press(Message::ResizePressed(i))
                .interaction(mouse::Interaction::ResizingHorizontally)
                .into(),
            );
        }

        row(cells).into()
    }

    /// The virtualized body: spacer + windowed rows + spacer.
    fn rows_view(&self) -> Element<'_, Message> {
        let total = self.visible.len();
        let first = self.first_visible();
        let count = ((self.viewport_h / ROW_H) as usize + 2 * OVERSCAN)
            .min(total - first);

        let top_gap = first as f32 * ROW_H;
        let bottom_gap = (total - first - count) as f32 * ROW_H;

        let window = self.visible[first..first + count]
            .iter()
            .enumerate()
            .map(|(offset, &index)| {
                self.row_view(first + offset, index)
            });

        let content = column![space().height(top_gap)]
            .extend(window)
            .push(space().height(bottom_gap));

        // The `sensor` wrapper reports the body's actual size, so the row
        // window is correct before the first scroll and after window resizes.
        sensor(
            scrollable(content)
                .id(BODY_ID)
                .on_scroll(Message::Scrolled)
                .width(Fill)
                .height(Fill),
        )
        .on_show(Message::BodyResized)
        .on_resize(Message::BodyResized)
        .into()
    }

    fn row_view(&self, position: usize, index: u32) -> Element<'_, Message> {
        let data = &self.rows[index as usize];
        let w = &self.col_widths;

        let chip_colors = match data.status {
            Status::Ok => (Color::from_rgb8(0x1d, 0x7a, 0x33), Color::WHITE),
            Status::Warn => {
                (Color::from_rgb8(0xc9, 0x8a, 0x0b), Color::BLACK)
            }
            Status::Err => (Color::from_rgb8(0xc2, 0x33, 0x2e), Color::WHITE),
        };

        fn cell<'a>(
            content: impl Into<Element<'a, Message>>,
            width: f32,
        ) -> Element<'a, Message> {
            container(content)
                .width(width + DIVIDER_W)
                .height(ROW_H)
                .padding([4, 8])
                .clip(true)
                .into()
        }

        let cells = row![
            cell(text(data.id.to_string()).size(13), w[0]),
            cell(text(&data.name).size(13), w[1]),
            cell(text(data.category).size(13), w[2]),
            cell(
                container(text(format!("{:.2}", data.value)).size(13))
                    .width(Fill)
                    .align_x(Right),
                w[3]
            ),
            cell(text(&data.date).size(13), w[4]),
            cell(
                container(
                    container(text(data.status.label()).size(11).color(chip_colors.1))
                        .padding([1, 8])
                        .style(move |_: &Theme| container::Style {
                            background: Some(chip_colors.0.into()),
                            border: Border::default().rounded(9.0),
                            ..container::Style::default()
                        })
                )
                .align_y(Center)
                .height(Fill),
                w[5]
            ),
        ];

        let selected = self.selected.contains(&index);
        let zebra = position % 2 == 1;

        mouse_area(container(cells).style(move |theme: &Theme| {
            let palette = theme.extended_palette();

            container::Style {
                background: Some(
                    if selected {
                        palette.primary.weak.color
                    } else if zebra {
                        palette.background.weakest.color
                    } else {
                        palette.background.base.color
                    }
                    .into(),
                ),
                ..container::Style::default()
            }
        }))
        .on_press(Message::RowClicked(position))
        .into()
    }
}

/// Global event filter, subscribed only while a resize drag is in progress.
fn resize_listener(
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

// ---------------------------------------------------------------------------
// Styles
// ---------------------------------------------------------------------------

fn header_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();

    container::Style {
        background: Some(palette.background.weak.color.into()),
        ..container::Style::default()
    }
}

fn divider_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();

    container::Style {
        background: Some(palette.background.strong.color.into()),
        ..container::Style::default()
    }
}

fn panel_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();

    container::Style {
        border: Border {
            color: palette.background.strong.color,
            width: 1.0,
            radius: 6.0.into(),
        },
        ..container::Style::default()
    }
}
