//! The edit subscription dialog, bound: an [`EditSubscription`] shown as
//! the reference's `EditSubscriptionPanel`, its query list's buttons with
//! the questions they ask (in the window's own question panel), the query
//! editor in the window's place while a query is edited, the downloader
//! chosen from the client's, the checker options editor, and "apply",
//! which gives the subscription back to the manage subscriptions dialog
//! (nothing is written to the store until that dialog's "apply").

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;
use std::sync::Arc;

use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};

use hydrus_core::subscriptions::{CheckerOptions, QueryState};
use hydrus_store::Store;
use hydrus_store::queues;

use crate::edit_subscription::{
    CheckQueriesNow, EMPTY_CLIPBOARD, EditSubscription, Paste, PastePlan, RESET_QUESTION,
    RetryIgnored,
};
use crate::subscriptions_dialog::{Choice, DELETE_QUESTION};
use crate::subscriptions_list::{QueryFacts, ShortSummary, next_check_status};
use crate::{CheckerOptionsWindow, EditSubscriptionWindow, TableRow};

/// What a question waits on.
enum Asking {
    Delete,
    Reset,
    RetryIgnored,
    /// A pasting's question, with its plan: its message, its yeses (the
    /// first revives the DEAD when there are two) and its no.
    Paste {
        plan: PastePlan,
        message: String,
        yeses: Vec<String>,
        no: String,
    },
    /// A message with only "ok".
    Message(Choice),
    Check(CheckQueriesNow),
    /// The downloaders to choose from: (key, name).
    Downloader(Vec<(String, String)>),
}

/// The query editor's query: the one edited (none for "add") and its file
/// and search logs' status lines.
struct Editing {
    key: Option<u64>,
    state: QueryState,
}

/// Change the dialog's state, and show it again.
type Change = Rc<dyn Fn(&dyn Fn(&mut Open))>;

/// The dialog's state while it is open.
struct Open {
    dialog: EditSubscription,
    original_queues: BTreeSet<i64>,
    /// The client's downloaders: (key, name, initial search text).
    gugs: Vec<(String, String, String)>,
    short: ShortSummary,
    asking: Option<Asking>,
    editing: Option<Editing>,
}

fn now() -> i64 {
    hydrus_core::time::TimestampMs::now().millis() / 1000
}

fn strings(items: Vec<String>) -> ModelRc<SharedString> {
    let items: Vec<SharedString> = items.into_iter().map(Into::into).collect();
    ModelRc::new(VecModel::from(items))
}

/// The settings fields, from the dialog (as it opens).
fn show_fields(window: &EditSubscriptionWindow, dialog: &EditSubscription) {
    let s = &dialog.settings;
    let limit = |n: Option<u64>| i32::try_from(n.unwrap_or(100)).unwrap_or(i32::MAX);
    window.set_name(dialog.name.clone().into());
    window.set_paused(s.paused);
    window.set_initial_limit(limit(s.initial_file_limit));
    window.set_periodic_limit(limit(s.periodic_file_limit));
    window.set_random_sample(s.this_is_a_random_sample);
    window.set_show_popup(s.show_a_popup_while_working);
    window.set_publish_popup_button(s.publish_files_to_popup_button);
    window.set_publish_page(s.publish_files_to_page);
    window.set_label_override(s.publish_label_override.clone().unwrap_or_default().into());
    window.set_label_override_none(s.publish_label_override.is_none());
    window.set_merge_publish(s.merge_query_publish_events);
    window.set_import_options(
        crate::edit_subscription::import_options_label(&s.import_options).into(),
    );
}

/// The settings fields, as edited, into the dialog.
fn read_fields(window: &EditSubscriptionWindow, dialog: &mut EditSubscription) {
    let s = &mut dialog.settings;
    let limit = |n: i32| Some(u64::try_from(n.max(1)).unwrap_or(1));
    dialog.name = window.get_name().into();
    s.paused = window.get_paused();
    s.initial_file_limit = limit(window.get_initial_limit());
    s.periodic_file_limit = limit(window.get_periodic_limit());
    s.this_is_a_random_sample = window.get_random_sample();
    s.show_a_popup_while_working = window.get_show_popup();
    s.publish_files_to_popup_button = window.get_publish_popup_button();
    s.publish_files_to_page = window.get_publish_page();
    s.publish_label_override =
        (!window.get_label_override_none()).then(|| window.get_label_override().into());
    s.merge_query_publish_events = window.get_merge_publish();
}

/// The query editor's status line, from its check now and paused boxes.
fn query_status(window: &EditSubscriptionWindow, editing: &Editing) -> String {
    let facts = QueryFacts {
        check_now: window.get_query_check_now(),
        paused: window.get_query_paused(),
        dead: editing.state.dead,
        next_check_time: editing.state.next_check_time,
        ..QueryFacts::default()
    };
    format!("next check: {}", next_check_status(&facts, now()))
}

/// Show the query editor on a query.
fn show_editor(window: &EditSubscriptionWindow, store: &Store, open: &Open, editing: &Editing) {
    let state = &editing.state;
    window.set_query_text(state.query_text.clone().into());
    window.set_display_name(state.display_name.clone().unwrap_or_default().into());
    window.set_display_name_none(state.display_name.is_none());
    window.set_query_check_now(state.check_now);
    window.set_query_paused(state.paused);
    window.set_query_status(query_status(window, editing).into());
    let query = editing.key.and_then(|k| open.dialog.get(k));
    let files = query.map(|q| q.query.files.clone()).unwrap_or_default();
    let searches = query
        .and_then(|q| q.query.queue)
        .and_then(|queue| store.read(|c| queues::gallery_seed_counts(c, queue)).ok())
        .unwrap_or_default();
    window.set_query_has_logs(query.and_then(|q| q.query.queue).is_some());
    window.set_query_file_log(queues::file_log_status(&files).into());
    window.set_query_search_log(queues::search_log_status(&searches).0.into());
}

/// Show the list, the delay and the question.
fn show(window: &EditSubscriptionWindow, open: &Open) {
    let now = now();
    let dialog = &open.dialog;
    let rows: Vec<TableRow> = dialog
        .rows(now, open.short)
        .into_iter()
        .map(|(_, cells, selected)| TableRow {
            cells: strings(cells),
            selected,
        })
        .collect();
    window.set_rows(ModelRc::new(VecModel::from(rows)));
    window.set_sort_column(i32::try_from(dialog.sort.0).unwrap_or(0));
    window.set_ascending(dialog.sort.1);
    window.set_any_selected(!dialog.selection.is_empty());
    window.set_can_quality(
        !hydrus_gui_model::subscription_quality::selected(dialog, &open.original_queues, now)
            .is_empty(),
    );
    window.set_one_selected(dialog.selection.one().is_some());
    window.set_can_check_now(dialog.can_check_now(now));
    window.set_can_reset(dialog.can_reset(now));
    window.set_can_retry_failed(dialog.can_retry_failed(now));
    window.set_can_retry_ignored(dialog.can_retry_ignored(now));
    window.set_delay(dialog.delay_text(now).into());
    let found = open
        .gugs
        .iter()
        .any(|(key, name, _)| *key == dialog.settings.gug_key || *name == dialog.settings.gug_name);
    window.set_downloader(dialog.downloader_label(found).into());
    window.set_editing_query(open.editing.is_some());
    let yes_no = |message: &str| Choice {
        title: "Are you sure?".into(),
        message: message.into(),
        choices: vec!["yes".into(), "no".into()],
    };
    let question = match &open.asking {
        None => None,
        Some(Asking::Delete) => Some(yes_no(DELETE_QUESTION)),
        Some(Asking::Reset) => Some(yes_no(RESET_QUESTION)),
        Some(Asking::RetryIgnored) => Some(Choice {
            title: RetryIgnored::TITLE.into(),
            message: String::new(),
            choices: RetryIgnored::CHOICES
                .iter()
                .map(|(_, label)| (*label).to_owned())
                .collect(),
        }),
        Some(Asking::Paste {
            message, yeses, no, ..
        }) => {
            let mut choices = yeses.clone();
            choices.push(no.clone());
            Some(Choice {
                title: "Are you sure?".into(),
                message: message.clone(),
                choices,
            })
        }
        Some(Asking::Message(choice)) => Some(choice.clone()),
        Some(Asking::Check(check)) => check.question(dialog),
        Some(Asking::Downloader(gugs)) => Some(Choice {
            title: "select gallery".into(),
            message: String::new(),
            choices: gugs.iter().map(|(_, name)| name.clone()).collect(),
        }),
    };
    window.set_asking(question.is_some());
    if let Some(choice) = question {
        window.set_asking_title(choice.title.into());
        window.set_asking_message(choice.message.into());
        window.set_asking_choices(strings(choice.choices));
    }
}

/// Read the clipboard's text.
fn clipboard_text() -> Result<String, String> {
    crate::from_clipboard()
}

/// Where the dialog is opened from: the window's slot, the checker
/// options editor's, and a query's log window's (and where it shows
/// files).
pub(crate) struct Slots {
    pub edit: Rc<RefCell<Option<EditSubscriptionWindow>>>,
    pub checker: Rc<RefCell<Option<CheckerOptionsWindow>>>,
    pub log: Rc<RefCell<Option<crate::FileLogWindow>>>,
    pub open_files: crate::file_log_window::OpenFiles,
    pub import_options: Rc<RefCell<Option<crate::ImportOptionsWindow>>>,
}

/// Open the dialog on a subscription; on "apply" it gives the edited
/// subscription to `done`. It forgets itself from its slot when closed.
pub(crate) fn open(
    store: &Arc<Store>,
    dialog: EditSubscription,
    slots: &Slots,
    done: Rc<dyn Fn(EditSubscription)>,
) -> Result<EditSubscriptionWindow, String> {
    let (definitions, naming, advanced) = store
        .read(|c| {
            Ok((
                hydrus_store::settings::get::<hydrus_parse::Downloaders>(c)?,
                hydrus_store::settings::get::<hydrus_core::pages::PageNameSettings>(c)?,
                hydrus_store::settings::get::<hydrus_store::settings::AdvancedMode>(c)?,
            ))
        })
        .unwrap_or_default();
    let window = EditSubscriptionWindow::new().map_err(|e| e.to_string())?;
    window.set_most_files(if advanced.0 { 50000 } else { 1000 });
    show_fields(&window, &dialog);
    let original_queues = dialog
        .queries
        .iter()
        .filter_map(|q| q.query.queue)
        .collect();
    let state = Rc::new(RefCell::new(Open {
        dialog,
        original_queues,
        gugs: crate::gallery::offered_gugs(&definitions.gugs),
        short: ShortSummary {
            new: naming.short_summary_new,
            deleted: naming.short_summary_deleted,
        },
        asking: None,
        editing: None,
    }));
    let quality = crate::subscription_quality_control::bind(
        &window,
        store,
        advanced.0,
        Rc::new({
            let state = state.clone();
            move || {
                let open = state.borrow();
                hydrus_gui_model::subscription_quality::selected(
                    &open.dialog,
                    &open.original_queues,
                    now(),
                )
            }
        }),
        Rc::new({
            let state = state.clone();
            let weak = window.as_weak();
            move |text| {
                state.borrow_mut().asking = Some(Asking::Message(Choice {
                    title: "Information".into(),
                    message: text,
                    choices: vec!["ok".into()],
                }));
                if let Some(w) = weak.upgrade() {
                    show(&w, &state.borrow());
                }
            }
        }),
    );
    let close = {
        let quality = quality.clone();
        let weak = window.as_weak();
        let slot = slots.edit.clone();
        move || {
            quality.cancel();
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            slot.borrow_mut().take();
        }
    };
    // (each change shown again)
    let change: Change = {
        let weak = window.as_weak();
        let state = state.clone();
        Rc::new(move |f: &dyn Fn(&mut Open)| {
            if let Some(w) = weak.upgrade()
                && w.get_quality_working()
            {
                return;
            }
            f(&mut state.borrow_mut());
            if let Some(window) = weak.upgrade() {
                show(&window, &state.borrow());
            }
        })
    };
    // the edited query's logs, opened on its queue (changes made there are
    // made at once)
    window.on_query_log({
        let state = state.clone();
        let store = store.clone();
        let log = slots.log.clone();
        let open_files = slots.open_files.clone();
        move |search| {
            let queue = {
                let open = state.borrow();
                open.editing
                    .as_ref()
                    .and_then(|e| e.key)
                    .and_then(|k| open.dialog.get(k))
                    .and_then(|q| q.query.queue)
            };
            let Some(queue) = queue else {
                return;
            };
            let old = log.borrow_mut().take();
            if let Some(old) = old {
                old.invoke_close_window();
            }
            let opened = if search {
                crate::search_log_window::open(&store, queue, &log)
            } else {
                crate::file_log_window::open(&store, queue, &log, &open_files)
            };
            match opened {
                Ok(window) => *log.borrow_mut() = Some(window),
                Err(e) => eprintln!("could not open the log: {e}"),
            }
        }
    });
    // the subscription's import options, in the editor (a subscription's
    // defaults)
    window.on_edit_import_options({
        let weak = window.as_weak();
        let state = state.clone();
        let store = store.clone();
        let editor_slot = slots.import_options.clone();
        move || {
            if editor_slot.borrow().is_some() {
                return;
            }
            let own = state.borrow().dialog.settings.import_options.clone();
            let done: Rc<dyn Fn(hydrus_core::import_options::ImportOptionsSlice)> = {
                let weak = weak.clone();
                let state = state.clone();
                Rc::new(move |options| {
                    if let Some(window) = weak.upgrade() {
                        window.set_import_options(
                            crate::edit_subscription::import_options_label(&options).into(),
                        );
                    }
                    state.borrow_mut().dialog.settings.import_options = options;
                })
            };
            match crate::import_options_window::open(
                &store,
                hydrus_core::import_options::CallerType::Subscription,
                &own,
                &editor_slot,
                done,
            ) {
                Ok(editor) => *editor_slot.borrow_mut() = Some(editor),
                Err(e) => eprintln!("could not open the import options: {e}"),
            }
        }
    });
    window.on_sort({
        let change = change.clone();
        move |column, ascending| {
            if let Ok(column) = usize::try_from(column) {
                change(&|open| open.dialog.sort = (column, ascending));
            }
        }
    });
    window.on_row_clicked({
        let change = change.clone();
        move |row, ctrl, shift| {
            if let Ok(row) = usize::try_from(row) {
                change(&|open| open.dialog.click(now(), row, ctrl, shift));
            }
        }
    });
    // the query editor, on a query or (for "add") a new one
    let edit = {
        let weak = window.as_weak();
        let state = state.clone();
        let store = store.clone();
        move |key: Option<u64>| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let editing = {
                let open = state.borrow();
                let query_state = if let Some(k) = key {
                    let Some(q) = open.dialog.get(k) else {
                        return;
                    };
                    q.query.state.clone()
                } else {
                    let initial = open
                        .gugs
                        .iter()
                        .find(|(_, name, _)| *name == open.dialog.settings.gug_name)
                        .map(|g| g.2.clone())
                        .unwrap_or_default();
                    QueryState::new(initial)
                };
                Editing {
                    key,
                    state: query_state,
                }
            };
            show_editor(&window, &store, &state.borrow(), &editing);
            state.borrow_mut().editing = Some(editing);
            show(&window, &state.borrow());
        }
    };
    window.on_add_query({
        let edit = edit.clone();
        move || edit(None)
    });
    window.on_edit_query({
        let edit = edit.clone();
        let state = state.clone();
        move || {
            let one = state.borrow().dialog.selection.one();
            if one.is_some() {
                edit(one);
            }
        }
    });
    window.on_row_activated({
        let edit = edit.clone();
        let state = state.clone();
        move |row| {
            let key = {
                let open = state.borrow();
                usize::try_from(row)
                    .ok()
                    .and_then(|r| open.dialog.order(now()).get(r).copied())
            };
            if key.is_some() {
                edit(key);
            }
        }
    });
    window.on_query_status_changed({
        let weak = window.as_weak();
        let state = state.clone();
        move || {
            if let (Some(window), Some(editing)) = (weak.upgrade(), &state.borrow().editing) {
                window.set_query_status(query_status(&window, editing).into());
            }
        }
    });
    window.on_query_apply({
        let change = change.clone();
        let weak = window.as_weak();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let text: String = window.get_query_text().trim().to_owned();
            let display: String = window.get_display_name().into();
            let none = window.get_display_name_none();
            let check_now = window.get_query_check_now();
            let paused = window.get_query_paused();
            change(&|open| {
                let Some(editing) = open.editing.take() else {
                    return;
                };
                let mut edited = editing.state.clone();
                edited.query_text.clone_from(&text);
                edited.display_name = (!none && !display.is_empty()).then(|| display.clone());
                edited.check_now = check_now;
                edited.paused = paused;
                let result = match editing.key {
                    Some(key) => open.dialog.edit_query(key, edited),
                    None => open.dialog.add_query(edited).map(|_| ()),
                };
                if let Err(warning) = result {
                    open.asking = Some(Asking::Message(message(&warning)));
                }
            });
        }
    });
    window.on_query_cancel({
        let change = change.clone();
        move || change(&|open| open.editing = None)
    });
    window.on_copy_queries({
        let state = state.clone();
        move || {
            let text = state.borrow().dialog.copy_queries(now());
            if !text.is_empty() {
                crate::copy_to_clipboard(&text);
            }
        }
    });
    window.on_paste_queries({
        let change = change.clone();
        let state = state.clone();
        move || {
            let text = match clipboard_text() {
                Ok(text) => text,
                Err(e) => {
                    change(&|open| {
                        open.asking = Some(Asking::Message(Choice {
                            title: "Problem pasting!".into(),
                            message: e.clone(),
                            choices: vec!["ok".into()],
                        }));
                    });
                    return;
                }
            };
            let paste = state.borrow().dialog.paste(&text);
            match paste {
                Paste::Empty => {
                    change(&|open| open.asking = Some(Asking::Message(message(EMPTY_CLIPBOARD))));
                }
                Paste::Nothing(text) => {
                    change(&|open| open.asking = Some(Asking::Message(message(&text))));
                }
                Paste::Ask {
                    message,
                    yeses,
                    no,
                    plan,
                } => change(&|open| {
                    open.asking = Some(Asking::Paste {
                        plan: plan.clone(),
                        message: message.clone(),
                        yeses: yeses.clone(),
                        no: no.clone(),
                    });
                }),
            }
        }
    });
    window.on_delete_queries({
        let change = change.clone();
        move || change(&|open| open.asking = Some(Asking::Delete))
    });
    window.on_reset({
        let change = change.clone();
        move || change(&|open| open.asking = Some(Asking::Reset))
    });
    window.on_retry_ignored({
        let change = change.clone();
        move || change(&|open| open.asking = Some(Asking::RetryIgnored))
    });
    window.on_retry_failed({
        let change = change.clone();
        move || change(&|open| open.dialog.retry_failed(now()))
    });
    window.on_pause_play({
        let change = change.clone();
        move || change(&|open| open.dialog.pause_play(now()))
    });
    window.on_check_now({
        let change = change.clone();
        move || {
            change(&|open| {
                let mut check = CheckQueriesNow::new(&open.dialog, now());
                // (nothing to ask: done at once)
                if check.next(&open.dialog).is_none() {
                    check.apply(&mut open.dialog);
                } else {
                    open.asking = Some(Asking::Check(check));
                }
            });
        }
    });
    window.on_choose_downloader({
        let change = change.clone();
        move || {
            change(&|open| {
                let gugs = open
                    .gugs
                    .iter()
                    .map(|(key, name, _)| (key.clone(), name.clone()))
                    .collect();
                open.asking = Some(Asking::Downloader(gugs));
            });
        }
    });
    window.on_chosen({
        let change = change.clone();
        move |index| {
            let Ok(index) = usize::try_from(index) else {
                return;
            };
            change(&|open| match open.asking.take() {
                Some(Asking::Delete) => {
                    if index == 0 {
                        open.dialog.delete_selected(now());
                    }
                }
                Some(Asking::Reset) => {
                    if index == 0 {
                        open.dialog.reset_selected(now());
                    }
                }
                Some(Asking::RetryIgnored) => {
                    if let Some((which, _)) = RetryIgnored::CHOICES.get(index) {
                        open.dialog.retry_ignored(now(), *which);
                    }
                }
                Some(Asking::Paste { plan, yeses, .. }) => {
                    // (the last choice is "hold off")
                    if index < yeses.len() {
                        open.dialog.apply_paste(&plan, index == 0);
                    }
                }
                Some(Asking::Check(mut check)) => {
                    check.answer(&open.dialog, index);
                    if check.next(&open.dialog).is_none() {
                        check.apply(&mut open.dialog);
                    } else {
                        open.asking = Some(Asking::Check(check));
                    }
                }
                Some(Asking::Downloader(gugs)) => {
                    if let Some((key, name)) = gugs.get(index) {
                        open.dialog.settings.gug_key.clone_from(key);
                        open.dialog.settings.gug_name.clone_from(name);
                    }
                }
                Some(Asking::Message(_)) | None => {}
            });
        }
    });
    window.on_cancelled({
        let change = change.clone();
        move || change(&|open| open.asking = None)
    });
    window.on_edit_checker({
        let state = state.clone();
        let change = change.clone();
        let slot = slots.checker.clone();
        let store = store.clone();
        move || {
            if slot.borrow().is_some() {
                return;
            }
            let current = state.borrow().dialog.settings.checker.clone();
            // (the queries' check times reckoned again)
            let applied: Rc<dyn Fn(CheckerOptions)> = {
                let change = change.clone();
                Rc::new(move |checker| {
                    change(&|open| open.dialog.set_checker(checker.clone(), now()));
                })
            };
            let advanced = store
                .read(hydrus_store::settings::get::<hydrus_store::settings::AdvancedMode>)
                .is_ok_and(|a| a.0);
            match crate::checker_options_window::open(&current, advanced, &slot, &applied) {
                Ok(window) => *slot.borrow_mut() = Some(window),
                Err(e) => eprintln!("could not open the checker options: {e}"),
            }
        }
    });
    window.on_apply({
        let state = state.clone();
        let weak = window.as_weak();
        let close = close.clone();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            if window.get_quality_working() {
                return;
            }
            let mut open = state.borrow_mut();
            read_fields(&window, &mut open.dialog);
            let dialog = open.dialog.clone();
            drop(open);
            done(dialog);
            close();
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
    show(&window, &state.borrow());
    window.show().map_err(|e| e.to_string())?;
    Ok(window)
}

/// A message with only "ok".
fn message(text: &str) -> Choice {
    Choice {
        title: "Warning".into(),
        message: text.into(),
        choices: vec!["ok".into()],
    }
}
