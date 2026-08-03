//! "Tasks" mini-app — floem (git main @ 778bb5f2), idiomatic fine-grained
//! reactivity: signals hold the state, the view tree is built ONCE, and only
//! the views that read a changed signal update (no Elm loop, no full-tree
//! rebuild). `dyn_stack` diffs the task list by key.
//!
//! API note (research-relevant): at this rev the free-function view
//! constructors (`v_stack`, `button`, `label`, `text_input`, ...) that all
//! published floem docs use are DEPRECATED in favor of struct constructors
//! (`Stack::vertical`, `Button::new`, `Label::derived`, `TextInput::new`).
//! This file uses the current, non-deprecated API.

use floem::Application;
use floem::kurbo::Size;
use floem::prelude::*;
use floem::window::WindowConfig;

/// A task row. Rows get a stable id so `dyn_stack` can diff by key
/// (indices would re-create every row below a deletion).
#[derive(Clone, PartialEq, Eq, Hash)]
struct Task {
    id: u64,
    text: String,
}

fn app_view() -> impl IntoView {
    let input = RwSignal::new(String::new());
    let tasks: RwSignal<Vec<Task>> = RwSignal::new(Vec::new());
    let next_id = RwSignal::new(0u64);

    let add = move || {
        let text = input.with_untracked(|s| s.trim().to_string());
        if text.is_empty() {
            return;
        }
        let id = next_id.get_untracked();
        next_id.set(id + 1);
        tasks.update(|t| t.push(Task { id, text }));
        input.set(String::new());
    };

    let entry_row = Stack::horizontal((
        TextInput::new(input)
            .placeholder("What needs to be done?")
            // Enter-to-add: TextInput emits a typed custom event on Enter.
            .on_event_stop(TextInputEnter::listener(), move |_, _| add())
            .style(|s| s.flex_grow(1.0).padding(10.0)),
        Button::new("Add").action(add).style(|s| s.padding(10.0)),
    ))
    .style(|s| s.gap(10.0).items_center().width_full());

    let counter = Label::derived(move || {
        let n = tasks.with(|t| t.len());
        format!("{n} task(s)")
    })
    .style(|s| s.font_size(14.0));

    let list = dyn_stack(
        move || tasks.get(),
        |task| task.id,
        move |task| {
            let id = task.id;
            Stack::horizontal((
                Label::new(task.text).style(|s| s.flex_grow(1.0)),
                Button::new("Delete")
                    .action(move || tasks.update(|t| t.retain(|task| task.id != id))),
            ))
            .style(|s| s.gap(10.0).items_center().width_full())
        },
    )
    .style(|s| s.flex_col().gap(10.0).width_full());

    Stack::vertical((
        entry_row,
        counter,
        list.scroll().style(|s| s.flex_grow(1.0).width_full()),
    ))
    .style(|s| s.flex_col().gap(15.0).padding(15.0).size_full())
}

fn main() {
    Application::new()
        .window(
            |_| app_view(),
            Some(
                WindowConfig::default()
                    .title("Tasks (floem)")
                    .size(Size::new(480.0, 640.0)),
            ),
        )
        .run();
}
