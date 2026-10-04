//! Detached tag lists for import additional-tags/whitelists, sharing write autocomplete.
use crate::WriteTagsWindow;
use hydrus_core::ServiceKey;
use hydrus_gui_model::write_autocomplete::{Paste, TagEntry, WriteAutocomplete};
use hydrus_store::Store;
use slint::{ComponentHandle as _, ModelRc, VecModel};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

pub type Slot = Rc<RefCell<Option<WriteTagsWindow>>>;

thread_local! {
    static LAST: RefCell<Option<slint::Weak<WriteTagsWindow>>> = const { RefCell::new(None) };
}
/// Most recently opened visible shared write-tag editor.
pub fn last_opened() -> Option<WriteTagsWindow> {
    LAST.with(|last| last.borrow().as_ref().and_then(slint::Weak::upgrade))
        .filter(|w| w.window().is_visible())
}

pub fn open(
    store: &Arc<Store>,
    service: ServiceKey,
    initial: &[String],
    title: &str,
    slot: &Slot,
    applied: Rc<dyn Fn(Vec<String>)>,
    closed: Rc<dyn Fn()>,
) -> Result<WriteTagsWindow, slint::PlatformError> {
    open_internal(
        store,
        (service, false),
        initial,
        title,
        slot,
        Rc::new(move |tags, _| applied(tags)),
        closed,
    )
}

/// Additive filename-tagging choices, with explicit additions for a per-file
/// owner. An unchanged selected union must not spread existing tags to all files.
pub fn open_additions(
    store: &Arc<Store>,
    service: ServiceKey,
    initial: &[String],
    title: &str,
    slot: &Slot,
    applied: Rc<dyn Fn(Vec<String>, Vec<String>)>,
    closed: Rc<dyn Fn()>,
) -> Result<WriteTagsWindow, slint::PlatformError> {
    open_internal(
        store,
        (service, true),
        initial,
        title,
        slot,
        applied,
        closed,
    )
}

/// The options favourite list adds manual choices, instead of toggling existing tags.
pub fn open_favourites(
    store: &Arc<Store>,
    initial: &[String],
    slot: &Slot,
    applied: Rc<dyn Fn(Vec<String>)>,
) -> Result<WriteTagsWindow, slint::PlatformError> {
    open_internal(
        store,
        (
            ServiceKey::new(hydrus_core::service::builtin_keys::COMBINED_TAG),
            true,
        ),
        initial,
        "edit favourite tags",
        slot,
        Rc::new(move |tags, _| applied(tags)),
        Rc::new(|| {}),
    )
}

fn open_internal(
    store: &Arc<Store>,
    (service, add_only): (ServiceKey, bool),
    initial: &[String],
    title: &str,
    slot: &Slot,
    applied: Rc<dyn Fn(Vec<String>, Vec<String>)>,
    closed: Rc<dyn Fn()>,
) -> Result<WriteTagsWindow, slint::PlatformError> {
    if let Some(existing) = slot.borrow().as_ref() {
        existing.show()?;
        return Ok(existing.clone_strong());
    }
    let window = WriteTagsWindow::new()?;
    window
        .global::<crate::TagTextHistory>()
        .on_record(crate::write_tag_history::record);
    window
        .global::<crate::TagTextHistory>()
        .on_undo(crate::write_tag_history::undo);
    window
        .global::<crate::TagTextHistory>()
        .on_redo(crate::write_tag_history::redo);
    window.set_window_title(title.into());
    let location = store
        .read(hydrus_store::settings::get::<hydrus_store::settings::SearchDefaults>)
        .unwrap_or_default()
        .local_location;
    let entry = TagEntry::new(
        WriteAutocomplete::new(store.clone(), service, location),
        initial,
    );
    let entry = if add_only {
        entry.additions_only()
    } else {
        entry
    };
    let model = Rc::new(RefCell::new(entry));
    let pending: Rc<RefCell<Option<Vec<String>>>> = Rc::default();
    let active = Rc::new(Cell::new(true));
    let refresh = Rc::new({
        let model = model.clone();
        let store = store.clone();
        let weak = window.as_weak();
        move || {
            let Some(w) = weak.upgrade() else {
                return;
            };
            let m = model.borrow();
            let (file, tags) = m.input.domain_labels();
            w.set_file_label(file.into());
            w.set_tag_label(tags.into());
            let presentation: hydrus_core::tag_presentation::TagPresentation =
                store.read(hydrus_store::settings::get).unwrap_or_default();
            let colours: hydrus_core::tag_presentation::NamespaceColours =
                store.read(hydrus_store::settings::get).unwrap_or_default();
            w.set_tags(ModelRc::new(VecModel::from(
                m.tags()
                    .iter()
                    .map(|tag| crate::list_text(&presentation.render(tag), colours.tag(tag)))
                    .collect::<Vec<_>>(),
            )));
            w.set_suggestions(ModelRc::new(VecModel::from(
                m.input
                    .rows()
                    .iter()
                    .map(|r| crate::list_text(&r.label, colours.tag(&r.colour_tag)))
                    .collect::<Vec<_>>(),
            )));
            w.set_selected(ModelRc::new(VecModel::from(m.input.selection_mask())));
            w.set_tab_index(i32::try_from(m.input.tab().index()).unwrap_or(0));
            w.set_highlighted(
                m.input
                    .highlighted()
                    .and_then(|i| i32::try_from(i).ok())
                    .unwrap_or(-1),
            );
            w.set_list_height(
                i32::try_from(m.input.options().autocomplete_list_height.clamp(1, 128))
                    .unwrap_or(11),
            );
            w.set_text(m.input.text().into());
        }
    });
    let editable = Rc::new({
        let active = active.clone();
        let pending = pending.clone();
        move || active.get() && pending.borrow().is_none()
    });
    let tag_menu = crate::write_tag_menu::TagMenu::new(
        store.clone(),
        editable.clone(),
        Rc::new({
            let model = model.clone();
            move |action| {
                if let hydrus_gui_model::write_tag_menu::Action::Decorate { tab, kind, value } =
                    action
                {
                    model.borrow_mut().input.decorate(tab, kind, value);
                } else if let hydrus_gui_model::write_tag_menu::Action::Domain(choice) = action {
                    model.borrow_mut().input.choose_domain(choice);
                }
            }
        }),
        Rc::new({
            let model = model.clone();
            let refresh = refresh.clone();
            move || {
                model.borrow_mut().input.fetch();
                refresh();
            }
        }),
        Rc::new({
            let weak = window.as_weak();
            move |question| {
                if let Some(w) = weak.upgrade() {
                    w.set_tag_menu_question(question.into());
                }
            }
        }),
        Rc::new({
            let weak = window.as_weak();
            move |error| {
                if let Some(w) = weak.upgrade() {
                    w.set_error(error.into());
                }
            }
        }),
    );
    crate::write_tag_menu::bind!(window, tag_menu);
    window.on_context_menu({
        let tag_menu = tag_menu.clone();
        let model = model.clone();
        let refresh = refresh.clone();
        let editable = editable.clone();
        move |i, x, y| {
            if !editable() || tag_menu.busy() {
                return;
            }
            if let Ok(i) = usize::try_from(i) {
                let entries = {
                    let mut model = model.borrow_mut();
                    model.input.click(i, false, false);
                    model.input.menu(i)
                };
                refresh();
                tag_menu.open(&entries, x, y);
            }
        }
    });
    window.on_domain_menu({
        let menu = tag_menu.clone();
        let model = model.clone();
        move |tags, x, y| {
            let entries = model.borrow().input.domain_menu(tags);
            menu.open(&entries, x, y);
        }
    });
    let editable = Rc::new({
        let editable = editable.clone();
        let tag_menu = tag_menu.clone();
        move || editable() && !tag_menu.busy()
    });
    let close = Rc::new({
        let active = active.clone();
        let pending = pending.clone();
        let weak = window.as_weak();
        let slot = slot.clone();
        let tag_menu = tag_menu.clone();
        move || {
            if !active.replace(false) {
                return;
            }
            tag_menu.close();
            pending.borrow_mut().take();
            if let Some(w) = weak.upgrade() {
                let _ = w.hide();
            }
            slot.borrow_mut().take();
            closed();
        }
    });
    window.on_edited({
        let model = model.clone();
        let refresh = refresh.clone();
        let editable = editable.clone();
        move |text| {
            if !editable() {
                return;
            }
            model.borrow_mut().input.set_text(&text);
            refresh();
        }
    });
    window.on_tab_chosen({
        let model = model.clone();
        let refresh = refresh.clone();
        let editable = editable.clone();
        move |i| {
            if !editable() {
                return;
            }
            model.borrow_mut().input.set_tab(
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
        let editable = editable.clone();
        move || {
            if !editable() {
                return;
            }
            model.borrow_mut().input.fetch();
            refresh();
        }
    });
    window.on_move_highlight({
        let model = model.clone();
        let refresh = refresh.clone();
        let editable = editable.clone();
        move |by| {
            if !editable() {
                return;
            }
            model.borrow_mut().input.move_highlight(by as isize);
            refresh();
        }
    });
    window.on_navigate({
        let model = model.clone();
        let refresh = refresh.clone();
        let editable = editable.clone();
        move |direction, ctrl, shift| {
            if !editable() {
                return;
            }
            model.borrow_mut().input.navigate(direction, ctrl, shift);
            refresh();
        }
    });
    window.on_results_action({
        let model = model.clone();
        let refresh = refresh.clone();
        let editable = editable.clone();
        move |action| {
            if !editable() {
                return false;
            }
            let handled =
                crate::write_tag_menu::results_action(&mut model.borrow_mut().input, action);
            refresh();
            handled
        }
    });
    window.on_selection_clicked({
        let model = model.clone();
        let refresh = refresh.clone();
        let editable = editable.clone();
        move |i, ctrl, shift| {
            if !editable() {
                return;
            }
            if let Ok(i) = usize::try_from(i) {
                model.borrow_mut().input.click(i, ctrl, shift);
                refresh();
            }
        }
    });
    window.on_entered({
        let model = model.clone();
        let refresh = refresh.clone();
        let editable = editable.clone();
        move || {
            if !editable() {
                return;
            }
            model.borrow_mut().enter(None);
            refresh();
        }
    });
    window.on_chosen({
        let model = model.clone();
        let refresh = refresh.clone();
        let editable = editable.clone();
        move |i| {
            if !editable() {
                return;
            }
            if let Ok(i) = usize::try_from(i) {
                model.borrow_mut().enter(Some(i));
            }
            refresh();
        }
    });
    window.on_remove({
        let model = model.clone();
        let refresh = refresh.clone();
        let editable = editable.clone();
        move |i| {
            if !editable() {
                return;
            }
            model
                .borrow_mut()
                .remove(usize::try_from(i).unwrap_or(usize::MAX));
            refresh();
        }
    });
    window.on_paste({
        let model = model.clone();
        let refresh = refresh.clone();
        let editable = editable.clone();
        let pending = pending.clone();
        let weak = window.as_weak();
        move |button| {
            if !editable() {
                return true;
            }
            let Some(w) = weak.upgrade() else {
                return true;
            };
            let text = match crate::from_clipboard() {
                Ok(t) => t,
                Err(e) => {
                    w.set_error(e.into());
                    return true;
                }
            };
            let decision = hydrus_gui_model::write_autocomplete::paste(
                &text,
                button,
                &model.borrow().input.options(),
            );
            match decision {
                Paste::Text => false,
                Paste::Confirm { message, tags } => {
                    *pending.borrow_mut() = Some(tags);
                    w.set_question(message.into());
                    true
                }
                Paste::Tags(tags) => {
                    model.borrow_mut().paste(&tags);
                    refresh();
                    true
                }
            }
        }
    });
    window.on_answered({
        let model = model.clone();
        let refresh = refresh.clone();
        let active = active.clone();
        let pending = pending.clone();
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
            }
            if yes {
                model.borrow_mut().paste(&tags);
            } else if let Some(w) = weak.upgrade() {
                w.invoke_normal_paste();
            }
            refresh();
        }
    });
    window.on_apply({
        let model = model.clone();
        let editable = editable.clone();
        let close = close.clone();
        move || {
            if !editable() {
                return;
            }
            let model = model.borrow();
            applied(model.tags(), model.additions());
            close();
        }
    });
    window.on_cancel({
        let close = close.clone();
        move || close()
    });
    window.window().on_close_requested(move || {
        close();
        slint::CloseRequestResponse::HideWindow
    });
    refresh();
    window.show()?;
    LAST.with(|last| *last.borrow_mut() = Some(window.as_weak()));
    *slot.borrow_mut() = Some(window.clone_strong());
    Ok(window)
}
