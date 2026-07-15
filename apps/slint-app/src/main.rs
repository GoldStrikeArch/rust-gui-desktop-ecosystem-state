use std::rc::Rc;

use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    let ui = MainWindow::new()?;

    let tasks: Rc<VecModel<SharedString>> = Rc::new(VecModel::default());
    ui.set_tasks(ModelRc::from(tasks.clone()));

    ui.on_add_task({
        let ui = ui.as_weak();
        let tasks = tasks.clone();
        move |text| {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return; // ignore empty/whitespace input
            }
            tasks.push(SharedString::from(trimmed));
            if let Some(ui) = ui.upgrade() {
                ui.set_input_text(SharedString::default());
            }
        }
    });

    ui.on_delete_task({
        let tasks = tasks.clone();
        move |index| {
            let index = index as usize;
            if index < tasks.row_count() {
                tasks.remove(index);
            }
        }
    });

    ui.run()
}
