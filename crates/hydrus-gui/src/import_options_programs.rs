//! The external programs page of the import options editor: the list of
//! calls, and the dialogs the reference opens from it (its entry editor, the
//! notices before it, and the question before a delete). The choices and
//! their words are `hydrus_gui_model::external_program_entries`'s.

use std::cell::RefCell;
use std::rc::Rc;

use hydrus_core::import_options::{ExternalProgramEntry, ExternalProgramsOptions};
use hydrus_gui_model::external_program_entries::{self as model, Choice};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::{
    ExternalProgramEntryWindow, ImportOptionsWindow, SessionDialog, TableColumn, TableRow,
};

type Slot<W> = Rc<RefCell<Option<W>>>;

thread_local! {
    /// The slots of the page opened last on this thread, for tests to reach
    /// the dialogs it opens.
    static LAST: RefCell<Option<(Slot<ExternalProgramEntryWindow>, Slot<SessionDialog>)>> =
        const { RefCell::new(None) };
}

/// The entry editor the page last opened on this thread, if it is open.
pub fn entry_window() -> Option<ExternalProgramEntryWindow> {
    LAST.with(|last| {
        last.borrow()
            .as_ref()
            .and_then(|l| l.0.borrow().as_ref().map(ComponentHandle::clone_strong))
    })
}

/// The notice or question the page last opened on this thread, if open.
pub fn dialog() -> Option<SessionDialog> {
    LAST.with(|last| {
        last.borrow()
            .as_ref()
            .and_then(|l| l.1.borrow().as_ref().map(ComponentHandle::clone_strong))
    })
}

/// What the page has open: the entry editor and one notice or question.
pub(crate) struct Programs {
    pub entry: Slot<ExternalProgramEntryWindow>,
    pub dialog: Slot<SessionDialog>,
    pub list: RefCell<model::List>,
}

impl Default for Programs {
    fn default() -> Self {
        let programs = Self {
            entry: Rc::default(),
            dialog: Rc::default(),
            list: RefCell::default(),
        };
        LAST.with(|last| {
            *last.borrow_mut() = Some((programs.entry.clone(), programs.dialog.clone()));
        });
        programs
    }
}

impl Programs {
    /// Close whatever is open (the editor is going away).
    pub fn cancel(&self) {
        let entry = self.entry.borrow_mut().take();
        if let Some(entry) = entry {
            let _ = entry.hide();
        }
        let dialog = self.dialog.borrow_mut().take();
        if let Some(dialog) = dialog {
            let _ = dialog.hide();
        }
    }

    /// Whether the entry editor or a dialog is open.
    pub fn busy(&self) -> bool {
        self.entry.borrow().is_some() || self.dialog.borrow().is_some()
    }
}

/// Show the page's list for `options` (not set: no rows).
pub(crate) fn show(
    window: &ImportOptionsWindow,
    programs: &Programs,
    options: Option<&ExternalProgramsOptions>,
) {
    let rows = options.map(model::rows).unwrap_or_default();
    let list = programs.list.borrow();
    let selected = list.selected(rows.len());
    window.set_program_columns(ModelRc::new(VecModel::from(vec![TableColumn {
        title: "".into(),
        width: 400.0,
        stretch: true,
    }])));
    window.set_program_rows(ModelRc::new(VecModel::from(
        rows.iter()
            .enumerate()
            .map(|(i, text)| TableRow {
                cells: ModelRc::new(VecModel::from(vec![SharedString::from(text.as_str())])),
                selected: selected.contains(&i),
            })
            .collect::<Vec<_>>(),
    )));
    window.set_program_any_selected(!selected.is_empty());
    window.set_program_child_open(programs.busy());
}

/// A notice (`ShowInformation` or `ShowCritical`), then `then`.
fn notice(
    programs: &Rc<Programs>,
    title: &str,
    message: &str,
    then: Rc<dyn Fn()>,
) -> Result<(), String> {
    let window = crate::app_title::new::<SessionDialog>().map_err(|e| e.to_string())?;
    window.set_window_title(title.into());
    window.set_message(message.into());
    window.set_notice_only(true);
    window.set_notice_ok_label("OK".into());
    let finish: Rc<dyn Fn()> = Rc::new({
        let weak = window.as_weak();
        let slot = Rc::downgrade(&programs.dialog);
        let done = std::cell::Cell::new(false);
        move || {
            if done.replace(true) {
                return;
            }
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            if let Some(slot) = slot.upgrade() {
                slot.borrow_mut().take();
            }
            then();
        }
    });
    window.on_cancelled({
        let finish = finish.clone();
        move || finish()
    });
    window.on_force_close({
        let finish = finish.clone();
        move || finish()
    });
    window.window().on_close_requested(move || {
        finish();
        slint::CloseRequestResponse::HideWindow
    });
    window.show().map_err(|e| e.to_string())?;
    *programs.dialog.borrow_mut() = Some(window);
    Ok(())
}

/// The question before a delete; `answered` is told yes or no.
pub(crate) fn ask_delete(
    programs: &Rc<Programs>,
    count: usize,
    answered: Rc<dyn Fn(bool)>,
) -> Result<(), String> {
    let window = crate::app_title::new::<SessionDialog>().map_err(|e| e.to_string())?;
    window.set_window_title("Are you sure?".into());
    window.set_message(model::delete_question(count).into());
    let finish: Rc<dyn Fn(bool)> = Rc::new({
        let weak = window.as_weak();
        let slot = Rc::downgrade(&programs.dialog);
        let done = std::cell::Cell::new(false);
        move |yes| {
            if done.replace(true) {
                return;
            }
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            if let Some(slot) = slot.upgrade() {
                slot.borrow_mut().take();
            }
            answered(yes);
        }
    });
    window.on_answered({
        let finish = finish.clone();
        move |yes| finish(yes)
    });
    window.on_cancelled({
        let finish = finish.clone();
        move || finish(false)
    });
    window.on_force_close({
        let finish = finish.clone();
        move || finish(false)
    });
    window.window().on_close_requested(move || {
        finish(false);
        slint::CloseRequestResponse::HideWindow
    });
    window.show().map_err(|e| e.to_string())?;
    *programs.dialog.borrow_mut() = Some(window);
    Ok(())
}

/// `_EditEntry`: the entry editor for `entry` over `choices`, with the
/// reference's notices first; `done` gets the edited entry, or `None` when
/// cancelled (or when there were no calls to choose from).
pub(crate) fn edit_entry(
    programs: &Rc<Programs>,
    choices: &[Choice],
    entry: &ExternalProgramEntry,
    done: Rc<dyn Fn(Option<ExternalProgramEntry>)>,
) -> Result<(), String> {
    let Some(editing) = model::start_editing(entry, choices) else {
        return notice(
            programs,
            model::NO_CALLS_TITLE,
            &model::no_calls_message(),
            Rc::new(move || done(None)),
        );
    };
    let open: Rc<dyn Fn() -> Result<(), String>> = Rc::new({
        let programs = programs.clone();
        let choices = choices.to_vec();
        let done = done.clone();
        move || {
            let window =
                crate::app_title::new::<ExternalProgramEntryWindow>().map_err(|e| e.to_string())?;
            window.set_window_title(model::EDIT_TITLE.into());
            window.set_programs(ModelRc::new(VecModel::from(
                choices
                    .iter()
                    .map(|c| SharedString::from(c.name.as_str()))
                    .collect::<Vec<_>>(),
            )));
            window.set_program_index(i32::try_from(editing.program_index).unwrap_or(0));
            window.set_whens(ModelRc::new(VecModel::from(
                model::WHENS
                    .iter()
                    .map(|(text, _, _)| SharedString::from(*text))
                    .collect::<Vec<_>>(),
            )));
            window.set_when_index(i32::try_from(editing.when_index).unwrap_or(0));
            let finish: Rc<dyn Fn(bool)> = Rc::new({
                let weak = window.as_weak();
                let slot = Rc::downgrade(&programs.entry);
                let choices = choices.clone();
                let done = done.clone();
                let closed = std::cell::Cell::new(false);
                move |accepted| {
                    if closed.replace(true) {
                        return;
                    }
                    let edited = weak.upgrade().and_then(|window| {
                        let edited = accepted
                            .then(|| {
                                model::finished(
                                    &choices,
                                    usize::try_from(window.get_program_index()).ok()?,
                                    usize::try_from(window.get_when_index()).ok()?,
                                )
                            })
                            .flatten();
                        let _ = window.hide();
                        edited
                    });
                    if let Some(slot) = slot.upgrade() {
                        slot.borrow_mut().take();
                    }
                    done(edited);
                }
            });
            window.on_accepted({
                let finish = finish.clone();
                move || finish(true)
            });
            window.on_cancelled({
                let finish = finish.clone();
                move || finish(false)
            });
            window.window().on_close_requested(move || {
                finish(false);
                slint::CloseRequestResponse::HideWindow
            });
            window.show().map_err(|e| e.to_string())?;
            *programs.entry.borrow_mut() = Some(window);
            Ok(())
        }
    });
    match editing.notice {
        Some(text) => notice(
            programs,
            "Information",
            text,
            Rc::new(move || {
                if let Err(error) = open() {
                    eprintln!("could not open the external program editor: {error}");
                }
            }),
        ),
        None => open(),
    }
}
