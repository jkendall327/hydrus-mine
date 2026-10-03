//! The manage subscriptions dialog, bound (network > subscriptions…): the
//! subscriptions read from the store into a [`Subscriptions`], its list
//! shown as the reference writes it, its buttons (delete, pause/resume,
//! scrub delays, check queries now, select subscriptions) with the
//! questions they ask, and "apply", which writes what changed back to the
//! store (only that, as the daemon may have run a subscription meanwhile).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};

use hydrus_core::subscriptions::{QueryState, SubscriptionSettings};
use hydrus_store::Store;
use hydrus_store::{queues, subscriptions};

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
}

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
    Rename(i64, String),
    Settings(i64, Box<SubscriptionSettings>),
    Query(i64, QueryState),
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
            continue;
        };
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
                continue;
            };
            let before = read.queries.iter().find(|(qq, _)| *qq == Some(queue));
            if before.is_none_or(|(_, state)| state != &q.state) {
                writes.push(Write::Query(queue, q.state.clone()));
            }
        }
    }
    writes
}

fn write(store: &Store, writes: Vec<Write>) -> hydrus_store::Result<()> {
    store.write(move |ctx| {
        let conn = ctx.conn();
        for change in &writes {
            match change {
                Write::Delete(id) => subscriptions::delete_subscription(conn, *id)?,
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
) -> Result<SubscriptionsWindow, String> {
    let state = Rc::new(RefCell::new(read(store).map_err(|e| e.to_string())?));
    let window = SubscriptionsWindow::new().map_err(|e| e.to_string())?;
    let paused = store
        .read(hydrus_store::settings::get::<hydrus_store::settings::Pauses>)
        .is_ok_and(|p| p.subscriptions);
    window.set_globally_paused(paused);
    let close = {
        let weak = window.as_weak();
        let slot = slot.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            slot.borrow_mut().take();
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
        let weak = window.as_weak();
        move |index| {
            let Ok(index) = usize::try_from(index) else {
                return;
            };
            let text = weak
                .upgrade()
                .map(|w| w.get_asked_text().to_string())
                .unwrap_or_default();
            change(&|open| match open.asking.take() {
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
                None => {}
            });
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
