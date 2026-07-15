//! "Tasks" mini-app — Dioxus 0.7 desktop (wry/tao webview renderer).
//! State lives in Rust via `use_signal`; UI is declared with the RSX macro.

use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
use dioxus::prelude::*;

fn main() {
    dioxus::LaunchBuilder::desktop()
        .with_cfg(
            Config::new().with_window(
                WindowBuilder::new()
                    .with_title("Tasks (dioxus)")
                    .with_inner_size(LogicalSize::new(480.0, 640.0))
                    .with_resizable(true),
            ),
        )
        .launch(App);
}

#[component]
fn App() -> Element {
    // Reactive state: both signals live in Rust, not in the webview.
    let mut tasks = use_signal(Vec::<String>::new);
    let mut input = use_signal(String::new);

    // Signals are Copy, so this closure is Copy and can back both the
    // button click and the Enter-key handler.
    let mut add_task = move || {
        let text = input.read().trim().to_string();
        if !text.is_empty() {
            tasks.write().push(text);
            input.set(String::new());
        }
    };

    rsx! {
        div {
            style: "display: flex; flex-direction: column; height: 100vh; margin: 0; \
                    padding: 12px; box-sizing: border-box; gap: 8px; \
                    font-family: system-ui, sans-serif;",

            // Input row: text field + Add button.
            div {
                style: "display: flex; gap: 8px;",
                input {
                    style: "flex: 1; padding: 6px 8px; font-size: 14px;",
                    placeholder: "What needs to be done?",
                    value: "{input}",
                    oninput: move |evt| input.set(evt.value()),
                    onkeydown: move |evt| {
                        if evt.key() == Key::Enter {
                            add_task();
                        }
                    },
                }
                button {
                    style: "padding: 6px 14px;",
                    onclick: move |_| add_task(),
                    "Add"
                }
            }

            // Live counter.
            p {
                style: "margin: 0; color: #555; font-size: 13px;",
                "{tasks.read().len()} task(s)"
            }

            // Scrollable task list.
            ul {
                style: "flex: 1; overflow-y: auto; margin: 0; padding: 0; list-style: none;",
                for (i, task) in tasks.read().iter().enumerate() {
                    li {
                        key: "{i}-{task}",
                        style: "display: flex; align-items: center; gap: 8px; \
                                padding: 6px 4px; border-bottom: 1px solid #ddd;",
                        span { style: "flex: 1; word-break: break-word;", "{task}" }
                        button {
                            style: "padding: 2px 10px;",
                            onclick: move |_| {
                                tasks.write().remove(i);
                            },
                            "Delete"
                        }
                    }
                }
            }
        }
    }
}
