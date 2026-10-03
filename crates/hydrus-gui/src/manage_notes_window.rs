//! The "manage notes" dialog, bound (`ui/manage_notes.slint`): a file's
//! notes read from the store into hydrus-gui-model's
//! [`NotesEditor`](crate::notes_editor::NotesEditor), its questions asked in
//! the window's own panel, and "apply" writing the notes to set and delete,
//! as the reference's `EditFileNotes` does.

use std::cell::RefCell;
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
    let window = ManageNotesWindow::new().map_err(|e| e.to_string())?;
    window.set_window_title(TITLE.into());
    let state = Rc::new(RefCell::new(State {
        editor: NotesEditor::new(&notes, None),
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
        let slot = slot.clone();
        let weak = window.as_weak();
        Rc::new(move || {
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            slot.borrow_mut().take();
        })
    };
    let ask = {
        let state = state.clone();
        let refresh = refresh.clone();
        let weak = window.as_weak();
        Rc::new(move |question: Question, text: String| {
            state.borrow_mut().asking = Some(question);
            if let Some(window) = weak.upgrade() {
                window.set_asking_text(text.into());
            }
            refresh();
        })
    };
    window.on_tab_chosen({
        let state = state.clone();
        let refresh = refresh.clone();
        move |i| {
            if let Ok(i) = usize::try_from(i) {
                state.borrow_mut().editor.current = i;
            }
            refresh();
        }
    });
    window.on_tab_renamed({
        let state = state.clone();
        let ask = ask.clone();
        move |i| {
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
        let state = state.clone();
        let weak = window.as_weak();
        move || {
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
        let state = state.clone();
        let ask = ask.clone();
        move || {
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
        let state = state.clone();
        let ask = ask.clone();
        move || {
            if state.borrow().editor.can_edit() {
                ask(Question::Delete, String::new());
            }
        }
    });
    window.on_copy({
        let state = state.clone();
        let refresh = refresh.clone();
        move || {
            let copied = state.borrow().editor.copy();
            if let Some((text, notice)) = copied {
                crate::copy_to_clipboard(&text);
                state.borrow_mut().notice = notice;
            }
            refresh();
        }
    });
    window.on_paste({
        let state = state.clone();
        let refresh = refresh.clone();
        move || {
            let pasted = crate::from_clipboard();
            let mut state = state.borrow_mut();
            state.notice = match pasted {
                Ok(text) => match state.editor.paste(&text) {
                    Ok(notice) | Err(notice) => notice,
                },
                Err(e) => format!("Problem pasting! {e}"),
            };
            drop(state);
            refresh();
        }
    });
    window.on_copy_urls({
        let state = state.clone();
        let refresh = refresh.clone();
        move || {
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
        let state = state.clone();
        let refresh = refresh.clone();
        let close = close.clone();
        let weak = window.as_weak();
        move |index| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let asked = state.borrow_mut().asking.take();
            let typed = window.get_asking_text().to_string();
            {
                let mut state = state.borrow_mut();
                let editor = &mut state.editor;
                match asked {
                    Some(Question::Add) => editor.add(&typed),
                    Some(Question::Rename(i)) => editor.rename(i, &typed),
                    Some(Question::Delete) if index == 0 => editor.delete_current(),
                    Some(Question::Cancel) if index == 0 => {
                        drop(state);
                        close();
                        return;
                    }
                    _ => {}
                }
            }
            refresh();
        }
    });
    window.on_cancelled({
        let state = state.clone();
        let refresh = refresh.clone();
        move || {
            state.borrow_mut().asking = None;
            refresh();
        }
    });
    window.on_apply({
        let state = state.clone();
        let store = store.clone();
        let close = close.clone();
        move || {
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
        let state = state.clone();
        let close = close.clone();
        let ask = ask.clone();
        Rc::new(move || {
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
        let state = state.clone();
        let slot = slot.clone();
        move || {
            if state.borrow().editor.changed() {
                cancel();
                return slint::CloseRequestResponse::KeepWindowShown;
            }
            slot.borrow_mut().take();
            slint::CloseRequestResponse::HideWindow
        }
    });
    show(&window, &state.borrow());
    window.show().map_err(|e| e.to_string())?;
    Ok(window)
}
