//! The "manage ratings" dialog, bound (`ui/manage_ratings.slint`): the
//! files' ratings read from the store into hydrus-gui-model's
//! [`RatingsEditor`](crate::ratings_editor::RatingsEditor), drawn as the
//! viewer draws ratings (mixed ones in their service's mixed colours), and
//! "apply" writing the ratings changed, as the reference's
//! `DialogManageRatings` does.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use slint::{ComponentHandle as _, Model as _, ModelRc, SharedString, VecModel};

use hydrus_core::HashId;
use hydrus_store::Store;

use crate::ratings_editor::{RatingsEditor, Update, title};
use crate::{ManageRatingsWindow, RatingRow, RatingShape};

struct State {
    // Updating row data retains each TouchArea and its active pointer grab.
    drawn: Rc<VecModel<RatingRow>>,
    editor: RatingsEditor,
    notice: String,
    /// A paste's error, shown until dismissed.
    error: Option<String>,
}

fn colour(rgb: hydrus_store::services::Rgb) -> slint::Color {
    slint::Color::from_rgb_u8(rgb.0[0], rgb.0[1], rgb.0[2])
}

fn show(window: &ManageRatingsWindow, state: &State) {
    window.set_counter_widths(ModelRc::new(VecModel::from(
        state
            .editor
            .rows
            .iter()
            .map(|row| {
                let (control, _) = row.display();
                if let crate::ratings::Kind::IncDec { value } = control.kind {
                    hydrus_gui_model::rating_sizes::counter_width(
                        f64::from(window.get_incdec_height()),
                        value,
                    ) as f32
                } else {
                    0.0
                }
            })
            .collect::<Vec<_>>(),
    )));
    let names: Vec<SharedString> = state
        .editor
        .rows
        .iter()
        .map(|r| r.name.as_str().into())
        .collect();
    window.set_names(ModelRc::new(VecModel::from(names)));
    let rows: Vec<RatingRow> = state
        .editor
        .rows
        .iter()
        .map(|row| {
            let (control, mixed) = row.display();
            let mut drawn = crate::rating_row(&control);
            if mixed {
                let m = control.colours.mixed;
                let shapes: Vec<RatingShape> = (0..control.shapes().len())
                    .map(|_| RatingShape {
                        pen: colour(m.pen),
                        brush: colour(m.brush),
                    })
                    .collect();
                if !matches!(control.kind, crate::ratings::Kind::IncDec { .. }) {
                    drawn.shapes = ModelRc::new(VecModel::from(shapes));
                }
                drawn.pen = colour(m.pen);
                drawn.brush = colour(m.brush);
            }
            drawn
        })
        .collect();
    if state.drawn.row_count() == rows.len() {
        for (index, row) in rows.into_iter().enumerate() {
            state.drawn.set_row_data(index, row);
        }
    } else {
        state.drawn.set_vec(rows);
    }
    window.set_notice(state.notice.as_str().into());
    window.set_asking(state.error.is_some());
    if let Some(error) = &state.error {
        window.set_asking_title("Clipboard Error!".into());
        window.set_asking_message(error.as_str().into());
    }
}

/// Open the dialog on `files`' ratings; it forgets itself from `slot` when
/// closed, and calls `applied` once the ratings are written.
pub(crate) fn open(
    store: &Arc<Store>,
    files: Vec<HashId>,
    slot: &Rc<RefCell<Option<ManageRatingsWindow>>>,
    editing: &Rc<RefCell<Option<crate::EditValueWindow>>>,
    applied: Rc<dyn Fn()>,
) -> Result<ManageRatingsWindow, String> {
    let services = store.snapshot().services.clone();
    let mut ratings: HashMap<HashId, _> = store
        .read(|c| hydrus_store::media::load(c, &services, None, &files))
        .map_err(|e| e.to_string())?
        .results
        .into_iter()
        .map(|m| (m.hash_id, m.ratings))
        .collect();
    let file_ratings: Vec<_> = files
        .iter()
        .map(|f| ratings.remove(f).unwrap_or_default())
        .collect();
    let window = crate::app_title::new::<crate::ManageRatingsWindow>().map_err(|e| e.to_string())?;
    window.set_window_title(title(files.len()).into());
    let sizes = store
        .read(hydrus_store::settings::get::<hydrus_store::settings::RatingContextSizes>)
        .map_err(|error| error.to_string())?;
    window.set_rating_size(sizes.dialog_icon_size.trunc() as f32);
    window.set_incdec_height(sizes.dialog_incdec_height.trunc() as f32);
    window.set_rating_outline(crate::ratings::outline_width(sizes.dialog_icon_size.trunc()) as f32);
    let drawn = Rc::new(VecModel::default());
    window.set_ratings(ModelRc::from(drawn.clone()));
    let state = Rc::new(RefCell::new(State {
        drawn,
        editor: RatingsEditor::new(&services, &file_ratings),
        notice: String::new(),
        error: None,
    }));
    let refresh = {
        let state = state.clone();
        let weak = window.as_weak();
        Rc::new(move || {
            if let Some(window) = weak.upgrade() {
                show(&window, &state.borrow());
            }
        })
    };
    let close = {
        let slot = slot.clone();
        let weak = window.as_weak();
        Rc::new(move || {
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            slot.borrow_mut().take();
        })
    };
    window.on_rating_clicked({
        let state = state.clone();
        let refresh = refresh.clone();
        move |row, left, proportion| {
            let Ok(row) = usize::try_from(row) else {
                return;
            };
            let mut state = state.borrow_mut();
            if left {
                state.editor.left(row, f64::from(proportion));
            } else {
                state.editor.right(row);
            }
            drop(state);
            refresh();
        }
    });
    // an inc/dec control's middle click: "edit value", its count typed
    let editing = editing.clone();
    window.on_rating_middle({
        let state = state.clone();
        let refresh = refresh.clone();
        let editing = editing.clone();
        move |row| {
            let Ok(row) = usize::try_from(row) else {
                return;
            };
            let Some(count) = state.borrow().editor.count(row) else {
                return;
            };
            let Ok(edit) = crate::app_title::new::<crate::EditValueWindow>() else {
                return;
            };
            edit.set_value(i32::try_from(count).unwrap_or(i32::MAX));
            edit.on_apply({
                let weak = edit.as_weak();
                let state = state.clone();
                let refresh = refresh.clone();
                move || {
                    if let Some(edit) = weak.upgrade() {
                        state
                            .borrow_mut()
                            .editor
                            .set_count(row, i64::from(edit.get_value()));
                        let _ = edit.hide();
                        refresh();
                    }
                }
            });
            edit.on_cancel({
                let weak = edit.as_weak();
                move || {
                    if let Some(edit) = weak.upgrade() {
                        let _ = edit.hide();
                    }
                }
            });
            if edit.show().is_ok() {
                *editing.borrow_mut() = Some(edit);
            }
        }
    });
    window.on_copy({
        let state = state.clone();
        let refresh = refresh.clone();
        move || {
            let (text, notice) = state.borrow().editor.copy();
            crate::copy_to_clipboard(&text);
            state.borrow_mut().notice = notice;
            refresh();
        }
    });
    window.on_paste({
        let state = state.clone();
        let refresh = refresh.clone();
        move || {
            let pasted = crate::from_clipboard();
            let mut state = state.borrow_mut();
            match pasted {
                Ok(text) => match state.editor.paste(&text) {
                    Ok(notice) => state.notice = notice,
                    Err(error) => state.error = Some(error),
                },
                Err(e) => state.error = Some(e),
            }
            drop(state);
            refresh();
        }
    });
    let dismiss = {
        let state = state.clone();
        let refresh = refresh.clone();
        move || {
            state.borrow_mut().error = None;
            refresh();
        }
    };
    window.on_chosen({
        let dismiss = dismiss.clone();
        move |_| dismiss()
    });
    window.on_cancelled(dismiss);
    window.on_apply({
        let state = state.clone();
        let store = store.clone();
        let close = close.clone();
        move || {
            let updates = state.borrow().editor.updates();
            if !updates.is_empty() {
                let files = files.clone();
                let written = store.write_content(move |w| {
                    for (service, update) in &updates {
                        match update {
                            Update::Rating(rating) => w.set_rating(*service, &files, *rating)?,
                            Update::IncDec(value) => w.set_incdec(*service, &files, *value)?,
                        }
                    }
                    Ok(())
                });
                match written {
                    Ok(()) => applied(),
                    Err(e) => eprintln!("could not write the ratings: {e}"),
                }
            }
            close();
        }
    });
    window.on_cancel(move || close());
    window.window().on_close_requested({
        let slot = slot.clone();
        move || {
            slot.borrow_mut().take();
            slint::CloseRequestResponse::HideWindow
        }
    });
    show(&window, &state.borrow());
    window.show().map_err(|e| e.to_string())?;
    Ok(window)
}
