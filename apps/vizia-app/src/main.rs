//! "Tasks" mini-app — vizia 0.4, idiomatic reactive/declarative style:
//! a `Model` that owns `Signal`s, a typed event enum routed through
//! `cx.emit(..)` -> `Model::event`, and a view tree built once whose
//! `Signal`-bound parts rebuild themselves when the data changes.
//!
//! Vizia is *not* an immediate-mode or full-rebuild framework: the closure
//! passed to `Application::new` runs once. Reactivity comes from `Signal<T>`
//! (fine-grained, per-binding) — `Label::new(cx, some_signal)` re-renders
//! only that label, and `List::new(cx, vec_signal, ..)` diffs the vector and
//! rebuilds only the rows that structurally changed.

use vizia::prelude::*;

fn main() -> Result<(), ApplicationError> {
    Application::new(|cx| {
        cx.add_stylesheet(STYLE).expect("failed to add stylesheet");

        let app = Tasks::new();
        let input = app.input;
        let tasks = app.tasks;
        // Derived (memoized) value: recomputed only when `tasks` changes.
        let counter = Memo::new(move |_| format!("{} task(s)", tasks.get().len()));
        app.build(cx);

        VStack::new(cx, |cx| {
            HStack::new(cx, |cx| {
                Textbox::new(cx, input)
                    .placeholder("What needs to be done?")
                    .width(Stretch(1.0))
                    // Every keystroke: keep the model in sync.
                    .on_edit(|cx, text| cx.emit(TaskEvent::SetInput(text)))
                    // Enter while focused -> same as clicking "Add".
                    // The bool is `true` for the Enter key and `false` for a
                    // focus-loss commit, so only Enter adds a task. (vizia's
                    // own todo example names this parameter `blur`, which
                    // reads as the opposite of what it means.)
                    .on_submit(|cx, text, enter| {
                        if enter {
                            cx.emit(TaskEvent::Add(text));
                            cx.emit(TextEvent::Clear);
                        }
                    });

                // The button reads the model signal directly; clearing the
                // input is the model's job (setting `input` re-runs the
                // Textbox's own value binding, which resets the visible text
                // and re-shows the placeholder).
                Button::new(cx, |cx| Label::new(cx, "Add"))
                    .variant(ButtonVariant::Primary)
                    .on_press(move |cx| cx.emit(TaskEvent::Add(input.get())));
            })
            .class("input-row");

            Label::new(cx, counter).class("counter");

            // `List` is vizia's keyed collection view: it owns a `ScrollView`
            // internally, so requirement 7 (scroll on overflow) is free.
            List::new(cx, tasks, move |cx, index, task| {
                HStack::new(cx, |cx| {
                    Label::new(cx, task).width(Stretch(1.0)).text_wrap(true);
                    Button::new(cx, |cx| Label::new(cx, "Delete"))
                        .variant(ButtonVariant::Outline)
                        .class("delete")
                        .on_press(move |cx| cx.emit(TaskEvent::Delete(index)));
                })
                .class("task-row");
            })
            .height(Stretch(1.0));
        })
        .class("tasks-app");
    })
    .title("Tasks (vizia)")
    .inner_size((480, 640))
    .run()
}

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

struct Tasks {
    input: Signal<String>,
    tasks: Signal<Vec<String>>,
}

impl Tasks {
    fn new() -> Self {
        Self { input: Signal::new(String::new()), tasks: Signal::new(Vec::new()) }
    }
}

enum TaskEvent {
    SetInput(String),
    Add(String),
    Delete(usize),
}

impl Model for Tasks {
    fn event(&mut self, _cx: &mut EventContext, event: &mut Event) {
        // `take` consumes the message (moves the payload out) so the `String`
        // does not have to be cloned; `map` would hand out a `&TaskEvent`.
        event.take(|task_event, _| match task_event {
            TaskEvent::SetInput(text) => self.input.set(text),
            TaskEvent::Add(text) => {
                let task = text.trim();
                if !task.is_empty() {
                    let task = task.to_owned();
                    self.tasks.update(|tasks| tasks.push(task));
                }
                self.input.set(String::new());
            }
            TaskEvent::Delete(index) => {
                self.tasks.update(|tasks| {
                    if index < tasks.len() {
                        tasks.remove(index);
                    }
                });
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Style
//
// Vizia styles views with CSS (parsed by `vizia_style`). Inline modifiers
// (`.width(Stretch(1.0))`) and stylesheets are interchangeable; a stylesheet
// is the idiomatic place for anything shared, so layout constants live here.
// ---------------------------------------------------------------------------

const STYLE: &str = r#"
.tasks-app {
    width: 1s;
    height: 1s;
    padding: 15px;
    vertical-gap: 12px;
}

.input-row {
    height: auto;
    horizontal-gap: 10px;
    alignment: center;
}

.counter {
    height: auto;
    font-size: 14px;
    color: #666666;
}

.task-row {
    height: auto;
    min-height: 34px;
    horizontal-gap: 10px;
    padding: 4px;
    alignment: center;
}

.task-row .delete {
    min-width: 70px;
}
"#;
