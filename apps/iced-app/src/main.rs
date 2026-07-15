//! "Tasks" mini-app — iced 0.14, idiomatic Elm architecture:
//! a `Tasks` state (Model), a `Message` enum, a pure-ish `update`,
//! and a declarative `view` that rebuilds the widget tree each frame.

use iced::widget::{button, column, row, scrollable, text, text_input};
use iced::{Center, Element, Fill};

pub fn main() -> iced::Result {
    iced::application(Tasks::default, update, view)
        .title(|_: &Tasks| String::from("Tasks (iced)"))
        .window_size((480.0, 640.0))
        .run()
}

#[derive(Default)]
struct Tasks {
    input: String,
    tasks: Vec<String>,
}

#[derive(Debug, Clone)]
enum Message {
    InputChanged(String),
    Add,
    Delete(usize),
}

fn update(state: &mut Tasks, message: Message) {
    match message {
        Message::InputChanged(value) => {
            state.input = value;
        }
        Message::Add => {
            let task = state.input.trim();

            if !task.is_empty() {
                state.tasks.push(task.to_owned());
                state.input.clear();
            }
        }
        Message::Delete(index) => {
            state.tasks.remove(index);
        }
    }
}

fn view(state: &Tasks) -> Element<'_, Message> {
    let input = row![
        text_input("What needs to be done?", &state.input)
            .on_input(Message::InputChanged)
            .on_submit(Message::Add)
            .padding(10),
        button("Add").on_press(Message::Add).padding(10),
    ]
    .spacing(10);

    let counter = text(format!("{} task(s)", state.tasks.len())).size(14);

    let list = column(state.tasks.iter().enumerate().map(|(index, task)| {
        row![
            text(task).width(Fill),
            button("Delete").on_press(Message::Delete(index)),
        ]
        .spacing(10)
        .align_y(Center)
        .into()
    }))
    .spacing(10);

    column![input, counter, scrollable(list).height(Fill).spacing(5)]
        .spacing(15)
        .padding(15)
        .into()
}
