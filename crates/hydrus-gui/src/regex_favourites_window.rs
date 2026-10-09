//! Regex favourites editor shared by the options draft and regex controls.
use crate::{RegexFavouritesWindow, TableRow};
use hydrus_gui_model::regex_favourites::{Editor, RegexFavourites, validity};
use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

/// A favourite editor owned and canceled by its parent window.
pub type Slot = Rc<RefCell<Option<RegexFavouritesWindow>>>;
/// Accept a complete favourites draft; errors leave it open for correction.
pub type Applied = Rc<dyn Fn(RegexFavourites) -> Result<(), String>>;
/// Saved preferences owned by the caller, deliberately separate from the draft.
pub type SavedFavourites = Rc<dyn Fn() -> Result<RegexFavourites, String>>;
thread_local! { static LAST: RefCell<Option<slint::Weak<RegexFavouritesWindow>>> = const { RefCell::new(None) }; }

/// Last visible editor, for rendered integration inspection.
pub fn last_opened() -> Option<RegexFavouritesWindow> {
    LAST.with(|last| last.borrow().as_ref().and_then(slint::Weak::upgrade))
        .filter(|w| w.window().is_visible())
}
/// Whether the parent's favourites editor currently blocks draft edits.
pub fn has_open(slot: &Slot) -> bool {
    slot.borrow().is_some()
}
/// Cancel the editor and invalidate callbacks on retained handles.
pub fn cancel(slot: &Slot) {
    let window = slot
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong);
    if let Some(window) = window {
        window.invoke_action("cancel".into());
    }
}

fn show(window: &RegexFavouritesWindow, editor: &Editor) {
    let selected = editor.selected();
    let rows = editor
        .rows()
        .into_iter()
        .enumerate()
        .map(|(i, (phrase, description))| TableRow {
            cells: ModelRc::new(VecModel::from(vec![
                SharedString::from(phrase),
                SharedString::from(description),
            ])),
            selected: selected.contains(&i),
        })
        .collect::<Vec<_>>();
    window.set_rows(ModelRc::new(VecModel::from(rows)));
    window.set_any_selected(!selected.is_empty());
    window.set_one_selected(selected.len() == 1);
}

fn show_validity(window: &RegexFavouritesWindow) {
    let result = validity(window.get_phrase().as_str());
    window.set_valid(result.is_ok());
    window.set_validity(
        result
            .map_or_else(
                |reason| format!("Invalid expression: {reason}"),
                |()| "Regex compiles.".into(),
            )
            .into(),
    );
}

/// Open on the supplied favourites; Apply passes its sorted draft to the owner.
pub fn open(
    value: &RegexFavourites,
    slot: &Slot,
    applied: Applied,
) -> Result<RegexFavouritesWindow, slint::PlatformError> {
    let saved = value.clone();
    open_owned(
        value,
        slot,
        applied,
        Rc::new(move || Ok(saved.clone())),
        Rc::new(|| true),
    )
}

/// Open with explicit saved-input access and an owner lifetime guard.
pub fn open_owned(
    value: &RegexFavourites,
    slot: &Slot,
    applied: Applied,
    saved: SavedFavourites,
    owner_valid: Rc<dyn Fn() -> bool>,
) -> Result<RegexFavouritesWindow, slint::PlatformError> {
    if !owner_valid() {
        return Err(slint::PlatformError::Other(
            "The regex input owner is unavailable.".into(),
        ));
    }
    if let Some(window) = slot.borrow().as_ref() {
        return Ok(window.clone_strong());
    }
    let window = crate::app_title::new::<crate::RegexFavouritesWindow>()?;
    let editor = Rc::new(RefCell::new(Editor::new(value)));
    let editing = Rc::new(Cell::new(None::<usize>));
    let active = Rc::new(Cell::new(true));
    let popup = crate::popup_menu::Popup::new();
    window.set_favourite_panes(popup.model());
    let valid: Rc<dyn Fn() -> bool> = Rc::new({
        let active = active.clone();
        let weak = window.as_weak();
        move || {
            active.get() && owner_valid() && weak.upgrade().is_some_and(|w| w.window().is_visible())
        }
    });
    window.on_favourite_menu({
        let valid = valid.clone();
        let weak = window.as_weak();
        let popup = popup.clone();
        move |x, y| {
            let Some(window) = weak.upgrade() else { return };
            if !valid() || !window.get_editing() || window.get_deleting() {
                popup.close();
                return;
            }
            popup.close();
            match saved() {
                Ok(value) => {
                    let (entries, actions) = hydrus_gui_model::regex_favourites::input_menu(&value);
                    popup.open(entries, actions, x, y);
                }
                Err(error) => window.set_error(error.into()),
            }
        }
    });
    window.on_favourite_line_hovered({
        let popup = popup.clone();
        move |p, l, r, t, left| popup.hover(p, l, r, t, left)
    });
    window.on_favourite_placed({
        let popup = popup.clone();
        move |p, x, y, w| popup.placed(p, x, y, w)
    });
    window.on_favourite_dismissed({
        let popup = popup.clone();
        move || popup.close()
    });
    window.on_favourite_line_clicked({
        let popup = popup.clone();
        let valid = valid.clone();
        let weak = window.as_weak();
        move |p, l, r, t, left| {
            let Some(window) = weak.upgrade() else { return };
            if !valid() || !window.get_editing() || window.get_deleting() {
                popup.close();
                return;
            }
            if let Some(crate::popup_menu::Chosen::Action(
                hydrus_gui_model::regex_favourites::MenuAction::Copy(phrase),
            )) = popup.click(p, l, r, t, left)
            {
                crate::copy_to_clipboard(&phrase);
            }
        }
    });
    let close: Rc<dyn Fn()> = Rc::new({
        let slot = Rc::downgrade(slot);
        let weak = window.as_weak();
        let active = active.clone();
        let popup = popup.clone();
        move || {
            if !active.replace(false) {
                return;
            }
            popup.close();
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            if let Some(slot) = slot.upgrade() {
                slot.borrow_mut().take();
            }
            if let Some(window) = weak.upgrade() {
                window.invoke_closed();
            }
        }
    });
    window.on_row_clicked({
        let weak = window.as_weak();
        let editor = editor.clone();
        let valid = valid.clone();
        move |i, ctrl, shift| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            if !valid() || window.get_editing() || window.get_deleting() {
                return;
            }
            if let Ok(i) = usize::try_from(i) {
                editor.borrow_mut().click(i, ctrl, shift);
            }
            show(&window, &editor.borrow());
        }
    });
    window.on_row_activated({
        let weak = window.as_weak();
        move |i| {
            if let Some(window) = weak.upgrade() {
                window.invoke_row_clicked(i, false, false);
                window.invoke_action("edit".into());
            }
        }
    });
    window.on_changed({
        let weak = window.as_weak();
        let valid = valid.clone();
        move || {
            if valid()
                && let Some(window) = weak.upgrade()
            {
                show_validity(&window);
            }
        }
    });
    window.on_action({
        let weak = window.as_weak();
        let editor = editor.clone();
        let active = active.clone();
        let close = close.clone();
        let valid = valid.clone();
        let popup = popup.clone();
        move |action| {
            if !active.get() {
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            if action != "cancel" && !valid() {
                popup.close();
                return;
            }
            popup.close();
            let result = (|| -> Result<(), String> {
                match action.as_str() {
                    "cancel" => close(),
                    "cancel-row" => {
                        window.set_editing(false);
                        editing.set(None);
                    }
                    "cancel-delete" => window.set_deleting(false),
                    "save-row" if window.get_editing() => {
                        let phrase = window.get_phrase().to_string();
                        let description = window.get_description().to_string();
                        hydrus_gui_model::regex_favourites::description_validity(&description)?;
                        match editing.get() {
                            Some(id) => editor.borrow_mut().replace(id, phrase, description),
                            None => editor.borrow_mut().add(phrase, description)?,
                        }
                        window.set_editing(false);
                        editing.set(None);
                    }
                    "confirm-delete" if window.get_deleting() => {
                        editor.borrow_mut().delete();
                        window.set_deleting(false);
                    }
                    _ if window.get_editing() || window.get_deleting() => {}
                    "add" => {
                        editing.set(None);
                        window.set_phrase("".into());
                        window.set_description("".into());
                        window.set_editing(true);
                        show_validity(&window);
                    }
                    "edit" => {
                        if let Some((id, (phrase, description))) = editor.borrow().editing() {
                            editing.set(Some(id));
                            window.set_phrase(phrase.into());
                            window.set_description(description.into());
                            window.set_editing(true);
                            show_validity(&window);
                        }
                    }
                    "delete" if !editor.borrow().selected().is_empty() => window.set_deleting(true),
                    "copy" => {
                        if let Some((_, (phrase, _))) = editor.borrow().editing() {
                            crate::copy_to_clipboard(&phrase);
                        }
                    }
                    "apply" => {
                        applied(editor.borrow().value())?;
                        close();
                    }
                    _ => {}
                }
                Ok(())
            })();
            window.set_error(result.err().unwrap_or_default().into());
            show(&window, &editor.borrow());
        }
    });
    window.window().on_close_requested(move || {
        close();
        slint::CloseRequestResponse::HideWindow
    });
    show(&window, &editor.borrow());
    window.show()?;
    *slot.borrow_mut() = Some(window.clone_strong());
    LAST.with(|last| *last.borrow_mut() = Some(window.as_weak()));
    Ok(window)
}
