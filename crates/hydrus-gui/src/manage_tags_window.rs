//! The manage tags window, bound to its model ([`ManageTags`]).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};

use crate::manage_tags::ManageTags;
use crate::{ListText, ManageTagsWindow, list_text};

/// Open the window on `model`; it forgets itself from `slot` when closed,
/// and calls `applied` once changes are written.
pub(crate) fn open(
    model: ManageTags,
    slot: &Rc<RefCell<Option<ManageTagsWindow>>>,
    applied: Rc<dyn Fn()>,
) -> Result<ManageTagsWindow, slint::PlatformError> {
    let window = ManageTagsWindow::new()?;
    let names: Vec<SharedString> = model
        .service_names()
        .iter()
        .map(|n| n.as_str().into())
        .collect();
    window.set_service_names(ModelRc::new(VecModel::from(names)));
    let model = Rc::new(RefCell::new(model));
    let refresh = {
        let model = model.clone();
        let weak = window.as_weak();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let model = model.borrow();
            let colours: hydrus_core::tag_presentation::NamespaceColours = model
                .store()
                .read(hydrus_store::settings::get)
                .unwrap_or_default();
            window.set_autocomplete_height(
                i32::try_from(
                    model
                        .autocomplete_options()
                        .autocomplete_list_height
                        .clamp(1, 128),
                )
                .unwrap_or(11),
            );
            window.set_service_index(i32::try_from(model.service()).unwrap_or(0));
            let tags: Vec<ListText> = model
                .rows()
                .iter()
                .map(|(tag, row)| list_text(row, colours.tag(tag)))
                .collect();
            window.set_tags(ModelRc::new(VecModel::from(tags)));
            window
                .set_autocomplete_tab(i32::try_from(model.autocomplete_tab().index()).unwrap_or(0));
            let suggestions: Vec<ListText> = model
                .suggestion_rows()
                .iter()
                .map(|r| list_text(&r.label, colours.tag(&r.colour_tag)))
                .collect();
            window.set_suggestions(ModelRc::new(VecModel::from(suggestions)));
            window.set_highlighted(
                model
                    .highlighted()
                    .and_then(|i| i32::try_from(i).ok())
                    .unwrap_or(-1),
            );
            if window.get_text().as_str() != model.text() {
                window.set_text(model.text().into());
            }
        }
    };
    let active = Rc::new(Cell::new(true));
    let pending_paste: Rc<RefCell<Option<Vec<String>>>> = Rc::default();
    let close = {
        let weak = window.as_weak();
        let slot = slot.clone();
        let active = active.clone();
        let pending_paste = pending_paste.clone();
        move || {
            if !active.replace(false) {
                return;
            }
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            pending_paste.borrow_mut().take();
            slot.borrow_mut().take();
        }
    };
    let migration_slot = crate::tag_migration_window::Slot::default();
    window.on_migrate_tags({
        let model = model.clone();
        let slot = migration_slot.clone();
        let weak = window.as_weak();
        let refresh = refresh.clone();
        let applied = applied.clone();
        move || {
            let m = model.borrow();
            let Some(key) = m.migration_service_key() else {
                if let Some(w) = weak.upgrade() {
                    w.set_error("the tag service was removed; reopen manage tags".into());
                }
                return;
            };
            let changed = Rc::new({
                let model = model.clone();
                let refresh = refresh.clone();
                let applied = applied.clone();
                move || {
                    model.borrow_mut().refresh_stored();
                    refresh();
                    applied();
                }
            });
            if let Err(e) = crate::tag_migration_window::open(
                m.store(),
                &key,
                m.files().to_vec(),
                &slot,
                changed,
            ) && let Some(w) = weak.upgrade()
            {
                w.set_error(e.into());
            }
        }
    });
    let apply = {
        let model = model.clone();
        let weak = window.as_weak();
        let close = close.clone();
        let active = active.clone();
        let pending = pending_paste.clone();
        move || {
            if !active.get() || pending.borrow().is_some() {
                return;
            }
            if let Err(e) = model.borrow().apply() {
                if let Some(window) = weak.upgrade() {
                    window.set_error(format!("could not apply the changes: {e}").into());
                }
                return;
            }
            applied();
            close();
        }
    };
    window.on_service_chosen({
        let model = model.clone();
        let refresh = refresh.clone();
        let weak = window.as_weak();
        let active = active.clone();
        let pending = pending_paste.clone();
        move |i| {
            if !active.get() || pending.borrow().is_some() {
                return;
            }
            if let Err(error) = model
                .borrow_mut()
                .choose_service(usize::try_from(i).unwrap_or(usize::MAX))
                && let Some(window) = weak.upgrade()
            {
                window.set_error(format!("could not remember the tag service: {error}").into());
            }
            refresh();
        }
    });
    window.on_refresh_autocomplete({
        let model = model.clone();
        let refresh = refresh.clone();
        let active = active.clone();
        let pending = pending_paste.clone();
        move || {
            if !active.get() || pending.borrow().is_some() {
                return;
            }
            let mut model = model.borrow_mut();
            let text = model.text().to_owned();
            model.set_text(&text);
            drop(model);
            refresh();
        }
    });
    window.on_tab_chosen({
        let model = model.clone();
        let refresh = refresh.clone();
        let active = active.clone();
        let pending = pending_paste.clone();
        move |i| {
            if !active.get() || pending.borrow().is_some() {
                return;
            }
            model.borrow_mut().choose_autocomplete_tab(
                hydrus_gui_model::write_autocomplete::Tab::from_index(
                    usize::try_from(i).unwrap_or(0),
                ),
            );
            refresh();
        }
    });
    window.on_fetch({
        let model = model.clone();
        let refresh = refresh.clone();
        let active = active.clone();
        let pending = pending_paste.clone();
        move || {
            if !active.get() || pending.borrow().is_some() {
                return;
            }
            model.borrow_mut().fetch();
            refresh();
        }
    });
    window.on_text_edited({
        let model = model.clone();
        let refresh = refresh.clone();
        let active = active.clone();
        let pending = pending_paste.clone();
        move |text| {
            if !active.get() || pending.borrow().is_some() {
                return;
            }
            model.borrow_mut().set_text(&text);
            refresh();
        }
    });
    window.on_move_highlight({
        let model = model.clone();
        let refresh = refresh.clone();
        let active = active.clone();
        let pending = pending_paste.clone();
        move |by| {
            if !active.get() || pending.borrow().is_some() {
                return;
            }
            model.borrow_mut().move_highlight(by as isize);
            refresh();
        }
    });
    window.on_entered({
        let model = model.clone();
        let weak = window.as_weak();
        let refresh = refresh.clone();
        let apply = apply.clone();
        let active = active.clone();
        let pending = pending_paste.clone();
        move || {
            if !active.get() || pending.borrow().is_some() {
                return;
            }
            // (enter with nothing typed applies, as in the reference)
            let empty = model.borrow().text().trim().is_empty();
            if empty {
                apply();
                return;
            }
            let entered = model.borrow_mut().enter_input();
            if let Some(window) = weak.upgrade() {
                window.set_error(entered.err().unwrap_or_default().into());
            }
            refresh();
        }
    });
    window.on_tag_activated({
        let model = model.clone();
        let refresh = refresh.clone();
        let active = active.clone();
        let pending = pending_paste.clone();
        move |i| {
            if !active.get() || pending.borrow().is_some() {
                return;
            }
            model
                .borrow_mut()
                .toggle_row(usize::try_from(i).unwrap_or(usize::MAX));
            refresh();
        }
    });
    window.on_suggestion_chosen({
        let model = model.clone();
        let refresh = refresh.clone();
        let active = active.clone();
        let pending = pending_paste.clone();
        move |i| {
            if !active.get() || pending.borrow().is_some() {
                return;
            }
            let _ = model
                .borrow_mut()
                .choose_suggestion(usize::try_from(i).unwrap_or(usize::MAX));
            refresh();
        }
    });
    window.on_paste_requested({
        let model = model.clone();
        let refresh = refresh.clone();
        let active = active.clone();
        let pending = pending_paste.clone();
        let weak = window.as_weak();
        move |button| {
            if !active.get() || pending.borrow().is_some() {
                return true;
            }
            let Some(w) = weak.upgrade() else {
                return true;
            };
            let text = match crate::from_clipboard() {
                Ok(text) => text,
                Err(error) => {
                    w.set_error(error.into());
                    return true;
                }
            };
            let decision = hydrus_gui_model::write_autocomplete::paste(
                &text,
                button,
                &model.borrow().autocomplete_options(),
            );
            match decision {
                hydrus_gui_model::write_autocomplete::Paste::Text => false,
                hydrus_gui_model::write_autocomplete::Paste::Confirm { message, tags } => {
                    *pending.borrow_mut() = Some(tags);
                    w.set_question(message.into());
                    true
                }
                hydrus_gui_model::write_autocomplete::Paste::Tags(tags) => {
                    // Release the options borrow before staging tags.
                    let result = model.borrow_mut().paste_tags(&tags);
                    w.set_error(result.err().unwrap_or_default().into());
                    refresh();
                    true
                }
            }
        }
    });
    window.on_paste_answered({
        let model = model.clone();
        let refresh = refresh.clone();
        let active = active.clone();
        let pending = pending_paste.clone();
        let weak = window.as_weak();
        move |yes| {
            if !active.get() {
                return;
            }
            let Some(tags) = pending.borrow_mut().take() else {
                return;
            };
            if let Some(w) = weak.upgrade() {
                w.set_question("".into());
                if yes {
                    w.set_error(
                        model
                            .borrow_mut()
                            .paste_tags(&tags)
                            .err()
                            .unwrap_or_default()
                            .into(),
                    );
                }
            }
            refresh();
        }
    });
    window.on_apply(apply);
    window.on_cancel(close.clone());
    window.window().on_close_requested(move || {
        close();
        slint::CloseRequestResponse::HideWindow
    });
    refresh();
    window.show()?;
    Ok(window)
}
