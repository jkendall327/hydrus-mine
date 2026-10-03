//! The manage subscriptions dialog, bound (network > subscriptions…): the
//! subscriptions read from the store into a [`Subscriptions`], its list
//! shown as the reference writes it, its buttons (add and edit, through
//! the edit subscription dialog; delete, pause/resume, scrub delays, check
//! queries now, select subscriptions) with the questions they ask, and
//! "apply", which writes what changed back to the store (only that, as the
//! daemon may have run a subscription meanwhile).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};

use hydrus_core::subscriptions::{QueryState, SubscriptionSettings};
use hydrus_store::Store;
use hydrus_store::{queues, subscriptions};

use crate::edit_subscription::{EditSubscription, LogChange};
use crate::edit_subscription_window::Slots;
use crate::subscriptions_dialog::{
    CheckNow, Choice, DELETE_QUESTION, DialogQuery, SELECT_MESSAGE, Subscriptions,
};
use crate::subscriptions_list::ShortSummary;
use crate::{SubscriptionsWindow, TableRow};

/// What a question waits on.
enum Asking {
    Delete,
    Select,
    Check(CheckNow),
    /// "add"'s downloader, from these: (key, name).
    Downloader(Vec<(String, String)>),
    /// A message with only "ok".
    Message(String),
}

/// What "add" says with no downloaders (`SelectGUGKeyAndName`).
const NO_DOWNLOADERS: &str = "Hey, you do not have any downloaders set up in this client, so you cannot create a new subscription yet!\n\nCheck the _network->downloaders_ menu to find downloaders made by users.";

/// A subscription as read, to write only what changed.
struct AsRead {
    name: String,
    settings: SubscriptionSettings,
    /// Its queries' states, by queue.
    queries: Vec<(Option<i64>, QueryState)>,
}

/// The dialog's state while it is open.
struct Open {
    dialog: Subscriptions,
    /// Each subscription as read, by id.
    read: HashMap<i64, AsRead>,
    short: ShortSummary,
    asking: Option<Asking>,
}

/// Change the dialog's state, and show it again.
type Change = Rc<dyn Fn(&dyn Fn(&mut Open))>;

fn now() -> i64 {
    hydrus_core::time::TimestampMs::now().millis() / 1000
}

/// Read the subscriptions, with each query's file log.
fn read(store: &Store) -> hydrus_store::Result<Open> {
    store.read(|conn| {
        let mut loaded = Vec::new();
        let mut read = HashMap::new();
        for s in subscriptions::subscriptions(conn)? {
            let mut queries = Vec::new();
            for q in subscriptions::queries(conn, s.id)? {
                let seeds = queues::file_seeds(conn, q.queue_id)?;
                queries.push(DialogQuery {
                    queue: Some(q.queue_id),
                    files: queues::file_seed_counts(conn, q.queue_id)?,
                    seed_times: hydrus_store::watchers::seed_times(&seeds),
                    ignored_notes: seeds
                        .iter()
                        .filter(|s| s.status == queues::SeedStatus::Vetoed)
                        .map(|s| s.note.clone())
                        .collect(),
                    log_changes: Vec::new(),
                    state: q.state,
                });
            }
            read.insert(
                s.id,
                AsRead {
                    name: s.name.clone(),
                    settings: s.settings.clone(),
                    queries: queries.iter().map(|q| (q.queue, q.state.clone())).collect(),
                },
            );
            loaded.push((Some(s.id), s.name, s.settings, queries));
        }
        let naming: hydrus_core::pages::PageNameSettings = hydrus_store::settings::get(conn)?;
        Ok(Open {
            dialog: Subscriptions::new(loaded),
            read,
            short: ShortSummary {
                new: naming.short_summary_new,
                deleted: naming.short_summary_deleted,
            },
            asking: None,
        })
    })
}

/// A change "apply" writes.
enum Write {
    Delete(i64),
    /// A new subscription, with its queries' states.
    Create(String, Box<SubscriptionSettings>, Vec<QueryState>),
    Rename(i64, String),
    Settings(i64, Box<SubscriptionSettings>),
    Query(i64, QueryState),
    AddQuery(i64, QueryState),
    RemoveQuery(i64),
    /// A query's file log changed.
    Log(i64, LogChange),
}

/// What the dialog changed: deleted subscriptions, and changed names,
/// settings and query states.
fn changes(open: &Open) -> Vec<Write> {
    let mut writes: Vec<Write> = open
        .dialog
        .deleted
        .iter()
        .map(|&id| Write::Delete(id))
        .collect();
    for s in &open.dialog.subscriptions {
        let Some(id) = s.id else {
            writes.push(Write::Create(
                s.name.clone(),
                Box::new(s.settings.clone()),
                s.queries.iter().map(|q| q.state.clone()).collect(),
            ));
            continue;
        };
        writes.extend(s.deleted_queries.iter().map(|&q| Write::RemoveQuery(q)));
        let Some(read) = open.read.get(&id) else {
            continue;
        };
        if s.name != read.name {
            writes.push(Write::Rename(id, s.name.clone()));
        }
        if s.settings != read.settings {
            writes.push(Write::Settings(id, Box::new(s.settings.clone())));
        }
        for q in &s.queries {
            let Some(queue) = q.queue else {
                writes.push(Write::AddQuery(id, q.state.clone()));
                continue;
            };
            writes.extend(q.log_changes.iter().map(|&c| Write::Log(queue, c)));
            let before = read.queries.iter().find(|(qq, _)| *qq == Some(queue));
            if before.is_none_or(|(_, state)| state != &q.state) {
                writes.push(Write::Query(queue, q.state.clone()));
            }
        }
    }
    writes
}

/// Carry out a change to a query's file log.
fn change_log(
    conn: &rusqlite::Connection,
    queue: i64,
    change: LogChange,
    now: i64,
) -> hydrus_store::Result<()> {
    use queues::SeedStatus;
    match change {
        LogChange::Reset => {
            let all: Vec<SeedStatus> = (0..=10).filter_map(SeedStatus::from_code).collect();
            queues::remove_file_seeds(conn, queue, &all)?;
        }
        LogChange::RetryFailed => {
            queues::retry_file_seeds(conn, queue, &[SeedStatus::Error], now)?;
        }
        LogChange::RetryIgnored(which) => {
            for mut seed in queues::file_seeds(conn, queue)? {
                if seed.status == SeedStatus::Vetoed && which.matches(&seed.note) {
                    seed.status = SeedStatus::Unknown;
                    seed.note.clear();
                    seed.modified = now;
                    queues::update_file_seed(conn, &seed)?;
                }
            }
        }
    }
    Ok(())
}

fn write(store: &Store, writes: Vec<Write>) -> hydrus_store::Result<()> {
    let now = now();
    store.write(move |ctx| {
        let conn = ctx.conn();
        for change in &writes {
            match change {
                Write::Delete(id) => subscriptions::delete_subscription(conn, *id)?,
                Write::Create(name, settings, queries) => {
                    if let Some(id) = subscriptions::create_subscription(conn, name, settings)? {
                        for q in queries {
                            subscriptions::add_query(conn, id, q, now)?;
                        }
                    }
                }
                Write::AddQuery(id, state) => {
                    subscriptions::add_query(conn, *id, state, now)?;
                }
                Write::RemoveQuery(queue) => subscriptions::remove_query(conn, *queue)?,
                Write::Log(queue, change) => change_log(conn, *queue, *change, now)?,
                Write::Rename(id, name) => {
                    subscriptions::rename_subscription(conn, *id, name)?;
                }
                Write::Settings(id, settings) => {
                    subscriptions::set_subscription_settings(conn, *id, settings)?;
                }
                Write::Query(queue, state) => subscriptions::set_query_state(conn, *queue, state)?,
            }
        }
        Ok(())
    })
}

/// Show the dialog's list, buttons and question.
fn show(window: &SubscriptionsWindow, open: &Open) {
    let now = now();
    let dialog = &open.dialog;
    let rows: Vec<TableRow> = dialog
        .rows(now, open.short)
        .into_iter()
        .map(|(_, cells, selected)| {
            let cells: Vec<SharedString> = cells.into_iter().map(Into::into).collect();
            TableRow {
                cells: ModelRc::new(VecModel::from(cells)),
                selected,
            }
        })
        .collect();
    window.set_rows(ModelRc::new(VecModel::from(rows)));
    window.set_sort_column(i32::try_from(dialog.sort.0).unwrap_or(0));
    window.set_ascending(dialog.sort.1);
    window.set_any_selected(!dialog.selection.is_empty());
    window.set_one_selected(dialog.selection.one().is_some());
    window.set_can_check_now(dialog.can_check_now(now));
    window.set_can_scrub_delays(dialog.can_scrub_delays(now));
    let question = match &open.asking {
        None => None,
        Some(Asking::Delete) => Some((
            Choice {
                title: "Are you sure?".into(),
                message: DELETE_QUESTION.into(),
                choices: vec!["yes".into(), "no".into()],
            },
            false,
        )),
        Some(Asking::Select) => Some((
            Choice {
                title: "Enter text".into(),
                message: SELECT_MESSAGE.into(),
                choices: vec!["ok".into()],
            },
            true,
        )),
        Some(Asking::Check(check)) => check.question(dialog).map(|c| (c, false)),
        Some(Asking::Downloader(gugs)) => Some((
            Choice {
                title: "select gallery".into(),
                message: String::new(),
                choices: gugs.iter().map(|(_, name)| name.clone()).collect(),
            },
            false,
        )),
        Some(Asking::Message(message)) => Some((
            Choice {
                title: "Warning".into(),
                message: message.clone(),
                choices: vec!["ok".into()],
            },
            false,
        )),
    };
    window.set_asking(question.is_some());
    if let Some((choice, wants_text)) = question {
        window.set_asking_title(choice.title.into());
        window.set_asking_message(choice.message.into());
        let choices: Vec<SharedString> = choice.choices.into_iter().map(Into::into).collect();
        window.set_asking_choices(ModelRc::new(VecModel::from(choices)));
        window.set_asking_text(wants_text);
    }
}

/// Open the dialog on the store's subscriptions; it forgets itself from
/// `slot` when closed.
pub(crate) fn open(
    store: &Arc<Store>,
    slot: &Rc<RefCell<Option<SubscriptionsWindow>>>,
    edit_slots: Slots,
) -> Result<SubscriptionsWindow, String> {
    let edit_slots = Rc::new(edit_slots);
    let state = Rc::new(RefCell::new(read(store).map_err(|e| e.to_string())?));
    let window = SubscriptionsWindow::new().map_err(|e| e.to_string())?;
    let paused = store
        .read(hydrus_store::settings::get::<hydrus_store::settings::Pauses>)
        .is_ok_and(|p| p.subscriptions);
    window.set_globally_paused(paused);
    let close = {
        let weak = window.as_weak();
        let slot = slot.clone();
        let edit = edit_slots.edit.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            slot.borrow_mut().take();
            // (the edit dialog goes with it)
            if let Some(window) = edit.borrow_mut().take() {
                let _ = window.hide();
            }
        }
    };
    // (each change shown again)
    let change = {
        let weak = window.as_weak();
        let state = state.clone();
        move |f: &dyn Fn(&mut Open)| {
            f(&mut state.borrow_mut());
            if let Some(window) = weak.upgrade() {
                show(&window, &state.borrow());
            }
        }
    };
    let change: Change = Rc::new(change);
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
    // the edit subscription dialog, on a subscription in the list or a new
    // one; what it gives back replaces the one edited, or joins the list
    let edit = {
        let state = state.clone();
        let store = store.clone();
        let change = change.clone();
        let slots = edit_slots.clone();
        move |key: Option<u64>, settings: SubscriptionSettings| {
            if slots.edit.borrow().is_some() {
                return;
            }
            let dialog = {
                let open = state.borrow();
                match key.and_then(|k| open.dialog.get(k)) {
                    Some(s) => {
                        EditSubscription::new(s.name.clone(), s.settings.clone(), s.queries.clone())
                    }
                    None => EditSubscription::new("new subscription".into(), settings, Vec::new()),
                }
            };
            let done: Rc<dyn Fn(EditSubscription)> = {
                let change = change.clone();
                Rc::new(move |edited: EditSubscription| {
                    let (name, settings, queries, deleted) = edited.into_parts(now());
                    change(&|open| match key {
                        Some(k) => open.dialog.replace_edited(
                            k,
                            &name,
                            settings.clone(),
                            queries.clone(),
                            deleted.clone(),
                        ),
                        None => {
                            open.dialog
                                .add_edited(&name, settings.clone(), queries.clone());
                        }
                    });
                })
            };
            match crate::edit_subscription_window::open(&store, dialog, &slots, done) {
                Ok(window) => *slots.edit.borrow_mut() = Some(window),
                Err(e) => eprintln!("could not open the subscription: {e}"),
            }
        }
    };
    let edit: Rc<dyn Fn(Option<u64>, SubscriptionSettings)> = Rc::new(edit);
    window.on_edit({
        let edit = edit.clone();
        let state = state.clone();
        move || {
            let one = state.borrow().dialog.selection.one();
            if one.is_some() {
                edit(one, SubscriptionSettings::default());
            }
        }
    });
    window.on_row_activated({
        let edit = edit.clone();
        let state = state.clone();
        move |row| {
            let key = usize::try_from(row)
                .ok()
                .and_then(|r| state.borrow().dialog.order(now()).get(r).copied());
            if key.is_some() {
                edit(key, SubscriptionSettings::default());
            }
        }
    });
    window.on_add({
        let change = change.clone();
        let store = store.clone();
        move || {
            let gugs = store
                .read(hydrus_store::settings::get::<hydrus_parse::Downloaders>)
                .map(|d| crate::gallery::offered_gugs(&d.gugs))
                .unwrap_or_default();
            change(&|open| {
                open.asking = Some(if gugs.is_empty() {
                    Asking::Message(NO_DOWNLOADERS.into())
                } else {
                    Asking::Downloader(gugs.iter().map(|g| (g.0.clone(), g.1.clone())).collect())
                });
            });
        }
    });
    window.on_pause_resume({
        let change = change.clone();
        move || change(&|open| open.dialog.pause_resume(now()))
    });
    window.on_scrub_delays({
        let change = change.clone();
        move || change(&|open| open.dialog.scrub_delays(now()))
    });
    window.on_delete({
        let change = change.clone();
        move || change(&|open| open.asking = Some(Asking::Delete))
    });
    window.on_select_subscriptions({
        let change = change.clone();
        let weak = window.as_weak();
        move || {
            if let Some(window) = weak.upgrade() {
                window.set_asked_text(SharedString::new());
            }
            change(&|open| open.asking = Some(Asking::Select));
        }
    });
    window.on_check_now({
        let change = change.clone();
        move || {
            change(&|open| {
                let mut check = CheckNow::new(&open.dialog, now());
                // (nothing to ask: done at once)
                if check.next(&open.dialog).is_none() {
                    check.apply(&mut open.dialog);
                } else {
                    open.asking = Some(Asking::Check(check));
                }
            });
        }
    });
    window.on_chosen({
        let change = change.clone();
        let edit = edit.clone();
        let store = store.clone();
        let weak = window.as_weak();
        move |index| {
            let Ok(index) = usize::try_from(index) else {
                return;
            };
            let text = weak
                .upgrade()
                .map(|w| w.get_asked_text().to_string())
                .unwrap_or_default();
            let new_sub = RefCell::new(None);
            change(&|open| match open.asking.take() {
                Some(Asking::Downloader(gugs)) => {
                    *new_sub.borrow_mut() = gugs.get(index).cloned();
                }
                Some(Asking::Delete) => {
                    if index == 0 {
                        open.dialog.delete_selected(now());
                    }
                }
                Some(Asking::Select) => open.dialog.select_by_text(now(), &text),
                Some(Asking::Check(mut check)) => {
                    check.answer(&open.dialog, index);
                    if check.next(&open.dialog).is_none() {
                        check.apply(&mut open.dialog);
                    } else {
                        open.asking = Some(Asking::Check(check));
                    }
                }
                Some(Asking::Message(_)) | None => {}
            });
            // "add": a new subscription on the downloader chosen, with the
            // client's checker timings for subscriptions
            if let Some((gug_key, gug_name)) = new_sub.into_inner() {
                let checker = store
                    .read(
                        hydrus_store::settings::get::<hydrus_core::subscriptions::CheckerDefaults>,
                    )
                    .map(|d| d.subscriptions)
                    .unwrap_or_default();
                edit(
                    None,
                    SubscriptionSettings {
                        gug_key,
                        gug_name,
                        checker,
                        ..SubscriptionSettings::default()
                    },
                );
            }
        }
    });
    window.on_cancelled({
        let change = change.clone();
        move || change(&|open| open.asking = None)
    });
    window.on_apply({
        let state = state.clone();
        let store = store.clone();
        let close = close.clone();
        move || {
            let writes = changes(&state.borrow());
            if let Err(e) = write(&store, writes) {
                eprintln!("could not save the subscriptions: {e}");
            }
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
