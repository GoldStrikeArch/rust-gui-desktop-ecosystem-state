//! "Tasks" mini-app — Freya 0.4, idiomatic builder API:
//! reactive `use_state` signals, a `render` function that re-derives the
//! element tree whenever a signal it read changes, and stock components
//! (`Input`, `Button`, `ScrollView`) for the widgets.

use freya::prelude::*;

fn main() {
    launch(
        LaunchConfig::new().with_window(
            WindowConfig::new(app)
                .with_title("Tasks (freya)")
                .with_size(480.0, 640.0),
        ),
    )
}

fn app() -> impl IntoElement {
    // `State<T>` is `Copy`, so both the Add button and the Input's on_submit
    // can capture the same handles without cloning.
    let input = use_state(String::new);
    let tasks = use_state(Vec::<String>::new);

    let count = tasks.read().len();

    rect()
        .expanded()
        .padding(Gaps::new_all(15.))
        .spacing(15.)
        .child(
            rect()
                .horizontal()
                // `Size::flex` on a child requires the parent to opt into
                // `Content::Flex`; otherwise the child falls back silently.
                .content(Content::flex())
                .spacing(10.)
                .cross_align(Alignment::Center)
                .child(
                    Input::new(input)
                        .placeholder("What needs to be done?")
                        .width(Size::flex(1.))
                        .on_submit(move |_| add_task(input, tasks)),
                )
                .child(
                    Button::new()
                        .on_press(move |_| add_task(input, tasks))
                        .child("Add"),
                ),
        )
        .child(label().text(format!("{count} task(s)")).font_size(14.))
        .child(
            ScrollView::new().spacing(10.).children(
                tasks
                    .read()
                    .iter()
                    .enumerate()
                    .map(|(index, task)| task_row(index, task, tasks))
                    .collect::<Vec<_>>(),
            ),
        )
}

/// Trim-and-append; whitespace-only input is ignored. Shared by the Add button
/// and by Enter-to-submit inside the Input.
fn add_task(mut input: State<String>, mut tasks: State<Vec<String>>) {
    let task = input.read().trim().to_owned();

    if !task.is_empty() {
        tasks.write().push(task);
        input.write().clear();
    }
}

fn task_row(index: usize, task: &str, mut tasks: State<Vec<String>>) -> Element {
    rect()
        .key(index)
        .horizontal()
        .content(Content::flex())
        .spacing(10.)
        .cross_align(Alignment::Center)
        .child(label().text(task.to_owned()).width(Size::flex(1.)))
        .child(
            Button::new()
                .on_press(move |_| {
                    tasks.write().remove(index);
                })
                .child("Delete"),
        )
        .into()
}
