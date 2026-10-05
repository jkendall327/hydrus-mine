//! The "manage times" dialog, bound (`ui/manage_times.slint`): the files'
//! times read from the store into hydrus-gui-model's
//! [`TimesEditor`](crate::times_editor::TimesEditor), each edited in a
//! [`DateTimeEditor`](crate::datetime_editor::DateTimeEditor) window, its
//! questions asked in the window's own panel, and "apply" writing the
//! times changed (and a changed file modified time to the files on disk),
//! as the reference's `EditFileTimestampsPanel` does.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};

use hydrus_core::HashId;
use hydrus_core::content::CanvasType;
use hydrus_store::Store;
use hydrus_store::content::FileTime;
use hydrus_store::media::MediaResult;

use crate::datetime_editor::{DateTimeEditor, Said, copy_text, read_pasted};
use crate::list_selection::ListSelection;
use crate::times_editor::{
    ARCHIVED, DELETE_QUESTION, DELETED, DOMAIN_EXISTS, DOMAIN_MODIFIED, DOMAIN_PROMPT,
    FILE_MODIFIED, FileTimes, IMPORTED, LAST_VIEWED, Location, Main, PREVIOUSLY_IMPORTED, Pasted,
    SOME_FILES_NO, SOME_FILES_YES, ServiceName, TITLE, TimeRange, TimeUpdate, TimesEditor,
};
use crate::{DateTimeEditorWindow, ManageTimesWindow, TableRow, TimeRow};

/// What the window's panel asks.
#[derive(Debug, Clone)]
enum Question {
    /// The domain to add.
    Domain,
    /// A warning or error, dismissed with "ok".
    Told,
    DeleteDomains(Vec<String>),
    /// Whether an edit of these domains is for all files.
    AllFiles(Vec<String>, TimeRange),
    /// A paste's question, with the answers so far.
    Paste(String, Vec<bool>),
    ManyChanges,
}

struct State {
    editor: TimesEditor,
    domains: ListSelection<usize>,
    services: ListSelection<usize>,
    asking: Option<(Question, String, String)>,
    notice: String,
    /// Errors to show after the one shown.
    told: Vec<(String, String)>,
}

/// Opens the date-time editor on a time, calling back with its value and
/// whether it changed, if applied.
type Edit = Rc<dyn Fn(TimeRange, Box<dyn Fn(TimeRange, bool)>)>;

/// The time the dialog runs at: now, in seconds and milliseconds.
fn now() -> (i64, i64) {
    let ms = jiff::Timestamp::now().as_millisecond();
    (ms.div_euclid(1000), ms)
}

fn strings(items: impl IntoIterator<Item = String>) -> ModelRc<SharedString> {
    let items: Vec<SharedString> = items.into_iter().map(Into::into).collect();
    ModelRc::new(VecModel::from(items))
}

fn table(rows: Vec<Vec<String>>, selection: &ListSelection<usize>) -> ModelRc<TableRow> {
    let rows: Vec<TableRow> = rows
        .into_iter()
        .enumerate()
        .map(|(i, cells)| TableRow {
            cells: strings(cells),
            selected: selection.is_selected(i),
        })
        .collect();
    ModelRc::new(VecModel::from(rows))
}

fn show(window: &ManageTimesWindow, state: &State, tz: &jiff::tz::TimeZone) {
    let (now, _) = now();
    let editor = &state.editor;
    let times: Vec<TimeRow> = editor
        .main
        .iter()
        .filter(|t| t.shown)
        .map(|t| TimeRow {
            label: t.which.label().into(),
            text: editor.main_text(t.which, now, tz).into(),
            enabled: t.enabled,
        })
        .collect();
    window.set_times(ModelRc::new(VecModel::from(times)));
    window.set_warning(editor.warning().unwrap_or_default().into());
    let domains = editor
        .domain_rows()
        .into_iter()
        .map(|(d, r)| vec![d.to_owned(), r.value.text(now, tz)])
        .collect();
    window.set_domains(table(domains, &state.domains));
    window.set_domain_selected(!state.domains.is_empty());
    let services = editor
        .file_service_rows()
        .into_iter()
        .map(|(s, k, r)| editor.file_service_text(s, k, r, now, tz).to_vec())
        .collect();
    window.set_file_services(table(services, &state.services));
    window.set_service_selected(!state.services.is_empty());
    window.set_can_copy(editor.can_copy());
    window.set_notice(state.notice.as_str().into());
    window.set_asking(state.asking.is_some());
    if let Some((question, title, message)) = &state.asking {
        window.set_asking_title(title.as_str().into());
        window.set_asking_message(message.as_str().into());
        window.set_asking_wants_text(matches!(question, Question::Domain));
        let choices: &[&str] = match question {
            Question::Domain | Question::Told => &["ok"],
            Question::AllFiles(..) | Question::Paste(..) => &[SOME_FILES_YES, SOME_FILES_NO],
            Question::DeleteDomains(_) | Question::ManyChanges => &["yes", "no"],
        };
        window.set_asking_choices(strings(choices.iter().map(|&c| c.to_owned())));
    }
}

/// The grid's shown times, in order (the rows' times).
fn shown(editor: &TimesEditor) -> Vec<Main> {
    editor
        .main
        .iter()
        .filter(|t| t.shown)
        .map(|t| t.which)
        .collect()
}

/// The time an update writes, as the store has it.
fn file_time(update: &TimeUpdate) -> Option<FileTime> {
    let time = &update.time;
    Some(match (time.kind, &time.location) {
        (DOMAIN_MODIFIED, Location::Domain(domain)) => FileTime::DomainModified(domain.clone()),
        (FILE_MODIFIED, _) => FileTime::FileModified,
        (ARCHIVED, _) => FileTime::Archived,
        (LAST_VIEWED, Location::Canvas(canvas)) => {
            FileTime::LastViewed(CanvasType::from_code(u8::try_from(*canvas).ok()?)?)
        }
        (IMPORTED, Location::Service(s)) => FileTime::Imported(*s),
        (DELETED, Location::Service(s)) => FileTime::Deleted(*s),
        (PREVIOUSLY_IMPORTED, Location::Service(s)) => FileTime::PreviouslyImported(*s),
        _ => return None,
    })
}

/// A file's times, as the dialog reads them.
fn file_times(result: &MediaResult) -> FileTimes {
    let viewed = |canvas: CanvasType| {
        result
            .viewing
            .iter()
            .find(|v| v.canvas == canvas)
            .and_then(|v| v.last_viewed)
            .map(|t| t.0)
    };
    let mut services: Vec<(hydrus_core::ServiceId, i64, i64)> = result
        .current
        .iter()
        .filter_map(|c| Some((c.service, IMPORTED, c.added?.0)))
        .collect();
    for d in &result.deleted {
        services.extend(d.deleted.map(|t| (d.service, DELETED, t.0)));
        services.extend(
            d.originally_added
                .map(|t| (d.service, PREVIOUSLY_IMPORTED, t.0)),
        );
    }
    FileTimes {
        inbox: result.inbox,
        modified: result
            .info
            .as_ref()
            .and_then(|i| i.file_modified)
            .map(|t| t.0),
        archived: result.archived.map(|t| t.0),
        viewed: viewed(CanvasType::MediaViewer),
        preview: viewed(CanvasType::Preview),
        domains: result
            .domain_modified
            .iter()
            .map(|(d, t)| (d.clone(), t.0))
            .collect(),
        services,
    }
}

/// Open the dialog on `files`' times; it forgets itself from `slot` when
/// closed, and calls `applied` once the times are written. The date-time
/// editor it opens is kept in `editing` while open.
pub(crate) fn open(
    store: &Arc<Store>,
    files: &[HashId],
    slot: &Rc<RefCell<Option<ManageTimesWindow>>>,
    editing: &Rc<RefCell<Option<DateTimeEditorWindow>>>,
    jobs: &crate::metadata_file_jobs::Jobs,
    applied: Rc<dyn Fn()>,
) -> Result<ManageTimesWindow, String> {
    let previous = slot.borrow().as_ref().map(ComponentHandle::clone_strong);
    if let Some(previous) = previous {
        previous.invoke_cancel();
    }
    let active = Rc::new(Cell::new(true));
    let snapshot = store.snapshot();
    let services = snapshot.services.clone();
    let mut loaded = store
        .read(|c| hydrus_store::media::load(c, &services, None, files))
        .map_err(|e| e.to_string())?
        .results;
    let results: Vec<MediaResult> = files
        .iter()
        .filter_map(|f| {
            let i = loaded.iter().position(|r| r.hash_id == *f)?;
            Some(loaded.swap_remove(i))
        })
        .collect();
    let names = services
        .all()
        .map(|s| ServiceName {
            id: s.id,
            name: s.name.clone(),
            key: s.key.to_hex(),
        })
        .collect();
    let (_, now_ms) = now();
    let tz = jiff::tz::TimeZone::system();
    let window = ManageTimesWindow::new().map_err(|e| e.to_string())?;
    window.set_window_title(TITLE.into());
    let state = Rc::new(RefCell::new(State {
        editor: TimesEditor::new(results.iter().map(file_times).collect(), names, now_ms),
        domains: ListSelection::default(),
        services: ListSelection::default(),
        asking: None,
        notice: String::new(),
        told: Vec::new(),
    }));
    let refresh = {
        let state = state.clone();
        let weak = window.as_weak();
        let tz = tz.clone();
        Rc::new(move || {
            if let Some(window) = weak.upgrade() {
                show(&window, &state.borrow(), &tz);
            }
        })
    };
    let ask = {
        let state = state.clone();
        let refresh = refresh.clone();
        Rc::new(move |question: Question, title: &str, message: &str| {
            state.borrow_mut().asking = Some((question, title.to_owned(), message.to_owned()));
            refresh();
        })
    };
    // errors and warnings, one after another
    let tell = {
        let state = state.clone();
        let ask = ask.clone();
        let refresh = refresh.clone();
        Rc::new(move |said: Vec<Said>| {
            for s in said {
                match s {
                    Said::Notice(n) => state.borrow_mut().notice = n,
                    Said::Warning(w) => state.borrow_mut().told.push(("Warning".into(), w)),
                    Said::Critical(t, m) => state.borrow_mut().told.push((t, m)),
                }
            }
            let next = {
                let mut state = state.borrow_mut();
                if state.asking.is_none() && !state.told.is_empty() {
                    Some(state.told.remove(0))
                } else {
                    None
                }
            };
            match next {
                Some((title, message)) => ask(Question::Told, &title, &message),
                None => refresh(),
            }
        })
    };
    let close = {
        let slot = slot.clone();
        let editing = editing.clone();
        let weak = window.as_weak();
        let active = active.clone();
        Rc::new(move || {
            if !active.replace(false) {
                return;
            }
            let child = editing.borrow().as_ref().map(ComponentHandle::clone_strong);
            if let Some(child) = child {
                child.invoke_cancel();
            }
            if let Some(editing) = editing.borrow_mut().take() {
                let _ = editing.hide();
            }
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            slot.borrow_mut().take();
        })
    };
    // a time edited in the date-time editor: `done` with its value and
    // whether it changed, if applied
    let edit: Edit = {
        let editing = editing.clone();
        let tz = tz.clone();
        let tell = tell.clone();
        let active = active.clone();
        Rc::new(
            move |value: TimeRange, done: Box<dyn Fn(TimeRange, bool)>| {
                if !active.get() || editing.borrow().is_some() {
                    return;
                }
                let alive = Rc::new(Cell::new(true));
                let Ok(dialog) = DateTimeEditorWindow::new() else {
                    return;
                };
                let (now_s, now_ms) = now();
                let editor = Rc::new(RefCell::new(DateTimeEditor::new(value, now_ms, tz.clone())));
                let told: Rc<RefCell<Vec<(String, String)>>> = Rc::default();
                let draw = {
                    let editor = editor.clone();
                    let told = told.clone();
                    let weak = dialog.as_weak();
                    let tz = tz.clone();
                    Rc::new(move |fields: bool| {
                        let Some(dialog) = weak.upgrade() else {
                            return;
                        };
                        let editor = editor.borrow();
                        dialog.set_label(editor.label(now_s).unwrap_or_default().into());
                        dialog.set_step_shown(editor.step_shown());
                        if fields {
                            dialog.set_date(editor.date.strftime("%Y-%m-%d").to_string().into());
                            dialog
                                .set_time(editor.time.strftime("%H:%M:%S%.3f").to_string().into());
                            dialog.set_step(editor.step_ms.to_string().into());
                        }
                        dialog.set_value(editor.value().text(now_s, &tz).into());
                        let told = told.borrow();
                        dialog.set_asking(!told.is_empty());
                        if let Some((title, message)) = told.first() {
                            dialog.set_asking_title(title.as_str().into());
                            dialog.set_asking_message(message.as_str().into());
                        }
                    })
                };
                let say = {
                    let told = told.clone();
                    let weak = dialog.as_weak();
                    move |said: Vec<Said>| {
                        for s in said {
                            match s {
                                Said::Notice(n) => {
                                    if let Some(dialog) = weak.upgrade() {
                                        dialog.set_notice(n.into());
                                    }
                                }
                                Said::Warning(w) => told.borrow_mut().push(("Warning".into(), w)),
                                Said::Critical(t, m) => told.borrow_mut().push((t, m)),
                            }
                        }
                    }
                };
                dialog.on_edited({
                    let editor = editor.clone();
                    let draw = draw.clone();
                    let weak = dialog.as_weak();
                    move || {
                        let Some(dialog) = weak.upgrade() else {
                            return;
                        };
                        let mut editor = editor.borrow_mut();
                        if let Ok(date) =
                            jiff::civil::Date::strptime("%Y-%m-%d", dialog.get_date().as_str())
                        {
                            editor.date = date;
                        }
                        if let Ok(time) =
                            jiff::civil::Time::strptime("%H:%M:%S%.f", dialog.get_time().as_str())
                                .or_else(|_| {
                                    jiff::civil::Time::strptime(
                                        "%H:%M:%S",
                                        dialog.get_time().as_str(),
                                    )
                                })
                        {
                            editor.time = time;
                        }
                        if let Ok(step) = dialog.get_step().trim().parse::<i64>() {
                            editor.step_ms = step;
                        }
                        drop(editor);
                        draw(false);
                    }
                });
                dialog.on_copy({
                    let editor = editor.clone();
                    let say = say.clone();
                    move || {
                        let (text, notice) = editor.borrow().copy();
                        crate::copy_to_clipboard(&text);
                        say(vec![notice]);
                    }
                });
                dialog.on_paste({
                    let editor = editor.clone();
                    let say = say.clone();
                    let draw = draw.clone();
                    move || {
                        match crate::from_clipboard() {
                            Ok(text) => {
                                let said = editor.borrow_mut().paste(&text);
                                say(said);
                            }
                            Err(e) => say(vec![Said::Critical("Problem pasting!".into(), e)]),
                        }
                        draw(true);
                    }
                });
                dialog.on_now({
                    let editor = editor.clone();
                    let draw = draw.clone();
                    move || {
                        editor.borrow_mut().now();
                        draw(true);
                    }
                });
                dialog.on_dismissed({
                    let told = told.clone();
                    let draw = draw.clone();
                    move || {
                        let mut told = told.borrow_mut();
                        if !told.is_empty() {
                            told.remove(0);
                        }
                        drop(told);
                        draw(false);
                    }
                });
                let finish = {
                    let editing = editing.clone();
                    let weak = dialog.as_weak();
                    let alive = alive.clone();
                    move || {
                        if !alive.replace(false) {
                            return;
                        }
                        if let Some(dialog) = weak.upgrade() {
                            let _ = dialog.hide();
                        }
                        editing.borrow_mut().take();
                    }
                };
                dialog.on_apply({
                    let editor = editor.clone();
                    let finish = finish.clone();
                    let alive = alive.clone();
                    let active = active.clone();
                    move || {
                        if !alive.get() || !active.get() {
                            return;
                        }
                        let (value, changed) = {
                            let editor = editor.borrow();
                            (editor.value(), editor.has_changes())
                        };
                        finish();
                        done(value, changed);
                    }
                });
                dialog.on_cancel(finish.clone());
                dialog.window().on_close_requested(move || {
                    finish();
                    slint::CloseRequestResponse::HideWindow
                });
                draw(true);
                if let Err(e) = dialog.show() {
                    tell(vec![Said::Critical("Error".into(), e.to_string())]);
                    return;
                }
                *editing.borrow_mut() = Some(dialog);
            },
        )
    };
    window.on_time_clicked({
        let state = state.clone();
        let edit = edit.clone();
        let refresh = refresh.clone();
        move |i| {
            let which = usize::try_from(i)
                .ok()
                .and_then(|i| shown(&state.borrow().editor).get(i).copied());
            let Some(which) = which else {
                return;
            };
            let value = state.borrow().editor.main_time(which).value;
            let state = state.clone();
            let refresh = refresh.clone();
            edit(
                value,
                Box::new(move |value, _| {
                    state.borrow_mut().editor.set_main(which, value);
                    refresh();
                }),
            );
        }
    });
    window.on_time_copy({
        let state = state.clone();
        let refresh = refresh.clone();
        move |i| {
            let which = usize::try_from(i)
                .ok()
                .and_then(|i| shown(&state.borrow().editor).get(i).copied());
            if let Some(which) = which {
                let text = copy_text(&state.borrow().editor.main_time(which).value);
                crate::copy_to_clipboard(&text);
                state.borrow_mut().notice = "Copied!".into();
                refresh();
            }
        }
    });
    window.on_time_paste({
        let state = state.clone();
        let tell = tell.clone();
        let tz = tz.clone();
        move |i| {
            let which = usize::try_from(i)
                .ok()
                .and_then(|i| shown(&state.borrow().editor).get(i).copied());
            let Some(which) = which else {
                return;
            };
            let text = match crate::from_clipboard() {
                Ok(text) => text,
                Err(e) => {
                    tell(vec![Said::Critical("Problem pasting!".into(), e)]);
                    return;
                }
            };
            let (read, mut said) = read_pasted(&text, &tz);
            if let Some(ms) = read {
                let mut state = state.borrow_mut();
                let value = state.editor.main_time(which).value.with(ms, false);
                state.editor.set_main(which, value);
                said.push(Said::Notice("Pasted!".into()));
            }
            tell(said);
        }
    });
    let pick =
        |selection: &mut ListSelection<usize>, count: usize, row: i32, ctrl: bool, shift: bool| {
            if let Ok(row) = usize::try_from(row) {
                let order: Vec<usize> = (0..count).collect();
                selection.click(&order, row, ctrl, shift);
            }
        };
    window.on_domain_clicked({
        let state = state.clone();
        let refresh = refresh.clone();
        move |row, ctrl, shift| {
            let mut state = state.borrow_mut();
            let count = state.editor.domain_rows().len();
            pick(&mut state.domains, count, row, ctrl, shift);
            drop(state);
            refresh();
        }
    });
    let selected_domains = {
        let state = state.clone();
        move || -> Vec<String> {
            let state = state.borrow();
            let rows = state.editor.domain_rows();
            let order: Vec<usize> = (0..rows.len()).collect();
            state
                .domains
                .in_order(&order)
                .into_iter()
                .filter_map(|i| rows.get(i).map(|(d, _)| (*d).to_owned()))
                .collect()
        }
    };
    let edit_domains = {
        let state = state.clone();
        let edit = edit.clone();
        let ask = ask.clone();
        let refresh = refresh.clone();
        let selected_domains = selected_domains.clone();
        Rc::new(move || {
            let chosen = selected_domains();
            let Some(first) = chosen.first().cloned() else {
                return;
            };
            let Some(value) = state.borrow().editor.domain_value(&first) else {
                return;
            };
            let state = state.clone();
            let ask = ask.clone();
            let refresh = refresh.clone();
            edit(
                value,
                Box::new(move |value, changed| {
                    if !changed {
                        return;
                    }
                    if state.borrow().editor.domain_edit_asks(&first) {
                        ask(
                            Question::AllFiles(chosen.clone(), value),
                            "Are you sure?",
                            crate::times_editor::SOME_FILES_QUESTION,
                        );
                    } else {
                        state
                            .borrow_mut()
                            .editor
                            .edit_domains(&chosen, value, false);
                        refresh();
                    }
                }),
            );
        })
    };
    window.on_domain_activated({
        let state = state.clone();
        let edit_domains = edit_domains.clone();
        move |row| {
            if let Ok(row) = usize::try_from(row) {
                state.borrow_mut().domains.select_only(Some(row));
            }
            edit_domains();
        }
    });
    window.on_edit_domains({
        let edit_domains = edit_domains.clone();
        move || edit_domains()
    });
    window.on_delete_domains({
        let ask = ask.clone();
        let selected_domains = selected_domains.clone();
        move || {
            let chosen = selected_domains();
            if !chosen.is_empty() {
                ask(
                    Question::DeleteDomains(chosen),
                    "Are you sure?",
                    DELETE_QUESTION,
                );
            }
        }
    });
    window.on_add_domain({
        let ask = ask.clone();
        let weak = window.as_weak();
        move || {
            if let Some(window) = weak.upgrade() {
                window.set_asking_text(SharedString::new());
            }
            ask(Question::Domain, "enter text", DOMAIN_PROMPT);
        }
    });
    window.on_service_clicked({
        let state = state.clone();
        let refresh = refresh.clone();
        move |row, ctrl, shift| {
            let mut state = state.borrow_mut();
            let count = state.editor.file_service_rows().len();
            pick(&mut state.services, count, row, ctrl, shift);
            drop(state);
            refresh();
        }
    });
    let edit_services = {
        let state = state.clone();
        let edit = edit.clone();
        let refresh = refresh.clone();
        Rc::new(move || {
            let chosen: Vec<(hydrus_core::ServiceId, i64)> = {
                let state = state.borrow();
                let rows = state.editor.file_service_rows();
                let order: Vec<usize> = (0..rows.len()).collect();
                state
                    .services
                    .in_order(&order)
                    .into_iter()
                    .filter_map(|i| rows.get(i).map(|(s, k, _)| (*s, *k)))
                    .collect()
            };
            let Some(&(service, kind)) = chosen.first() else {
                return;
            };
            let Some(value) = state.borrow().editor.file_service_value(service, kind) else {
                return;
            };
            let state = state.clone();
            let refresh = refresh.clone();
            edit(
                value,
                Box::new(move |value, changed| {
                    if changed {
                        state.borrow_mut().editor.edit_file_services(&chosen, value);
                        refresh();
                    }
                }),
            );
        })
    };
    window.on_service_activated({
        let state = state.clone();
        let edit_services = edit_services.clone();
        move |row| {
            if let Ok(row) = usize::try_from(row) {
                state.borrow_mut().services.select_only(Some(row));
            }
            edit_services();
        }
    });
    window.on_edit_services(move || edit_services());
    window.on_copy({
        let state = state.clone();
        let refresh = refresh.clone();
        move |which| {
            let kinds: Option<&[i64]> = match which {
                1 => Some(&[FILE_MODIFIED]),
                2 => Some(&[ARCHIVED]),
                3 => Some(&[LAST_VIEWED]),
                4 => Some(&[DOMAIN_MODIFIED]),
                5 => Some(&[IMPORTED, PREVIOUSLY_IMPORTED, DELETED]),
                _ => None,
            };
            let copied = state.borrow().editor.copy(kinds);
            if let Some((text, notice)) = copied {
                crate::copy_to_clipboard(&text);
                state.borrow_mut().notice = notice;
            }
            refresh();
        }
    });
    // a paste, asking its questions in turn
    let paste: Rc<dyn Fn(String, Vec<bool>)> = {
        let state = state.clone();
        let ask = ask.clone();
        let tell = tell.clone();
        Rc::new(move |text: String, answers: Vec<bool>| {
            let pasted = state.borrow_mut().editor.paste(&text, &answers);
            match pasted {
                Pasted::Done(notice) => tell(vec![Said::Notice(notice)]),
                Pasted::Ask(question) => {
                    ask(Question::Paste(text, answers), "Are you sure?", question);
                }
                Pasted::Error(message) => {
                    tell(vec![Said::Critical("Clipboard Error!".into(), message)]);
                }
            }
        })
    };
    window.on_paste({
        let paste = paste.clone();
        let tell = tell.clone();
        move || match crate::from_clipboard() {
            Ok(text) => paste(text, Vec::new()),
            Err(e) => tell(vec![Said::Critical("Problem pasting!".into(), e)]),
        }
    });
    let write = {
        let state = state.clone();
        let store = store.clone();
        let close = close.clone();
        let active = active.clone();
        let editing = editing.clone();
        let jobs = jobs.clone();
        Rc::new(move || {
            if !active.get() || editing.borrow().is_some() {
                return;
            }
            let (updates, modified) = {
                let state = state.borrow();
                (state.editor.updates(), state.editor.file_modified_update())
            };
            if !updates.is_empty() {
                let ids: Vec<HashId> = results.iter().map(|r| r.hash_id).collect();
                let written = store.write_content(move |w| {
                    for update in &updates {
                        let hashes: Vec<HashId> = update.files.iter().map(|&i| ids[i]).collect();
                        let Some(time) = file_time(update) else {
                            continue;
                        };
                        match (update.time.ms, &time) {
                            (Some(ms), time) => w.set_file_time(&hashes, time, ms)?,
                            (None, FileTime::DomainModified(domain)) => {
                                w.clear_domain_modified_time(&hashes, domain)?;
                            }
                            (None, _) => {}
                        }
                    }
                    Ok(())
                });
                if let Err(e) = written {
                    eprintln!("could not write the times: {e}");
                    return;
                }
                // (and the files' modified times on disk)
                if let Some((files, ms, step)) = modified {
                    let files = files
                        .iter()
                        .filter_map(|&i| {
                            let result = results.get(i)?;
                            let info = result.info.as_ref()?;
                            Some(hydrus_store::metadata_jobs::File {
                                id: result.hash_id,
                                hash: result.hash,
                                mime: info.mime,
                                original_mime: info.original_mime.unwrap_or(info.mime),
                            })
                        })
                        .collect();
                    let request = hydrus_store::metadata_jobs::Request::Modified {
                        files,
                        milliseconds: ms,
                        step,
                    };
                    if let Err(error) = jobs.start(store.clone(), request, applied.clone()) {
                        eprintln!("{error}");
                    }
                }
                applied();
            }
            close();
        })
    };
    window.on_apply({
        let state = state.clone();
        let ask = ask.clone();
        let write = write.clone();
        let active = active.clone();
        let editing = editing.clone();
        move || {
            if !active.get() || editing.borrow().is_some() {
                return;
            }
            let question = state.borrow().editor.ok_question();
            match question {
                Some(q) => ask(Question::ManyChanges, "Are you sure?", q),
                None => write(),
            }
        }
    });
    window.on_chosen({
        let state = state.clone();
        let refresh = refresh.clone();
        let tell = tell.clone();
        let edit = edit.clone();
        let paste = paste.clone();
        let write = write.clone();
        let weak = window.as_weak();
        move |index| {
            let asked = state.borrow_mut().asking.take();
            let yes = index == 0;
            match asked.map(|(q, _, _)| q) {
                Some(Question::Domain) => {
                    let domain = weak
                        .upgrade()
                        .map(|w| w.get_asking_text().to_string())
                        .unwrap_or_default();
                    if state.borrow().editor.has_domain(&domain) {
                        tell(vec![Said::Warning(DOMAIN_EXISTS.into())]);
                        return;
                    }
                    let value = state.borrow().editor.new_domain_value();
                    let state = state.clone();
                    let refresh = refresh.clone();
                    edit(
                        value,
                        Box::new(move |value, _| {
                            state.borrow_mut().editor.add_domain(&domain, value);
                            refresh();
                        }),
                    );
                }
                Some(Question::DeleteDomains(domains)) if yes => {
                    let mut state = state.borrow_mut();
                    state.editor.delete_domains(&domains);
                    state.domains = ListSelection::default();
                }
                Some(Question::AllFiles(domains, value)) => {
                    state.borrow_mut().editor.edit_domains(&domains, value, yes);
                }
                Some(Question::Paste(text, mut answers)) => {
                    answers.push(yes);
                    paste(text, answers);
                    return;
                }
                Some(Question::ManyChanges) if yes => {
                    write();
                    return;
                }
                _ => {}
            }
            // (the next error waiting, if any)
            tell(Vec::new());
            refresh();
        }
    });
    window.on_cancelled({
        let state = state.clone();
        let tell = tell.clone();
        move || {
            state.borrow_mut().asking = None;
            tell(Vec::new());
        }
    });
    window.on_cancel({
        let close = close.clone();
        move || close()
    });
    window.window().on_close_requested({
        let close = close.clone();
        move || {
            close();
            slint::CloseRequestResponse::HideWindow
        }
    });
    show(&window, &state.borrow(), &tz);
    window.show().map_err(|e| e.to_string())?;
    Ok(window)
}
