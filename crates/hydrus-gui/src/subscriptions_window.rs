//! The manage subscriptions dialog, bound (network > subscriptions…): the
//! subscriptions read from the store into a [`Subscriptions`], its list
//! shown as the reference writes it, its buttons (add and edit, through
//! the edit subscription dialog; delete, merge, separate, lowercase,
//! pause/resume, scrub delays, check queries now, retry, reset, select
//! subscriptions, overwrite downloader and checker options) with the
//! questions they ask, and
//! "apply", which writes what changed back to the store (only that, as the
//! daemon may have run a subscription meanwhile).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};

use hydrus_core::subscriptions::{QueryState, SubscriptionSettings};
use hydrus_store::Store;
use hydrus_store::{queues, subscriptions};

use crate::edit_subscription::{EditSubscription, LogChange, RetryIgnored};
use crate::edit_subscription_window::Slots;
use crate::subscriptions_dedupe::{Answer, Dedupe, Question};
use crate::subscriptions_dialog::{
    CheckNow, Choice, DELETE_QUESTION, DialogQuery, ImportOptionsPaste, LOWERCASE_QUESTION,
    MERGE_PRIMARY, MERGE_QUESTION, NOT_MERGEABLE, Picked, RESET_QUESTION, SELECT_MESSAGE,
    SEPARATE_CHOICES, SEPARATE_MERGED_CHOICES, SEPARATE_MERGED_NAME, SEPARATE_MERGED_QUESTION,
    SEPARATE_NAME, SEPARATE_PICK, SEPARATE_QUESTION, Separate, Subscriptions, added_message,
    picked,
};
use crate::subscriptions_list::ShortSummary;
use crate::{SubscriptionGalleryWindow, SubscriptionsWindow, TableRow, Tick};

/// The reference warns before using a custom overwrite with several selected rows.
const MULTIPLE_FAVOURITE_LOAD: &str = "Hey, multiple items in the subscriptions list are selected. I am only going to do this on the topmost selected.  If you need to do this to multiple entries, set up one exactly how you want and then copy/replace-paste to the rest.";

/// What a question waits on.
enum Asking {
    MissingHistory(
        Box<hydrus_downloader_exchange::subscriptions::Subscription>,
        Vec<hydrus_downloader_exchange::subscriptions::Subscription>,
    ),
    FavouriteLoad(String),
    Delete,
    ClearImportOptions(Vec<u64>, String),
    Select,
    Check(CheckNow),
    /// A message with only "ok".
    Message(String),
    /// Information with only "ok".
    Information(String),
    Reset,
    Lowercase,
    RetryIgnored,
    Merge,
    /// A merge group's primary; the groups after it.
    MergePrimary(Vec<u64>, Vec<Vec<u64>>),
    /// A merged subscription's name; the groups after it.
    MergeName {
        group: Vec<u64>,
        primary: u64,
        rest: Vec<Vec<u64>>,
    },
    SeparateHow,
    /// "only extract some": the queries ticked.
    SeparatePick(Vec<bool>),
    /// Whether the queries ticked make one subscription.
    SeparateMerged(Vec<usize>),
    /// The name for a separation, how it separates.
    SeparateName(Separate),
    /// "deduplicate", its question, and the boxes ticked when it is a list
    /// of texts.
    Dedupe(Dedupe, Question, Vec<bool>),
}

/// What "overwrite downloader" says with no downloaders.
const NOTHING_TO_SELECT: &str = "Hey, you do not have any downloaders set up in this client, so there is nothing to select!\n\nCheck the _network->downloaders_ menu to find downloaders made by users.";

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
    services: Arc<hydrus_store::store::Snapshot>,
    dialog: Subscriptions,
    /// Each subscription as read, by id.
    read: HashMap<i64, AsRead>,
    short: ShortSummary,
    asking: Option<Asking>,
    /// The text the question's text box starts with, when it is asked.
    text: RefCell<Option<String>>,
    favourites: Option<Rc<crate::import_options_favourites_window::Controller>>,
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
                    copy_of: None,
                    exchange: None,
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
            services: store.snapshot(),
            dialog: Subscriptions::new(loaded),
            read,
            short: ShortSummary {
                new: naming.short_summary_new,
                deleted: naming.short_summary_deleted,
            },
            asking: None,
            text: RefCell::new(None),
            favourites: None,
        })
    })
}

/// A query as "apply" writes it: its queue if it has one, its state, and
/// the changes to its file log.
struct QueryWrite {
    queue: Option<i64>,
    state: QueryState,
    logs: Vec<LogChange>,
    /// A new query's file log, copied from this queue's.
    copy_of: Option<i64>,
    exchange: Option<hydrus_downloader_exchange::subscriptions::Query>,
}

/// A change "apply" writes.
enum Write {
    Delete(i64),
    /// A new subscription, with its queries (some perhaps another's
    /// until now).
    Create(String, Box<SubscriptionSettings>, Vec<QueryWrite>),
    Rename(i64, String),
    Settings(i64, Box<SubscriptionSettings>),
    Query(i64, QueryState),
    /// A new query, its file log copied from a queue's if any.
    AddQuery(i64, Box<QueryWrite>),
    /// A query moved to this subscription.
    Move(i64, i64),
    RemoveQuery(i64),
    /// A query's file log changed.
    Log(i64, LogChange),
}

/// What the dialog changed: deleted subscriptions, new ones, changed
/// names, settings and query states, queries moved between subscriptions
/// (merged or separated), and those emptied by it. In an order the
/// store's unique names allow: deletions, new subscriptions, queries,
/// those emptied, then names and settings.
fn changes(open: &Open) -> Vec<Write> {
    let owners: HashMap<i64, i64> = open
        .read
        .iter()
        .flat_map(|(&id, read)| {
            read.queries
                .iter()
                .filter_map(move |(q, _)| q.map(|q| (q, id)))
        })
        .collect();
    let mut writes: Vec<Write> = open
        .dialog
        .deleted
        .iter()
        .map(|&id| Write::Delete(id))
        .collect();
    let mut names = Vec::new();
    for s in &open.dialog.subscriptions {
        let Some(id) = s.id else {
            writes.push(Write::Create(
                s.name.clone(),
                Box::new(s.settings.clone()),
                s.queries
                    .iter()
                    .map(|q| QueryWrite {
                        queue: q.queue,
                        state: q.state.clone(),
                        logs: q.log_changes.clone(),
                        copy_of: q.copy_of,
                        exchange: q.exchange.clone(),
                    })
                    .collect(),
            ));
            continue;
        };
        writes.extend(s.deleted_queries.iter().map(|&q| Write::RemoveQuery(q)));
        let read = open.read.get(&id);
        if read.is_some_and(|r| s.name != r.name) {
            names.push(Write::Rename(id, s.name.clone()));
        }
        if read.is_none_or(|r| s.settings != r.settings) {
            names.push(Write::Settings(id, Box::new(s.settings.clone())));
        }
        for q in &s.queries {
            let Some(queue) = q.queue else {
                writes.push(Write::AddQuery(
                    id,
                    Box::new(QueryWrite {
                        queue: None,
                        state: q.state.clone(),
                        logs: q.log_changes.clone(),
                        copy_of: q.copy_of,
                        exchange: q.exchange.clone(),
                    }),
                ));
                continue;
            };
            if owners.get(&queue) != Some(&id) {
                writes.push(Write::Move(queue, id));
            }
            writes.extend(q.log_changes.iter().map(|&c| Write::Log(queue, c)));
            let before = owners
                .get(&queue)
                .and_then(|owner| open.read.get(owner))
                .and_then(|r| r.queries.iter().find(|(qq, _)| *qq == Some(queue)));
            if before.is_none_or(|(_, state)| state != &q.state) {
                writes.push(Write::Query(queue, q.state.clone()));
            }
        }
    }
    writes.extend(open.dialog.absorbed.iter().map(|&id| Write::Delete(id)));
    writes.extend(names);
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
            let ids = queues::file_seeds(conn, queue)?
                .into_iter()
                .filter(|seed| seed.status == SeedStatus::Error)
                .map(|seed| seed.id)
                .collect::<Vec<_>>();
            queues::set_file_seed_statuses(conn, &ids, SeedStatus::Unknown, now)?;
        }
        LogChange::RetryIgnored(which) => {
            let ids = queues::file_seeds(conn, queue)?
                .into_iter()
                .filter(|seed| seed.status == SeedStatus::Vetoed && which.matches(&seed.note))
                .map(|seed| seed.id)
                .collect::<Vec<_>>();
            queues::set_file_seed_statuses(conn, &ids, SeedStatus::Unknown, now)?;
        }
    }
    hydrus_gui_model::subscription_exchange::update_file_status(conn, queue, now)?;
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
                    let Some(id) = subscriptions::create_subscription(conn, name, settings)? else {
                        eprintln!("could not add the subscription {name:?}: the name is taken");
                        continue;
                    };
                    for q in queries {
                        let Some(queue) = q.queue else {
                            let queue = subscriptions::add_query(conn, id, &q.state, now)?;
                            if let Some(from) = q.copy_of {
                                queues::copy_seeds(conn, from, queue)?;
                                hydrus_gui_model::subscription_exchange::copy_header(
                                    conn, from, queue,
                                )?;
                            }
                            if q.copy_of.is_none() {
                                if let Some(query) = &q.exchange {
                                    hydrus_gui_model::subscription_exchange::restore(
                                        conn, queue, query,
                                    )?;
                                }
                            }
                            for &change in &q.logs {
                                change_log(conn, queue, change, now)?;
                            }
                            continue;
                        };
                        subscriptions::move_query(conn, queue, id)?;
                        subscriptions::set_query_state(conn, queue, &q.state)?;
                        for &change in &q.logs {
                            change_log(conn, queue, change, now)?;
                        }
                    }
                }
                Write::Move(queue, id) => subscriptions::move_query(conn, *queue, *id)?,
                Write::AddQuery(id, q) => {
                    let queue = subscriptions::add_query(conn, *id, &q.state, now)?;
                    if let Some(from) = q.copy_of {
                        queues::copy_seeds(conn, from, queue)?;
                        hydrus_gui_model::subscription_exchange::copy_header(conn, from, queue)?;
                    } else if let Some(query) = &q.exchange {
                        hydrus_gui_model::subscription_exchange::restore(conn, queue, query)?;
                    }
                    for &change in &q.logs {
                        change_log(conn, queue, change, now)?;
                    }
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

/// Ask "separate"'s new subscriptions' base name, starting from the
/// selected's name.
fn ask_separate_name(open: &mut Open, how: Separate) {
    let name = open
        .dialog
        .selection
        .one()
        .and_then(|k| open.dialog.get(k))
        .map(|s| s.name.clone());
    *open.text.borrow_mut() = name;
    open.asking = Some(Asking::SeparateName(how));
}

/// Merge a group into its primary, named `name` (or its own name), and
/// ask about the next group, if there is one.
fn merge_named(
    open: &mut Open,
    group: &[u64],
    primary: u64,
    mut rest: Vec<Vec<u64>>,
    name: Option<&str>,
) {
    let name = name
        .map(str::to_owned)
        .or_else(|| open.dialog.get(primary).map(|s| s.name.clone()))
        .unwrap_or_default();
    open.dialog.merge(primary, group, &name);
    if !rest.is_empty() {
        let next = rest.remove(0);
        open.asking = Some(Asking::MergePrimary(next, rest));
    }
}

/// Import in reference list order, pausing at each incomplete subscription.
fn import_next(
    open: &mut Open,
    mut incoming: Vec<hydrus_downloader_exchange::subscriptions::Subscription>,
) {
    while !incoming.is_empty() {
        let subscription = incoming.remove(0);
        if subscription.queries.iter().any(|q| q.log.is_none()) {
            open.asking = Some(Asking::MissingHistory(Box::new(subscription), incoming));
            return;
        }
        if let Err(error) =
            hydrus_gui_model::subscription_exchange::stage(&mut open.dialog, vec![subscription])
        {
            open.asking = Some(Asking::Message(error));
            return;
        }
    }
}

/// Show the dialog's list, buttons and question.
fn show(window: &SubscriptionsWindow, open: &Open) {
    let now = now();
    let dialog = &open.dialog;
    let rows: Vec<TableRow> = dialog
        .rows(now, open.short)
        .into_iter()
        .map(|(key, mut cells, selected)| {
            if let Some(subscription) = dialog.get(key) {
                cells[8] = crate::import_options_editor::container_summary(
                    &subscription.settings.import_options,
                    &|key| {
                        hex::decode(key)
                            .ok()
                            .and_then(|key| {
                                open.services
                                    .services
                                    .by_key(&hydrus_core::ServiceKey::new(key))
                                    .ok()
                            })
                            .map_or_else(|| "unknown service".into(), |s| s.name.clone())
                    },
                );
            }
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
    window.set_can_merge(dialog.can_merge(now));
    window.set_can_dedupe(crate::subscriptions_dedupe::can_dedupe(dialog, now));
    window.set_can_separate(dialog.can_separate(now));
    window.set_can_lowercase(dialog.can_lowercase(now));
    window.set_can_reset(dialog.can_reset(now));
    window.set_can_retry_failed(dialog.can_retry_failed(now));
    window.set_can_retry_ignored(dialog.can_retry_ignored(now));
    let yes_no = |message: &str| {
        Some((
            Choice {
                title: "Are you sure?".into(),
                message: message.into(),
                choices: vec!["yes".into(), "no".into()],
            },
            false,
        ))
    };
    let names = |keys: &[u64]| -> Vec<String> {
        keys.iter()
            .filter_map(|&k| dialog.get(k).map(|s| s.name.clone()))
            .collect()
    };
    let question = match &open.asking {
        Some(Asking::MissingHistory(subscription, _)) => Some((
            hydrus_gui_model::subscription_exchange::missing_history_question(&subscription.name),
            false,
        )),
        Some(Asking::Reset) => yes_no(RESET_QUESTION),
        Some(Asking::Lowercase) => yes_no(LOWERCASE_QUESTION),
        Some(Asking::Merge) => yes_no(MERGE_QUESTION),
        Some(Asking::RetryIgnored) => Some((
            Choice {
                title: RetryIgnored::TITLE.into(),
                message: String::new(),
                choices: RetryIgnored::CHOICES
                    .iter()
                    .map(|c| c.1.to_owned())
                    .collect(),
            },
            false,
        )),
        Some(Asking::MergePrimary(group, _)) => Some((
            Choice {
                title: MERGE_PRIMARY.into(),
                message: String::new(),
                choices: names(group),
            },
            false,
        )),
        Some(Asking::MergeName { group, primary, .. }) => Some((
            Choice {
                title: "Enter text".into(),
                message: format!(
                    "{} was able to merge {} other subscriptions. If you wish to change its name, do so here.",
                    dialog
                        .get(*primary)
                        .map(|s| s.name.as_str())
                        .unwrap_or_default(),
                    hydrus_core::numbers::human_int(group.len().saturating_sub(1) as u64)
                ),
                choices: vec!["ok".into()],
            },
            true,
        )),
        Some(Asking::SeparateHow) => Some((
            Choice {
                title: "Are you sure?".into(),
                message: SEPARATE_QUESTION.into(),
                choices: SEPARATE_CHOICES
                    .iter()
                    .chain(&["forget it"])
                    .map(|&c| c.to_owned())
                    .collect(),
            },
            false,
        )),
        Some(Asking::SeparatePick(_)) => Some((
            Choice {
                title: SEPARATE_PICK.into(),
                message: String::new(),
                choices: vec!["ok".into()],
            },
            false,
        )),
        Some(Asking::SeparateMerged(_)) => Some((
            Choice {
                title: "Are you sure?".into(),
                message: SEPARATE_MERGED_QUESTION.into(),
                choices: SEPARATE_MERGED_CHOICES
                    .iter()
                    .chain(&["forget it"])
                    .map(|&c| c.to_owned())
                    .collect(),
            },
            false,
        )),
        Some(Asking::SeparateName(how)) => Some((
            Choice {
                title: "Enter text".into(),
                message: if matches!(how, Separate::Part { merged: true, .. }) {
                    SEPARATE_MERGED_NAME
                } else {
                    SEPARATE_NAME
                }
                .into(),
                choices: vec!["ok".into()],
            },
            true,
        )),
        _ => None,
    };
    let question = question.or_else(|| match &open.asking {
        Some(Asking::ClearImportOptions(_, message)) => yes_no(message),
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
        Some(Asking::Message(message)) => Some((
            Choice {
                title: "Warning".into(),
                message: message.clone(),
                choices: vec!["ok".into()],
            },
            false,
        )),
        Some(Asking::Dedupe(_, question, _)) => Some((dedupe_choice(question), false)),
        Some(Asking::FavouriteLoad(_)) => Some((
            Choice {
                title: "Information".into(),
                message: MULTIPLE_FAVOURITE_LOAD.into(),
                choices: vec!["ok".into()],
            },
            false,
        )),
        Some(Asking::Information(message)) => Some((
            Choice {
                title: "Information".into(),
                message: message.clone(),
                choices: vec!["ok".into()],
            },
            false,
        )),
        _ => None,
    });
    let names_of_selected: Vec<String> = dialog
        .selection
        .one()
        .and_then(|k| dialog.get(k))
        .map(|s| {
            s.queries
                .iter()
                .map(|q| q.state.human_name().to_owned())
                .collect()
        })
        .unwrap_or_default();
    let ticks: Vec<Tick> = match &open.asking {
        Some(Asking::SeparatePick(ticked)) => names_of_selected
            .iter()
            .zip(ticked)
            .map(|(label, &on): (&String, &bool)| Tick {
                label: label.as_str().into(),
                on,
            })
            .collect(),
        Some(Asking::Dedupe(_, Question::Multiple { choices, .. }, ticked)) => choices
            .iter()
            .zip(ticked)
            .map(|(label, &on): (&String, &bool)| Tick {
                label: label.as_str().into(),
                on,
            })
            .collect(),
        _ => Vec::new(),
    };
    window.set_asking_ticks(ModelRc::new(VecModel::from(ticks)));
    if let Some(text) = open.text.borrow_mut().take() {
        window.set_asked_text(text.into());
    }
    window.set_asking(question.is_some());
    if let Some((choice, wants_text)) = question {
        window.set_asking_title(choice.title.into());
        window.set_asking_message(choice.message.into());
        let choices: Vec<SharedString> = choice.choices.into_iter().map(Into::into).collect();
        window.set_asking_choices(ModelRc::new(VecModel::from(choices)));
        window.set_asking_text(wants_text);
    }
}

/// A "deduplicate" question as the window's panel asks it.
fn dedupe_choice(question: &Question) -> Choice {
    let choice = |title: &str, message: &str, choices: &[&str]| Choice {
        title: title.into(),
        message: message.into(),
        choices: choices.iter().map(|&c| c.to_owned()).collect(),
    };
    match question {
        Question::Choice(choice) => choice.clone(),
        Question::YesNo(message) => choice("Are you sure?", message, &["yes", "no"]),
        Question::YesYesNo { message, yeses, no } => Choice {
            title: "Are you sure?".into(),
            message: message.clone(),
            choices: yeses.iter().chain([no]).cloned().collect(),
        },
        Question::Multiple { title, .. } => choice(title, "", &["ok"]),
        Question::Warning(message) => choice("Warning", message, &["ok"]),
    }
}

/// A "deduplicate" question answered with the button at `index`.
fn dedupe_answer(question: &Question, ticked: &[bool], index: usize) -> Answer {
    match question {
        Question::Choice(_) => Answer::Index(index),
        Question::YesNo(_) => Answer::Yes(index == 0),
        Question::YesYesNo { yeses, .. } if index < yeses.len() => Answer::Index(index),
        Question::Multiple { choices, .. } => Answer::Texts(
            choices
                .iter()
                .zip(ticked)
                .filter(|(_, on): &(&String, &bool)| **on)
                .map(|(c, _): (&String, &bool)| c.clone())
                .collect(),
        ),
        _ => Answer::Cancel,
    }
}

/// Ask `question` next, if there is one.
fn ask_dedupe(open: &mut Open, dedupe: Dedupe, question: Option<Question>) {
    if let Some(question) = question {
        let ticked = match &question {
            Question::Multiple { choices, .. } => vec![true; choices.len()],
            _ => Vec::new(),
        };
        open.asking = Some(Asking::Dedupe(dedupe, question, ticked));
    }
}

/// Ask for a gallery in the reference's separate list dialog, then continue.
fn select_gallery(
    store: &Store,
    slot: &Rc<RefCell<Option<SubscriptionGalleryWindow>>>,
    parent: &slint::Weak<SubscriptionsWindow>,
    current: Option<(String, String)>,
    for_add: bool,
    done: Rc<dyn Fn(String, String)>,
) {
    if slot.borrow().is_some() {
        return;
    }
    let gugs = store
        .read(hydrus_store::settings::get::<hydrus_parse::Downloaders>)
        .map(|d| crate::gallery::offered_gugs(&d.gugs))
        .unwrap_or_default();
    let Ok(window) = SubscriptionGalleryWindow::new() else {
        return;
    };
    if gugs.is_empty() {
        window.set_window_title("Warning".into());
        window.set_message(
            if for_add {
                NO_DOWNLOADERS
            } else {
                NOTHING_TO_SELECT
            }
            .into(),
        );
    } else {
        let selected = current
            .and_then(|(key, name)| {
                gugs.iter()
                    .position(|g| g.0 == key)
                    .or_else(|| gugs.iter().position(|g| g.1 == name))
            })
            .unwrap_or(0);
        window.set_selected(i32::try_from(selected).unwrap_or(0));
        window.set_galleries(ModelRc::new(VecModel::from(
            gugs.iter()
                .map(|g| SharedString::from(g.1.as_str()))
                .collect::<Vec<_>>(),
        )));
    }
    let active = Rc::new(Cell::new(true));
    let close: Rc<dyn Fn()> = {
        let slot = Rc::downgrade(slot);
        let parent = parent.clone();
        let active = active.clone();
        Rc::new(move || {
            if !active.replace(false) {
                return;
            }
            if let Some(slot) = slot.upgrade()
                && let Some(window) = slot.borrow_mut().take()
            {
                let _ = window.hide();
            }
            if let Some(parent) = parent.upgrade() {
                parent.set_gallery_open(false);
            }
        })
    };
    window.on_accept_clicked({
        let weak = window.as_weak();
        let close = close.clone();
        move || {
            if !active.get() {
                return;
            }
            let selected = weak
                .upgrade()
                .and_then(|w| usize::try_from(w.get_selected()).ok());
            let picked = selected.and_then(|i| gugs.get(i).cloned());
            close();
            if let Some((key, name, _)) = picked {
                done(key, name);
            }
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
    if window.show().is_ok() {
        *slot.borrow_mut() = Some(window);
        if let Some(parent) = parent.upgrade() {
            parent.set_gallery_open(true);
        }
    }
}

/// Open the dialog on the store's subscriptions; it forgets itself from
/// `slot` when closed.
pub(crate) fn open(
    store: &Arc<Store>,
    slot: &Rc<RefCell<Option<SubscriptionsWindow>>>,
    gallery_slot: &Rc<RefCell<Option<SubscriptionGalleryWindow>>>,
    edit_slots: Slots,
) -> Result<SubscriptionsWindow, String> {
    let edit_slots = Rc::new(edit_slots);
    let exchange = edit_slots.exchange.clone();
    let state = Rc::new(RefCell::new(read(store).map_err(|e| e.to_string())?));
    let active = Rc::new(Cell::new(true));
    let window = SubscriptionsWindow::new().map_err(|e| e.to_string())?;
    let paused = store
        .read(hydrus_store::settings::get::<hydrus_store::settings::Pauses>)
        .is_ok_and(|p| p.subscriptions);
    window.set_globally_paused(paused);
    let close = {
        let active = active.clone();
        let weak = window.as_weak();
        let slot = slot.clone();
        let edit = edit_slots.edit.clone();
        let gallery = gallery_slot.clone();
        let state = state.clone();
        let exchange = exchange.clone();
        move || {
            if !active.replace(false) {
                return;
            }
            exchange.cancel();
            let favourites = state.borrow().favourites.clone();
            if let Some(favourites) = favourites {
                favourites.close();
            }
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            slot.borrow_mut().take();
            let chooser = gallery
                .borrow()
                .as_ref()
                .map(slint::ComponentHandle::clone_strong);
            if let Some(window) = chooser {
                window.invoke_cancel();
            }
            // (the edit dialog goes with it)
            if let Some(window) = edit.borrow_mut().take() {
                let _ = window.hide();
            }
        }
    };
    // (each change shown again)
    let change = {
        let active = active.clone();
        let weak = window.as_weak();
        let state = state.clone();
        let exchange = exchange.clone();
        move |f: &dyn Fn(&mut Open)| {
            if !active.get()
                || exchange.has_open()
                || state
                    .borrow()
                    .favourites
                    .as_ref()
                    .is_some_and(|owner| owner.busy())
            {
                return;
            }
            f(&mut state.borrow_mut());
            if let Some(window) = weak.upgrade() {
                show(&window, &state.borrow());
            }
        }
    };
    let change: Change = Rc::new(change);
    let targets: Rc<RefCell<Vec<u64>>> = Rc::default();
    let favourites = crate::import_options_favourites_window::Controller::new(
        store.clone(),
        hydrus_core::import_options::CallerType::SpecificImporter,
        Rc::new({
            let state = Rc::downgrade(&state);
            let targets = targets.clone();
            let active = active.clone();
            move || {
                if !active.get() {
                    return None;
                }
                let state = state.upgrade()?;
                let open = state.borrow();
                targets
                    .borrow()
                    .first()
                    .and_then(|key| open.dialog.get(*key))
                    .map(|s| s.settings.import_options.clone())
            }
        }),
        Rc::new({
            let state = Rc::downgrade(&state);
            let targets = targets.clone();
            let weak = window.as_weak();
            let active = active.clone();
            move |options| {
                if !active.get() {
                    return;
                }
                let Some(state) = state.upgrade() else {
                    return;
                };
                state.borrow_mut().dialog.paste_import_options(
                    &targets.borrow(),
                    ImportOptionsPaste::Replace,
                    &options,
                );
                if let Some(window) = weak.upgrade() {
                    show(&window, &state.borrow());
                }
            }
        }),
        Rc::new({
            let state = Rc::downgrade(&state);
            let weak = window.as_weak();
            move |error| {
                if let Some(state) = state.upgrade() {
                    state.borrow_mut().asking = Some(Asking::Message(error));
                    if let Some(window) = weak.upgrade() {
                        show(&window, &state.borrow());
                    }
                }
            }
        }),
        Rc::new({
            let weak = window.as_weak();
            move |busy| {
                if let Some(window) = weak.upgrade() {
                    window.set_import_child_open(busy);
                }
            }
        }),
    );
    favourites.set_save_current_allowed(false);
    state.borrow_mut().favourites = Some(favourites.clone());
    let refresh_favourites = Rc::new({
        let favourites = favourites.clone();
        let weak = window.as_weak();
        move || {
            if let (Ok(rows), Some(window)) = (favourites.rows(), weak.upgrade()) {
                window.set_favourites(ModelRc::new(VecModel::from(rows)));
            }
        }
    });
    window.on_refresh_favourites({
        let refresh = refresh_favourites.clone();
        move || refresh()
    });
    window.on_favourite({ let state = state.clone(); let targets = targets.clone(); let favourites = favourites.clone(); let change = change.clone(); let active = active.clone(); let refresh = refresh_favourites.clone(); let exchange = exchange.clone(); move |action, name| {
        if !active.get() || exchange.has_open() || favourites.busy() || state.borrow().asking.is_some() { return; }
        if action == 0 || action == 1 {
            let keys = state.borrow().dialog.selected(now());
            if keys.is_empty() { change(&|open| open.asking = Some(Asking::Information("Hey, nothing is selected in the subscriptions list--select something and try loading again.".into()))); return; }
            let multiple = keys.len() > 1;
            *targets.borrow_mut() = keys;
            if action == 1 && multiple { change(&|open| open.asking = Some(Asking::FavouriteLoad(name.to_string()))); return; }
        }
        favourites.choose(action, name.as_str()); refresh();
    } });
    refresh_favourites();
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
        let active = active.clone();
        move |key: Option<u64>, settings: SubscriptionSettings| {
            if !active.get() || slots.exchange.has_open() || slots.edit.borrow().is_some() {
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
        let store = store.clone();
        let slot = gallery_slot.clone();
        let weak = window.as_weak();
        let edit = edit.clone();
        let exchange = exchange.clone();
        let active = active.clone();
        move || {
            if !active.get() || exchange.has_open() {
                return;
            }
            let current = store
                .read(hydrus_store::settings::get::<hydrus_core::subscriptions::GalleryDefaults>)
                .ok()
                .and_then(|d| d.gug);
            let store_for_edit = store.clone();
            let edit = edit.clone();
            let done = Rc::new(move |gug_key: String, gug_name: String| {
                let checker = store_for_edit
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
            });
            select_gallery(&store, &slot, &weak, current, true, done);
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
        let state = state.clone();
        let favourites = favourites.clone();
        let change = change.clone();
        let weak = window.as_weak();
        move |index| {
            let Ok(index) = usize::try_from(index) else {
                return;
            };
            let favourite = match state.borrow().asking.as_ref() {
                Some(Asking::FavouriteLoad(name)) => Some(name.clone()),
                _ => None,
            };
            if let Some(name) = favourite {
                change(&|open| open.asking = None);
                if index == 0 {
                    favourites.choose(1, &name);
                }
                return;
            }
            let text = weak
                .upgrade()
                .map(|w| w.get_asked_text().to_string())
                .unwrap_or_default();
            change(&|open| match open.asking.take() {
                Some(Asking::MissingHistory(subscription, rest)) => {
                    if index == 0 {
                        if let Err(error) = hydrus_gui_model::subscription_exchange::stage(
                            &mut open.dialog,
                            vec![*subscription],
                        ) {
                            open.asking = Some(Asking::Message(error));
                            return;
                        }
                    }
                    import_next(open, rest);
                }
                Some(Asking::ClearImportOptions(keys, _)) => {
                    if index == 0 {
                        open.dialog.clear_import_options(&keys);
                    }
                }
                Some(Asking::Reset) => {
                    if index == 0 {
                        open.dialog.reset_selected(now());
                    }
                }
                Some(Asking::Lowercase) => {
                    if index == 0 {
                        open.dialog.lowercase_selected(now());
                    }
                }
                Some(Asking::RetryIgnored) => {
                    if let Some((which, _)) = RetryIgnored::CHOICES.get(index) {
                        open.dialog.retry_ignored_selected(now(), *which);
                    }
                }
                Some(Asking::Merge) => {
                    if index == 0 {
                        let mut groups = open.dialog.merge_groups(now());
                        if groups.is_empty() {
                            open.asking = Some(Asking::Message(NOT_MERGEABLE.into()));
                        } else {
                            let group = groups.remove(0);
                            open.asking = Some(Asking::MergePrimary(group, groups));
                        }
                    }
                }
                Some(Asking::MergePrimary(group, rest)) => {
                    if let Some(&primary) = group.get(index) {
                        let name = open.dialog.get(primary).map(|s| s.name.clone());
                        *open.text.borrow_mut() = name;
                        open.asking = Some(Asking::MergeName {
                            group,
                            primary,
                            rest,
                        });
                    }
                }
                Some(Asking::MergeName {
                    group,
                    primary,
                    rest,
                }) => merge_named(open, &group, primary, rest, Some(&text)),
                Some(Asking::SeparateHow) => match index {
                    0 => {
                        open.dialog.separate(now(), &Separate::Half, "");
                    }
                    1 => ask_separate_name(open, Separate::Whole),
                    2 => {
                        let n = open
                            .dialog
                            .selection
                            .one()
                            .and_then(|k| open.dialog.get(k))
                            .map_or(0, |s| s.queries.len());
                        open.asking = Some(Asking::SeparatePick(vec![false; n]));
                    }
                    _ => {}
                },
                Some(Asking::SeparatePick(ticked)) => {
                    let queries: Vec<usize> = (0..ticked.len()).filter(|&i| ticked[i]).collect();
                    match picked(queries.len(), ticked.len()) {
                        Picked::All => ask_separate_name(open, Separate::Whole),
                        Picked::Several => open.asking = Some(Asking::SeparateMerged(queries)),
                        Picked::Few => ask_separate_name(
                            open,
                            Separate::Part {
                                queries,
                                merged: false,
                            },
                        ),
                    }
                }
                Some(Asking::SeparateMerged(queries)) => {
                    if index < SEPARATE_MERGED_CHOICES.len() {
                        ask_separate_name(
                            open,
                            Separate::Part {
                                queries,
                                merged: index == 0,
                            },
                        );
                    }
                }
                Some(Asking::SeparateName(how)) => {
                    open.dialog.separate(now(), &how, &text);
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
                Some(Asking::Dedupe(mut dedupe, question, ticked)) => {
                    let answer = dedupe_answer(&question, &ticked, index);
                    let next = dedupe.answer(&mut open.dialog, &answer);
                    ask_dedupe(open, dedupe, next);
                }
                Some(Asking::Message(_) | Asking::Information(_) | Asking::FavouriteLoad(_))
                | None => {}
            });
        }
    });
    window.on_cancelled({
        let change = change.clone();
        move || {
            change(&|open| {
                match open.asking.take() {
                    // A cancelled rename keeps the primary's original name.
                    Some(Asking::MergeName {
                        group,
                        primary,
                        rest,
                    }) => {
                        merge_named(open, &group, primary, rest, None);
                    }
                    Some(Asking::MissingHistory(_, rest)) => {
                        import_next(open, rest);
                    }
                    _ => (),
                }
            });
        }
    });
    window.on_reset({
        let change = change.clone();
        move || change(&|open| open.asking = Some(Asking::Reset))
    });
    window.on_lowercase({
        let change = change.clone();
        move || change(&|open| open.asking = Some(Asking::Lowercase))
    });
    window.on_retry_ignored({
        let change = change.clone();
        move || change(&|open| open.asking = Some(Asking::RetryIgnored))
    });
    window.on_retry_failed({
        let change = change.clone();
        move || change(&|open| open.dialog.retry_failed_selected(now()))
    });
    window.on_duplicate({
        let change = change.clone();
        move || {
            change(&|open| {
                let made = open.dialog.duplicate_selected(now());
                if !made.is_empty() {
                    open.asking = Some(Asking::Information(added_message(made.len())));
                }
            });
        }
    });
    window.on_deduplicate({
        let change = change.clone();
        move || {
            change(&|open| {
                let (dedupe, question) = Dedupe::start(&open.dialog, now());
                ask_dedupe(open, dedupe, Some(question));
            });
        }
    });
    window.on_ticked({
        let change = change.clone();
        move |index, on| {
            change(&|open| {
                let Some(Asking::Dedupe(_, _, ticked) | Asking::SeparatePick(ticked)) =
                    &mut open.asking
                else {
                    return;
                };
                if let Some(tick) = usize::try_from(index).ok().and_then(|i| ticked.get_mut(i)) {
                    *tick = on;
                }
            });
        }
    });
    window.on_merge({
        let change = change.clone();
        move || change(&|open| open.asking = Some(Asking::Merge))
    });
    window.on_separate({
        let change = change.clone();
        move || {
            change(&|open| {
                let queries = open
                    .dialog
                    .selection
                    .one()
                    .and_then(|k| open.dialog.get(k))
                    .map_or(0, |s| s.queries.len());
                if queries > 2 {
                    open.asking = Some(Asking::SeparateHow);
                } else if queries == 2 {
                    ask_separate_name(open, Separate::Whole);
                }
            });
        }
    });
    window.on_overwrite_downloader({
        let change = change.clone();
        let state = state.clone();
        let store = store.clone();
        let slot = gallery_slot.clone();
        let weak = window.as_weak();
        let exchange = exchange.clone();
        let active = active.clone();
        move || {
            if !active.get() || exchange.has_open() {
                return;
            }
            let current = {
                let open = state.borrow();
                open.dialog
                    .selection
                    .in_order(&open.dialog.order(now()))
                    .first()
                    .and_then(|key| open.dialog.get(*key))
                    .map(|s| (s.settings.gug_key.clone(), s.settings.gug_name.clone()))
            };
            let change = change.clone();
            let done = Rc::new(move |key: String, name: String| {
                change(&|open| open.dialog.set_downloader_selected(now(), &key, &name));
            });
            select_gallery(&store, &slot, &weak, current, false, done);
        }
    });
    window.on_overwrite_checker({
        let change = change.clone();
        let state = state.clone();
        let store = store.clone();
        let slot = edit_slots.checker.clone();
        let exchange = exchange.clone();
        let active = active.clone();
        move || {
            if !active.get() || exchange.has_open() {
                return;
            }
            if slot.borrow().is_some() {
                return;
            }
            let Some(current) = state.borrow().dialog.first_selected_checker(now()) else {
                return;
            };
            let applied: Rc<dyn Fn(hydrus_core::subscriptions::CheckerOptions)> = {
                let change = change.clone();
                Rc::new(move |checker| {
                    change(&|open| open.dialog.set_checker_selected(now(), &checker));
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
    window.on_copy_import_options({
        let weak = window.as_weak();
        let state = state.clone();
        let change = change.clone();
        let active = active.clone();
        let exchange = exchange.clone();
        move || {
            if !active.get() || exchange.has_open() {
                return;
            }
            if !active.get() {
                return;
            }
            let options = {
                let open = state.borrow();
                open.dialog
                    .selection
                    .one()
                    .and_then(|key| open.dialog.get(key))
                    .map(|s| s.settings.import_options.clone())
            };
            let Some(options) = options else {
                return;
            };
            match hydrus_downloader_exchange::import_options::encode_text(&options) {
                Ok(text) => {
                    crate::to_clipboard(&crate::Clip::Text(text));
                    if let Some(window) = weak.upgrade() {
                        window.set_import_status("Copied!".into());
                    }
                }
                Err(error) => {
                    change(&|open| open.asking = Some(Asking::Message(error.to_string())));
                }
            }
        }
    });
    window.on_paste_import_options({
        let weak = window.as_weak();
        let state = state.clone();
        let change = change.clone();
        let active = active.clone();
        let exchange = exchange.clone();
        move |index| {
            if !active.get() || exchange.has_open() { return; }
            if !active.get() { return; }
            let Some(mode) = usize::try_from(index).ok().and_then(ImportOptionsPaste::from_menu_index) else { return; };
            let keys = state.borrow().dialog.selected(now());
            if keys.is_empty() { return; }
            let decoded = crate::from_clipboard().and_then(|text|
                hydrus_downloader_exchange::import_options::decode_text(&text).map_err(|e|
                    format!("Could not understand the clipboard as JSON-serialised Import Options Container.\n\n{e}")));
            match decoded {
                Ok(options) => {
                    change(&|open| open.dialog.paste_import_options(&keys, mode, &options));
                    if let Some(window) = weak.upgrade() { window.set_import_status("Pasted!".into()); }
                }
                Err(error) => change(&|open| open.asking = Some(Asking::Message(error.clone()))),
            }
        }
    });
    window.on_clear_import_options({
        let change = change.clone();
        move || {
            change(&|open| {
                if let Some(question) = open.dialog.clear_import_options_question(now()) {
                    open.asking = Some(Asking::ClearImportOptions(
                        open.dialog.selected(now()),
                        question,
                    ));
                }
            });
        }
    });
    window.on_exchange({
        let active = active.clone();
        let state = state.clone();
        let store = store.clone();
        let weak = window.as_weak();
        let slots = exchange.clone();
        move |importing| {
            if !active.get() || slots.has_open() || state.borrow().asking.is_some()
                || state.borrow().favourites.as_ref().is_some_and(|owner| owner.busy())
            {
                return;
            }
            let definitions = if importing {
                Ok(Vec::new())
            } else {
                hydrus_gui_model::subscription_exchange::selected(&store, &state.borrow().dialog, now())
            };
            let result = definitions.and_then(|definitions| {
                let preview = Rc::new(|incoming: Vec<hydrus_downloader_exchange::subscriptions::Subscription>| {
                    hydrus_gui_model::subscription_exchange::validate(&incoming)?;
                    let missing = incoming.iter()
                        .filter(|s| s.queries.iter().any(|q| q.log.is_none()))
                        .map(|s| s.name.as_str()).collect::<Vec<_>>();
                    let warning = if missing.is_empty() {
                        String::new()
                    } else {
                        format!("\nMissing query histories in {} will be reinitialised empty. Cancel to back out.", missing.join(", "))
                    };
                    let names = incoming.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join("\n");
                    Ok(format!("Import {} complete subscriptions:\n{names}{warning}\nChanges are saved only when you apply manage subscriptions.", incoming.len()))
                });
                let applied = Rc::new({
                    let active = active.clone();
                    let state = state.clone();
                    move |incoming| {
                        if !active.get() {
                            return Err("The subscription list was closed.".into());
                        }
                        import_next(&mut state.borrow_mut(), incoming);
                        Ok(())
                    }
                });
                crate::downloader_interchange_window::open_subscriptions(&store, &slots, importing, definitions, preview, applied)
            });
            if let Some(parent) = weak.upgrade() {
                match result {
                    Ok(child) => {
                        parent.set_exchange_open(true);
                        child.on_closed({
                            let weak = weak.clone();
                            let state = state.clone();
                            let active = active.clone();
                            move || {
                                if active.get() && let Some(parent) = weak.upgrade() {
                                    parent.set_exchange_open(false);
                                    show(&parent, &state.borrow());
                                }
                            }
                        });
                    }
                    Err(error) => { parent.set_import_status(error.into()); }
                }
            }
        }
    });
    window.on_exchange_mode({
        let active = active.clone();
        let weak = window.as_weak();
        let slots = exchange.clone();
        move |mode| {
            if !active.get() || slots.has_open() || !(0..=5).contains(&mode) {
                return;
            }
            let Some(parent) = weak.upgrade() else {
                return;
            };
            parent.invoke_exchange(mode >= 3);
            let child = slots
                .0
                .borrow()
                .as_ref()
                .map(slint::ComponentHandle::clone_strong);
            let Some(child) = child else {
                return;
            };
            match mode {
                0 => {
                    child.invoke_action("copy".into());
                    if child.get_error().is_empty() {
                        child.invoke_action("cancel".into());
                    }
                }
                1 => {
                    child.invoke_action("browse-json".into());
                    if child.get_path().is_empty() {
                        child.invoke_action("cancel".into());
                    } else {
                        child.invoke_action("save-json".into());
                    }
                }
                2 => {
                    child.invoke_export_png();
                }
                3 => {
                    child.invoke_action("paste".into());
                    if child.get_error().is_empty() {
                        child.invoke_action("review".into());
                    }
                }
                _ => {
                    child.invoke_action(
                        if mode == 4 {
                            "import-jsons"
                        } else {
                            "import-pngs"
                        }
                        .into(),
                    );
                    if !child.get_ready() && child.get_error().is_empty() {
                        child.invoke_action("cancel".into());
                    }
                }
            }
        }
    });
    window.on_apply({
        let exchange = exchange.clone();
        let active = active.clone();
        let state = state.clone();
        let store = store.clone();
        let close = close.clone();
        move || {
            if !active.get()
                || state
                    .borrow()
                    .favourites
                    .as_ref()
                    .is_some_and(|owner| owner.busy())
            {
                return;
            }
            if exchange.has_open() || state.borrow().asking.is_some() {
                return;
            }
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
