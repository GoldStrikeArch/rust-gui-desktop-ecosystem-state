// Board — kanban task board (Slint 1.17.1).
//
// Rust owns the three column models (VecModel<CardData>). The UI talks back
// through the `Logic` global. Drag payloads are `slint::DataTransfer` values
// carrying "col:idx" as plain text; `drop-card` parses the payload, removes
// the card from the source model and inserts it at the target index (with the
// usual -1 adjustment for a within-column move to a later position).
//
// Repaint model: purely event-driven — repaints only happen on property
// changes from user interaction (hover, drag, edits); zero idle CPU.

use std::cell::Cell;
use std::rc::Rc;

use slint::{ComponentHandle, DataTransfer, Model, ModelRc, SharedString, VecModel};

slint::include_modules!();

type Col = Rc<VecModel<CardData>>;

fn col_model(cols: &[Col; 3], idx: i32) -> Option<&Col> {
    usize::try_from(idx).ok().and_then(|i| cols.get(i))
}

fn main() -> Result<(), slint::PlatformError> {
    let ui = MainWindow::new()?;

    let seed = |texts: &[&str], base: i32| -> Col {
        Rc::new(VecModel::from(
            texts
                .iter()
                .enumerate()
                .map(|(i, t)| CardData { id: base + i as i32, text: (*t).into() })
                .collect::<Vec<_>>(),
        ))
    };
    let cols: [Col; 3] = [
        seed(&["Write SPEC-3 friction notes", "Try DragArea/DropArea API"], 0),
        seed(&["Port dashboard to Slint"], 100),
        seed(&["Pin slint 1.17.1"], 200),
    ];
    ui.set_todo_cards(ModelRc::from(cols[0].clone()));
    ui.set_doing_cards(ModelRc::from(cols[1].clone()));
    ui.set_done_cards(ModelRc::from(cols[2].clone()));

    let next_id = Rc::new(Cell::new(1000_i32));
    let logic = ui.global::<Logic>();

    logic.on_add_card({
        let cols = cols.clone();
        let next_id = next_id.clone();
        move |col, text| {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return;
            }
            if let Some(model) = col_model(&cols, col) {
                let id = next_id.get();
                next_id.set(id + 1);
                model.push(CardData { id, text: trimmed.into() });
            }
        }
    });

    logic.on_delete_card({
        let cols = cols.clone();
        move |col, idx| {
            if let Some(model) = col_model(&cols, col) {
                let idx = idx as usize;
                if idx < model.row_count() {
                    model.remove(idx);
                }
            }
        }
    });

    logic.on_edit_card({
        let cols = cols.clone();
        move |col, idx, text| {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return; // treat empty commit as cancel
            }
            if let Some(model) = col_model(&cols, col) {
                if let Some(mut row) = model.row_data(idx as usize) {
                    row.text = trimmed.into();
                    model.set_row_data(idx as usize, row);
                }
            }
        }
    });

    // Drag payload: the source column/index encoded as plain text. The
    // `data-transfer` type is opaque in the DSL by design; only the host
    // language can construct and read it.
    logic.on_make_payload(|col, idx| {
        DataTransfer::from(SharedString::from(format!("{col}:{idx}")))
    });

    logic.on_drop_card({
        let cols = cols.clone();
        move |data, to_col, to_idx| {
            let Ok(payload) = data.plain_text() else { return };
            let Some((sc, si)) = payload
                .split_once(':')
                .and_then(|(a, b)| Some((a.parse::<i32>().ok()?, b.parse::<usize>().ok()?)))
            else {
                return;
            };
            let (Some(src), Some(dst)) = (col_model(&cols, sc), col_model(&cols, to_col))
            else {
                return;
            };
            if si >= src.row_count() {
                return;
            }
            let card = src.row_data(si).unwrap();
            src.remove(si);
            let mut idx = to_idx.max(0) as usize;
            if sc == to_col && si < idx {
                idx -= 1; // removal above the insertion point shifted it
            }
            let idx = idx.min(dst.row_count());
            dst.insert(idx, card);
        }
    });

    ui.run()
}
