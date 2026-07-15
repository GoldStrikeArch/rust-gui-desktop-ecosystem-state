// "Tasks" mini-app per apps/SPEC.md, built with xilem 0.4.0.
//
// Xilem is a reactive-view-tree framework: `app_logic` is re-run after every
// state mutation, producing a lightweight view tree that is diffed against the
// previous one and applied to the retained Masonry widget tree.

use xilem::core::one_of::Either;
use xilem::style::Style as _;
use xilem::view::{flex_col, flex_row, label, portal, text_button, text_input, FlexExt as _};
use xilem::winit::error::EventLoopError;
use xilem::{EventLoop, InsertNewline, WidgetView, WindowOptions, Xilem};

struct Tasks {
    input: String,
    tasks: Vec<String>,
}

impl Tasks {
    /// Append the trimmed input as a new task and clear the input.
    /// Empty/whitespace-only input is ignored.
    fn add(&mut self) {
        let text = self.input.trim();
        if !text.is_empty() {
            self.tasks.push(text.to_string());
        }
        self.input.clear();
    }
}

fn app_logic(state: &mut Tasks) -> impl WidgetView<Tasks> + use<> {
    // Text input with placeholder; Enter submits (same as clicking "Add").
    let input = text_input(state.input.clone(), |state: &mut Tasks, new_value| {
        state.input = new_value;
    })
    .placeholder("What needs to be done?")
    .insert_newline(InsertNewline::Never)
    .on_enter(|state: &mut Tasks, _| state.add());

    let add_button = text_button("Add", |state: &mut Tasks| state.add());

    let input_row = flex_row((input.flex(1.0), add_button));

    // Live counter: `N task(s)`.
    let counter = label(format!("{} task(s)", state.tasks.len()));

    // One row per task: task text + a Delete button removing that row.
    let rows = state
        .tasks
        .iter()
        .enumerate()
        .map(|(i, task)| {
            flex_row((
                label(task.clone()).flex(1.0),
                text_button("Delete", move |state: &mut Tasks| {
                    state.tasks.remove(i);
                }),
            ))
        })
        .collect::<Vec<_>>();

    let list = if rows.is_empty() {
        Either::A(label("No tasks yet — add one above."))
    } else {
        Either::B(portal(flex_col(rows)))
    };

    flex_col((input_row, counter, list.flex(1.0))).padding(12.0)
}

fn main() -> Result<(), EventLoopError> {
    let state = Tasks {
        input: String::new(),
        tasks: Vec::new(),
    };

    let window = WindowOptions::new("Tasks (xilem)")
        .with_initial_inner_size(xilem::dpi::LogicalSize::new(480.0, 640.0))
        .with_resizable(true);

    Xilem::new_simple(state, app_logic, window).run_in(EventLoop::with_user_event())
}
