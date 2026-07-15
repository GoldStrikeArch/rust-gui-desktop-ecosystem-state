//! "Tasks" mini-app per apps/SPEC.md, written in idiomatic immediate-mode
//! egui 0.35 with eframe as the app shell.

use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([480.0, 640.0])
            .with_resizable(true),
        ..Default::default()
    };
    eframe::run_native(
        "Tasks (egui)", // window title
        options,
        Box::new(|_cc| Ok(Box::new(TasksApp::default()))),
    )
}

#[derive(Default)]
struct TasksApp {
    input: String,
    tasks: Vec<String>,
}

impl TasksApp {
    /// Append the trimmed input as a new task (no-op for empty/whitespace)
    /// and clear the input.
    fn add_task(&mut self) {
        let trimmed = self.input.trim();
        if !trimmed.is_empty() {
            self.tasks.push(trimmed.to_owned());
        }
        self.input.clear();
    }
}

impl TasksApp {
    /// The whole UI, immediate-mode style: rebuilt from `self` every frame.
    /// Split out from `eframe::App::ui` so tests can drive it headlessly.
    fn show(&mut self, ui: &mut egui::Ui) {
        // Input row: text field + "Add" button.
        ui.horizontal(|ui| {
            let button_width = 50.0;
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.input)
                    .hint_text("What needs to be done?")
                    .desired_width(ui.available_width() - button_width),
            );

            // Enter while the input is focused: egui reports this as the
            // TextEdit losing focus with the Enter key pressed this frame.
            let enter_pressed =
                response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if enter_pressed {
                self.add_task();
                response.request_focus(); // keep typing more tasks
            }

            if ui.button("Add").clicked() {
                self.add_task();
            }
        });

        // Live counter.
        ui.label(format!("{} task(s)", self.tasks.len()));
        ui.separator();

        // Task list; scrolls when it overflows the window.
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                let mut delete_index = None;
                for (index, task) in self.tasks.iter().enumerate() {
                    ui.horizontal(|ui| {
                        // Right-align the delete button, let the text
                        // take the remaining width.
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                if ui.button("Delete").clicked() {
                                    delete_index = Some(index);
                                }
                                ui.with_layout(
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |ui| ui.label(task),
                                );
                            },
                        );
                    });
                }
                if let Some(index) = delete_index {
                    self.tasks.remove(index);
                }
            });
    }
}

impl eframe::App for TasksApp {
    // Since egui 0.34, `App::ui` (replacing `App::update`) hands us the root
    // `Ui` of the viewport; we wrap it in a `CentralPanel` for margin/background.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| self.show(ui));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    fn new_harness() -> Harness<'static, TasksApp> {
        Harness::new_ui_state(
            |ui, app: &mut TasksApp| app.show(ui),
            TasksApp::default(),
        )
    }

    /// Drives the UI through its AccessKit tree (egui_kittest queries nodes
    /// by accessibility label/role), so this doubles as a smoke test that
    /// the widgets are exposed to assistive technology.
    #[test]
    fn add_via_button_then_delete() {
        let mut harness = new_harness();

        let input = harness.get_by_role(egui::accesskit::Role::TextInput);
        input.focus(); // egui::Event::Text goes to the focused widget
        input.type_text("Buy milk");
        harness.run();
        harness.get_by_label("Add").click();
        harness.run();

        assert_eq!(harness.state().tasks, ["Buy milk"]);
        assert!(harness.state().input.is_empty());
        harness.get_by_label("1 task(s)"); // counter is in the a11y tree

        harness.get_by_label("Delete").click();
        harness.run();
        assert!(harness.state().tasks.is_empty());
        harness.get_by_label("0 task(s)");
    }

    #[test]
    fn enter_adds_trimmed_task_and_ignores_whitespace() {
        let mut harness = new_harness();

        let input = harness.get_by_role(egui::accesskit::Role::TextInput);
        input.focus();
        input.type_text("  Walk the dog  ");
        harness.run();
        harness.key_press(egui::Key::Enter);
        harness.run();

        assert_eq!(harness.state().tasks, ["Walk the dog"]);
        assert!(harness.state().input.is_empty());

        // Whitespace-only input must be ignored.
        let input = harness.get_by_role(egui::accesskit::Role::TextInput);
        input.focus();
        input.type_text("   ");
        harness.run();
        harness.key_press(egui::Key::Enter);
        harness.run();

        assert_eq!(harness.state().tasks, ["Walk the dog"]);
    }
}
