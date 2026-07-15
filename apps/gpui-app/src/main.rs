//! "Tasks" mini-app in gpui 0.2.2 (Zed Industries' UI framework).
//!
//! gpui has no built-in widget library — no TextInput, Button, or ListView.
//! Everything below is composed from styled `div()` elements (tailwind-style
//! builder API) plus raw keyboard events. See GAPS.md for what a production
//! text input would additionally require (EntityInputHandler for IME etc.).

use gpui::{
    App, Application, Bounds, Context, FocusHandle, KeyDownEvent, SharedString, TitlebarOptions,
    Window, WindowBounds, WindowOptions, div, prelude::*, px, rgb, size,
};

struct TasksApp {
    input: String,
    tasks: Vec<(u64, SharedString)>,
    next_id: u64,
    focus_handle: FocusHandle,
}

impl TasksApp {
    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            input: String::new(),
            tasks: Vec::new(),
            next_id: 0,
            focus_handle: cx.focus_handle(),
        }
    }

    fn add_task(&mut self, cx: &mut Context<Self>) {
        let text = self.input.trim();
        if !text.is_empty() {
            self.tasks.push((self.next_id, SharedString::from(text.to_string())));
            self.next_id += 1;
            self.input.clear();
            cx.notify();
        }
    }

    fn delete_task(&mut self, id: u64, cx: &mut Context<Self>) {
        self.tasks.retain(|(task_id, _)| *task_id != id);
        cx.notify();
    }

    /// Minimal hand-rolled text editing: gpui ships no text input widget.
    /// (The official `input.rs` example implements one from scratch in ~750
    /// lines via `EntityInputHandler`; this handles plain typing only.)
    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        if keystroke.modifiers.platform
            || keystroke.modifiers.control
            || keystroke.modifiers.function
        {
            return;
        }
        match keystroke.key.as_str() {
            "enter" => self.add_task(cx),
            "backspace" => {
                self.input.pop();
                cx.notify();
            }
            _ => {
                if let Some(key_char) = &keystroke.key_char {
                    self.input.push_str(key_char);
                    cx.notify();
                }
            }
        }
    }
}

impl Render for TasksApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let input_focused = self.focus_handle.is_focused(window);
        let count = self.tasks.len();

        // --- Text input (styled div + key events; no built-in widget) ---
        let text_input = div()
            .id("input")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .flex_1()
            .flex()
            .items_center()
            .h(px(32.))
            .px_2()
            .bg(gpui::white())
            .border_1()
            .border_color(if input_focused {
                rgb(0x3b82f6)
            } else {
                rgb(0xd1d5db)
            })
            .rounded_md()
            .cursor_text()
            .child(if self.input.is_empty() {
                div()
                    .text_color(rgb(0x9ca3af))
                    .child("What needs to be done?")
            } else {
                div()
                    .text_color(rgb(0x111827))
                    .child(self.input.clone())
            })
            .when(input_focused, |this| {
                // caret
                this.child(div().w(px(1.5)).h(px(16.)).bg(rgb(0x111827)))
            });

        let add_button = div()
            .id("add")
            .flex_none()
            .flex()
            .items_center()
            .h(px(32.))
            .px_3()
            .bg(rgb(0x3b82f6))
            .hover(|this| this.bg(rgb(0x2563eb)))
            .active(|this| this.bg(rgb(0x1d4ed8)))
            .rounded_md()
            .cursor_pointer()
            .text_color(gpui::white())
            .child("Add")
            .on_click(cx.listener(|this, _, _, cx| this.add_task(cx)));

        // --- Task list (scrollable) ---
        let task_list = div()
            .id("tasks")
            .flex_1()
            .overflow_y_scroll()
            .px_3()
            .child(
                div().flex().flex_col().gap_1().pb_3().children(
                    self.tasks.iter().map(|(id, text)| {
                        let id = *id;
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .px_2()
                            .py_1()
                            .bg(gpui::white())
                            .border_1()
                            .border_color(rgb(0xe5e7eb))
                            .rounded_md()
                            .child(div().flex_1().truncate().child(text.clone()))
                            .child(
                                div()
                                    .id(("delete", id))
                                    .flex_none()
                                    .px_2()
                                    .rounded_md()
                                    .text_color(rgb(0x6b7280))
                                    .hover(|this| {
                                        this.bg(rgb(0xfee2e2)).text_color(rgb(0xdc2626))
                                    })
                                    .cursor_pointer()
                                    .child("Delete")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.delete_task(id, cx)
                                    })),
                            )
                    }),
                ),
            );

        let counter = div()
            .flex_none()
            .px_3()
            .py_2()
            .border_t_1()
            .border_color(rgb(0xe5e7eb))
            .text_color(rgb(0x6b7280))
            .child(format!("{count} task(s)"));

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0xf9fafb))
            .text_sm()
            .text_color(rgb(0x111827))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .gap_2()
                    .p_3()
                    .child(text_input)
                    .child(add_button),
            )
            .child(task_list)
            .child(counter)
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(480.), px(640.)), cx);

        // gpui apps do not quit when the last window closes unless told to.
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        cx.open_window(
            WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some(SharedString::from("Tasks (gpui)")),
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| {
                cx.new(|cx| {
                    let app = TasksApp::new(cx);
                    // Focus the input on launch so Enter/typing works immediately.
                    app.focus_handle.focus(window);
                    app
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
