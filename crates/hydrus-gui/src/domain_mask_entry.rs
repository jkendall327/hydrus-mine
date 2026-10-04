//! Owned domain-mask entry and deletion dialogs, including advisory regex tools.
use crate::{
    SessionDialog,
    popup_menu::{Chosen, Popup},
};
use hydrus_gui_model::downloader_definitions::DefinitionEditor;
use hydrus_gui_model::regex_favourites::{self, MenuAction};
use hydrus_store::Store;
use slint::{ComponentHandle as _, ModelRc, VecModel};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
    sync::Arc,
};

/// Entry and nested favourites belong to one class editor.
#[derive(Clone, Default)]
pub struct Slots {
    pub entry: Rc<RefCell<Option<SessionDialog>>>,
    pub favourites: crate::regex_favourites_window::Slot,
}
impl std::fmt::Debug for Slots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Slots")
            .field("open", &self.has_open())
            .finish_non_exhaustive()
    }
}
impl Slots {
    pub fn has_open(&self) -> bool {
        self.entry.borrow().is_some()
    }
    pub fn cancel(&self) {
        let window = self
            .entry
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong);
        if let Some(window) = window {
            window.invoke_force_close();
        }
    }
}
type Done = Rc<dyn Fn(Option<String>)>;

fn open(
    store: &Arc<Store>,
    slots: &Slots,
    regex: bool,
    value: Option<String>,
    question: Option<String>,
    done: Done,
) -> Result<(), String> {
    if slots.has_open() {
        return Ok(());
    }
    let window = SessionDialog::new().map_err(|e| e.to_string())?;
    window.set_asking_name(question.is_none());
    window.set_window_title(
        if question.is_some() {
            "Are you sure?"
        } else if regex {
            "Enter domain regex."
        } else {
            "Enter Text"
        }
        .into(),
    );
    window.set_message(
        question
            .unwrap_or_else(|| {
                if regex {
                    String::new()
                } else {
                    "Enter domain.".into()
                }
            })
            .into(),
    );
    window.set_text(value.unwrap_or_default().into());
    window.set_placeholder(if regex { "" } else { "example.com" }.into());
    window.set_regex_mode(regex);
    window.set_regex_components(ModelRc::new(VecModel::from(
        regex_favourites::regex_tools(1)
            .into_iter()
            .map(|(label, _)| label.into())
            .collect::<Vec<slint::SharedString>>(),
    )));
    let active = Rc::new(Cell::new(true));
    let popup = Popup::<MenuAction>::new();
    window.set_favourite_panes(popup.model());
    let close: Rc<dyn Fn(Option<String>)> = Rc::new({
        let active = active.clone();
        let weak = window.as_weak();
        let slots = slots.clone();
        let popup = popup.clone();
        move |value| {
            if !active.replace(false) {
                return;
            }
            popup.close();
            crate::regex_favourites_window::cancel(&slots.favourites);
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            slots.entry.borrow_mut().take();
            done(value);
        }
    });
    window.on_name_entered({
        let close = close.clone();
        let active = active.clone();
        let slots = slots.clone();
        let weak = window.as_weak();
        move |text| {
            if !regex && text.is_empty() {
                if active.get()
                    && let Some(window) = weak.upgrade()
                {
                    window.set_warning("Cannot enter blank text here!".into());
                }
                return;
            }
            if active.get() && !crate::regex_favourites_window::has_open(&slots.favourites) {
                let text = text.trim();
                close((!text.is_empty()).then(|| text.to_owned()));
            }
        }
    });
    window.on_cancelled({
        let close = close.clone();
        let slots = slots.clone();
        move || {
            if !crate::regex_favourites_window::has_open(&slots.favourites) {
                close(None);
            }
        }
    });
    window.on_answered({
        let close = close.clone();
        move |yes| close(yes.then(String::new))
    });
    window.on_force_close({
        let close = close.clone();
        move || close(None)
    });
    window.window().on_close_requested({
        let close = close.clone();
        move || {
            close(None);
            slint::CloseRequestResponse::KeepWindowShown
        }
    });
    window.on_regex_changed({
        let weak = window.as_weak();
        let active = active.clone();
        move || {
            if active.get()
                && let Some(window) = weak.upgrade()
            {
                window.set_regex_validity(
                    regex_favourites::validity(window.get_text().as_str())
                        .map_or_else(
                            |e| format!("Invalid expression: {e}"),
                            |()| "Regex compiles.".into(),
                        )
                        .into(),
                );
            }
        }
    });
    window.invoke_regex_changed();
    window.on_regex_tool({
        let active = active.clone();
        move |category, index| {
            if active.get()
                && let (Ok(category), Ok(index)) =
                    (usize::try_from(category), usize::try_from(index))
                && let Some((_, value)) = regex_favourites::regex_tools(category).get(index)
            {
                crate::copy_to_clipboard(value);
            }
        }
    });
    window.on_favourite_menu({
        let active = active.clone();
        let store = store.clone();
        let popup = popup.clone();
        let slots = slots.clone();
        move |x, y| {
            if active.get()
                && !crate::regex_favourites_window::has_open(&slots.favourites)
                && let Ok(value) = store.read(hydrus_store::regex_favourites::load)
            {
                let (entries, actions) = regex_favourites::menu(&value);
                popup.open(entries, actions, x, y);
            }
        }
    });
    window.on_favourite_placed({
        let popup = popup.clone();
        move |p, x, y, w| popup.placed(p, x, y, w)
    });
    window.on_favourite_line_hovered({
        let popup = popup.clone();
        move |p, l, r, t, left| popup.hover(p, l, r, t, left)
    });
    window.on_favourite_dismissed({
        let popup = popup.clone();
        move || popup.close()
    });
    window.on_favourite_line_clicked({
        let active = active.clone();
        let slots = slots.clone();
        let store = store.clone();
        let popup = popup.clone();
        let weak = window.as_weak();
        move |p, l, r, t, left| {
            if !active.get() || crate::regex_favourites_window::has_open(&slots.favourites) {
                popup.close();
                return;
            }
            match popup.click(p, l, r, t, left) {
                Some(Chosen::Action(MenuAction::Copy(value))) => {
                    crate::copy_to_clipboard(&value);
                }
                Some(Chosen::Action(MenuAction::Manage)) => {
                    let Ok(value) = store.read(hydrus_store::regex_favourites::load) else {
                        return;
                    };
                    let applied: crate::regex_favourites_window::Applied = Rc::new({
                        let store = store.clone();
                        let active = active.clone();
                        move |value| {
                            if !active.get() {
                                return Err("The regex input has closed.".into());
                            }
                            store
                                .write_and_refresh(move |ctx| {
                                    hydrus_store::settings::set(ctx.conn(), &value)
                                })
                                .map_err(|e| e.to_string())
                        }
                    });
                    if let Ok(child) =
                        crate::regex_favourites_window::open(&value, &slots.favourites, applied)
                        && let Some(window) = weak.upgrade()
                    {
                        window.set_child_open(true);
                        let weak = window.as_weak();
                        child.on_closed(move || {
                            if let Some(window) = weak.upgrade() {
                                window.set_child_open(false);
                            }
                        });
                    }
                }
                _ => (),
            }
        }
    });
    window.show().map_err(|e| e.to_string())?;
    *slots.entry.borrow_mut() = Some(window);
    Ok(())
}

/// Edit each selected row in order; Cancel stops the remaining dialogs.
pub(crate) fn edit(
    store: &Arc<Store>,
    slots: &Slots,
    editor: &Rc<RefCell<DefinitionEditor>>,
    regex: bool,
    indices: VecDeque<Option<usize>>,
    alive: Rc<dyn Fn() -> bool>,
    refresh: Rc<dyn Fn(bool)>,
) -> Result<(), String> {
    if !alive() {
        return Ok(());
    }
    let mut indices = indices;
    let Some(index) = indices.pop_front() else {
        return Ok(());
    };
    let value = index.and_then(|i| editor.borrow().domain_values(regex).get(i).cloned());
    let done: Done = Rc::new({
        let store = store.clone();
        let slots = slots.clone();
        let editor = editor.clone();
        move |value| {
            if !alive() {
                return;
            }
            if let Some(value) = value {
                editor.borrow_mut().domain_put(regex, index, &value);
                refresh(true);
                let _ = edit(
                    &store,
                    &slots,
                    &editor,
                    regex,
                    indices.clone(),
                    alive.clone(),
                    refresh.clone(),
                );
                refresh(false);
            } else {
                refresh(true);
            }
        }
    });
    open(store, slots, regex, value, None, done)
}
/// Ask the reference's counted question before removing any selected rows.
pub(crate) fn delete(
    store: &Arc<Store>,
    slots: &Slots,
    editor: &Rc<RefCell<DefinitionEditor>>,
    regex: bool,
    alive: Rc<dyn Fn() -> bool>,
    refresh: Rc<dyn Fn(bool)>,
) -> Result<(), String> {
    if !alive() {
        return Ok(());
    }
    let selected = editor.borrow().domain_selected(regex);
    if selected.is_empty() {
        return Ok(());
    }
    let question = format!(
        "Remove {} selected?",
        hydrus_core::numbers::human_int(u64::try_from(selected.len()).unwrap_or(u64::MAX))
    );
    let done: Done = Rc::new({
        let editor = editor.clone();
        move |value| {
            if !alive() {
                return;
            }
            if value.is_some() {
                editor.borrow_mut().domain_remove(regex, &selected);
            }
            refresh(true);
        }
    });
    open(store, slots, false, None, Some(question), done)
}
