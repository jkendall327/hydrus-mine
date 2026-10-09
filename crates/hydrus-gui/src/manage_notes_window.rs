//! The "manage notes" dialog, bound (`ui/manage_notes.slint`): a file's
//! notes read from the store into hydrus-gui-model's
//! [`NotesEditor`](crate::notes_editor::NotesEditor), its questions asked in
//! the window's own panel, and "apply" writing the notes to set and delete,
//! as the reference's `EditFileNotes` does.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};

use hydrus_core::HashId;
use hydrus_store::Store;

use crate::ManageNotesWindow;
use crate::notes_editor::{CANCEL_QUESTION, DELETE_QUESTION, NAME_PROMPT, NotesEditor, TITLE};

/// What the window's panel asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Question {
    /// A new note's name.
    Add,
    /// A tab's new name.
    Rename(usize),
    Delete,
    /// Cancelling with changes made.
    Cancel,
}

struct State {
    editor: NotesEditor,
    asking: Option<Question>,
    notice: String,
    selections: Vec<(i32, i32)>,
}

fn initial_selection(text: &str, at_end: bool) -> (i32, i32) {
    let position = if at_end {
        i32::try_from(text.len()).unwrap_or(i32::MAX)
    } else {
        0
    };
    (position, position)
}

fn remember_selection(window: &ManageNotesWindow, state: &mut State) {
    if let Some(selection) = state.selections.get_mut(state.editor.current) {
        *selection = (window.get_anchor(), window.get_cursor());
    }
}

fn focus_current(window: &ManageNotesWindow, state: &State) {
    if let Some(&(anchor, cursor)) = state.selections.get(state.editor.current) {
        window.invoke_select_text(anchor, cursor);
        window.invoke_focus_note();
    }
}

fn show_preferences(
    window: &ManageNotesWindow,
    preferences: &hydrus_store::settings::NotePreferences,
) {
    window.set_cog_checks(ModelRc::new(VecModel::from(vec![
        preferences.copy_all,
        preferences.copy_json,
        preferences.start_at_end,
        preferences.hover_text_only,
    ])));
}

fn strings(items: impl IntoIterator<Item = String>) -> ModelRc<SharedString> {
    let items: Vec<SharedString> = items.into_iter().map(Into::into).collect();
    ModelRc::new(VecModel::from(items))
}

fn show(window: &ManageNotesWindow, state: &State) {
    let editor = &state.editor;
    window.set_names(strings(editor.tabs.iter().map(|(n, _)| n.clone())));
    let current = editor.tabs.get(editor.current);
    window.set_current(if current.is_some() {
        i32::try_from(editor.current).unwrap_or(-1)
    } else {
        -1
    });
    let text = current.map_or("", |(_, t)| t.as_str());
    // (not while it is the text typed, so typing isn't disturbed)
    if window.get_text().as_str() != text {
        window.set_text(text.into());
    }
    window.set_can_edit(editor.can_edit());
    window.set_notice(state.notice.as_str().into());
    window.set_asking(state.asking.is_some());
    let Some(question) = state.asking else {
        return;
    };
    let (title, message, wants_text) = match question {
        Question::Add | Question::Rename(_) => ("enter text", NAME_PROMPT, true),
        Question::Delete => ("Are you sure?", DELETE_QUESTION, false),
        Question::Cancel => ("Are you sure?", CANCEL_QUESTION, false),
    };
    window.set_asking_title(title.into());
    window.set_asking_message(message.into());
    window.set_asking_wants_text(wants_text);
    let choices = if wants_text {
        ["ok"].as_slice()
    } else {
        ["yes", "no"].as_slice()
    };
    window.set_asking_choices(strings(choices.iter().map(|&c| c.to_owned())));
}

/// Open the dialog on `file`'s notes; it forgets itself from `slot` when
/// closed, and calls `applied` once the notes are written.
pub(crate) fn open(
    store: &Arc<Store>,
    file: HashId,
    slot: &Rc<RefCell<Option<ManageNotesWindow>>>,
    applied: Rc<dyn Fn()>,
) -> Result<ManageNotesWindow, String> {
    let notes = store
        .read(|c| hydrus_store::media::notes(c, file))
        .map_err(|e| e.to_string())?;
    let window = crate::app_title::new::<crate::ManageNotesWindow>().map_err(|e| e.to_string())?;
    window.set_window_title(TITLE.into());
    let active = Rc::new(Cell::new(true));
    let preferences: hydrus_store::settings::NotePreferences = store
        .read(hydrus_store::settings::get)
        .map_err(|error| error.to_string())?;
    let editor = NotesEditor::new(&notes, None);
    let selections = editor
        .tabs
        .iter()
        .map(|(_, text)| initial_selection(text, preferences.start_at_end))
        .collect();
    let state = Rc::new(RefCell::new(State {
        editor,
        selections,
        asking: None,
        notice: String::new(),
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
        let active = active.clone();
        let slot = slot.clone();
        let weak = window.as_weak();
        Rc::new(move || {
            if !active.replace(false) {
                return;
            }
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            slot.borrow_mut().take();
        })
    };
    let ask = {
        let active = active.clone();
        let state = state.clone();
        let refresh = refresh.clone();
        let weak = window.as_weak();
        Rc::new(move |question: Question, text: String| {
            if !active.get() || state.borrow().asking.is_some() {
                return;
            }
            state.borrow_mut().asking = Some(question);
            if let Some(window) = weak.upgrade() {
                window.set_asking_text(text.into());
            }
            refresh();
        })
    };
    window.on_tab_chosen({
        let active = active.clone();
        let weak = window.as_weak();
        let state = state.clone();
        let refresh = refresh.clone();
        move |i| {
            if !active.get() {
                return;
            }
            if state.borrow().asking.is_some() {
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            if let Ok(i) = usize::try_from(i) {
                let mut state = state.borrow_mut();
                if i >= state.editor.tabs.len() {
                    return;
                }
                remember_selection(&window, &mut state);
                state.editor.current = i;
            }
            refresh();
            focus_current(&window, &state.borrow());
        }
    });
    window.on_tab_renamed({
        let active = active.clone();
        let state = state.clone();
        let ask = ask.clone();
        move |i| {
            if !active.get() {
                return;
            }
            let Ok(i) = usize::try_from(i) else {
                return;
            };
            let name = state.borrow().editor.tabs.get(i).map(|(n, _)| n.clone());
            if let Some(name) = name {
                ask(Question::Rename(i), name);
            }
        }
    });
    window.on_text_edited({
        let active = active.clone();
        let state = state.clone();
        let weak = window.as_weak();
        move || {
            if !active.get() {
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            let mut state = state.borrow_mut();
            let current = state.editor.current;
            if let Some(tab) = state.editor.tabs.get_mut(current) {
                tab.1 = window.get_text().to_string();
            }
        }
    });
    window.on_add({
        let ask = ask.clone();
        move || ask(Question::Add, String::new())
    });
    window.on_rename({
        let active = active.clone();
        let state = state.clone();
        let ask = ask.clone();
        move || {
            if !active.get() {
                return;
            }
            let current = {
                let state = state.borrow();
                let editor = &state.editor;
                editor
                    .tabs
                    .get(editor.current)
                    .map(|(n, _)| (editor.current, n.clone()))
            };
            if let Some((i, name)) = current {
                ask(Question::Rename(i), name);
            }
        }
    });
    window.on_delete({
        let active = active.clone();
        let state = state.clone();
        let ask = ask.clone();
        move || {
            if !active.get() {
                return;
            }
            if state.borrow().editor.can_edit() {
                ask(Question::Delete, String::new());
            }
        }
    });
    window.on_copy({
        let active = active.clone();
        let store = store.clone();
        let state = state.clone();
        let refresh = refresh.clone();
        move || {
            if !active.get() {
                return;
            }
            if state.borrow().asking.is_some() {
                return;
            }
            let preferences =
                store.read(hydrus_store::settings::get::<hydrus_store::settings::NotePreferences>);
            let copied = match preferences {
                Ok(preferences) => state
                    .borrow()
                    .editor
                    .copy_with(preferences.copy_all, preferences.copy_json),
                Err(error) => {
                    state.borrow_mut().notice = error.to_string();
                    refresh();
                    return;
                }
            };
            if let Some((text, notice)) = copied {
                crate::copy_to_clipboard(&text);
                state.borrow_mut().notice = notice;
            }
            refresh();
        }
    });
    window.on_paste({
        let active = active.clone();
        let store = store.clone();
        let weak = window.as_weak();
        let state = state.clone();
        let refresh = refresh.clone();
        move || {
            if !active.get() {
                return;
            }
            if state.borrow().asking.is_some() {
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            let preferences: hydrus_store::settings::NotePreferences =
                match store.read(hydrus_store::settings::get) {
                    Ok(value) => value,
                    Err(error) => {
                        state.borrow_mut().notice = error.to_string();
                        refresh();
                        return;
                    }
                };
            let pasted = crate::from_clipboard();
            {
                let mut state = state.borrow_mut();
                remember_selection(&window, &mut state);
                let before = state.editor.tabs.clone();
                let selections = state.selections.clone();
                state.notice = match pasted {
                    Ok(text) => match state.editor.paste(&text) {
                        Ok(notice) | Err(notice) => notice,
                    },
                    Err(e) => format!("Problem pasting! {e}"),
                };
                state.selections = state
                    .editor
                    .tabs
                    .iter()
                    .map(|(name, text)| {
                        if let Some(index) = before.iter().position(|(old, _)| old == name) {
                            if before[index].1 == *text {
                                selections[index]
                            } else {
                                (0, 0)
                            }
                        } else {
                            initial_selection(text, preferences.start_at_end)
                        }
                    })
                    .collect();
            }
            refresh();
            focus_current(&window, &state.borrow());
        }
    });
    window.on_copy_urls({
        let active = active.clone();
        let state = state.clone();
        let refresh = refresh.clone();
        move || {
            if !active.get() {
                return;
            }
            let copied = state.borrow().editor.copy_urls();
            if let Some((text, notice)) = copied {
                if let Some(text) = text {
                    crate::copy_to_clipboard(&text);
                }
                state.borrow_mut().notice = notice;
            }
            refresh();
        }
    });
    window.on_chosen({
        let active = active.clone();
        let store = store.clone();
        let state = state.clone();
        let refresh = refresh.clone();
        let close = close.clone();
        let weak = window.as_weak();
        move |index| {
            if !active.get() {
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            let preferences: hydrus_store::settings::NotePreferences =
                match store.read(hydrus_store::settings::get) {
                    Ok(value) => value,
                    Err(error) => {
                        state.borrow_mut().notice = error.to_string();
                        refresh();
                        return;
                    }
                };
            let asked = state.borrow_mut().asking.take();
            let typed = window.get_asking_text().to_string();
            {
                let mut state = state.borrow_mut();
                remember_selection(&window, &mut state);
                match asked {
                    Some(Question::Add) => {
                        state.editor.add(&typed);
                        state
                            .selections
                            .push(initial_selection("", preferences.start_at_end));
                    }
                    Some(Question::Rename(i)) => state.editor.rename(i, &typed),
                    Some(Question::Delete) if index == 0 => {
                        let current = state.editor.current;
                        state.editor.delete_current();
                        if current < state.selections.len() {
                            state.selections.remove(current);
                        }
                    }
                    Some(Question::Cancel) if index == 0 => {
                        drop(state);
                        close();
                        return;
                    }
                    _ => {}
                }
            }
            refresh();
            focus_current(&window, &state.borrow());
        }
    });
    window.on_cancelled({
        let active = active.clone();
        let state = state.clone();
        let refresh = refresh.clone();
        move || {
            if !active.get() {
                return;
            }
            state.borrow_mut().asking = None;
            refresh();
        }
    });
    window.on_apply({
        let active = active.clone();
        let state = state.clone();
        let store = store.clone();
        let close = close.clone();
        move || {
            if !active.get() {
                return;
            }
            if state.borrow().asking.is_some() {
                return;
            }
            let (notes, deletees) = state.borrow().editor.value();
            let written = store.write_content(move |w| {
                for (name, note) in &notes {
                    w.set_note(file, name, note)?;
                }
                for name in &deletees {
                    w.delete_note(file, name)?;
                }
                Ok(())
            });
            match written {
                Ok(()) => {
                    applied();
                    close();
                }
                Err(e) => eprintln!("could not write the notes: {e}"),
            }
        }
    });
    let cancel = {
        let active = active.clone();
        let state = state.clone();
        let close = close.clone();
        let ask = ask.clone();
        Rc::new(move || {
            if !active.get() {
                return;
            }
            if state.borrow().asking.is_some() {
                state.borrow_mut().asking = None;
            }
            if state.borrow().editor.changed() {
                ask(Question::Cancel, String::new());
            } else {
                close();
            }
        })
    };
    window.on_cancel({
        let cancel = cancel.clone();
        move || cancel()
    });
    // (closing the window is cancelling)
    window.window().on_close_requested({
        let active = active.clone();
        let state = state.clone();
        let close = close.clone();
        let cancel = cancel.clone();
        move || {
            if !active.get() {
                return slint::CloseRequestResponse::HideWindow;
            }
            if state.borrow().editor.changed() {
                cancel();
                return slint::CloseRequestResponse::KeepWindowShown;
            }
            close();
            slint::CloseRequestResponse::HideWindow
        }
    });
    window.on_cog_opened({
        let active = active.clone();
        let state = state.clone();
        let store = store.clone();
        let weak = window.as_weak();
        move || {
            if !active.get()
                || state.borrow().asking.is_some()
                || weak
                    .upgrade()
                    .is_none_or(|window| !window.window().is_visible())
            {
                return;
            }
            if let (Some(window), Ok(preferences)) =
                (weak.upgrade(), store.read(hydrus_store::settings::get))
            {
                show_preferences(&window, &preferences);
            }
        }
    });
    window.on_cog_chosen({
        let active = active.clone();
        let state = state.clone();
        let refresh = refresh.clone();
        let store = store.clone();
        let weak = window.as_weak();
        move |index| {
            if !active.get()
                || state.borrow().asking.is_some()
                || !(0..=3).contains(&index)
                || weak
                    .upgrade()
                    .is_none_or(|window| !window.window().is_visible())
            {
                return;
            }
            let changed = store.write(move |writer| {
                let mut preferences: hydrus_store::settings::NotePreferences =
                    hydrus_store::settings::get(writer.conn())?;
                match index {
                    0 => preferences.copy_all = !preferences.copy_all,
                    1 => preferences.copy_json = !preferences.copy_json,
                    2 => preferences.start_at_end = !preferences.start_at_end,
                    3 => preferences.hover_text_only = !preferences.hover_text_only,
                    _ => unreachable!(),
                }
                hydrus_store::settings::set(writer.conn(), &preferences)?;
                Ok(preferences)
            });
            match changed {
                Ok(preferences) => {
                    if let Some(window) = weak.upgrade() {
                        show_preferences(&window, &preferences);
                    }
                }
                Err(error) => {
                    state.borrow_mut().notice = error.to_string();
                    refresh();
                }
            }
        }
    });
    show_preferences(&window, &preferences);
    show(&window, &state.borrow());
    focus_current(&window, &state.borrow());
    window.show().map_err(|e| e.to_string())?;
    Ok(window)
}
