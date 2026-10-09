//! Subscription list exchange drafts, history persistence and live exports.
use crate::subscriptions_dialog::{DialogQuery, Subscriptions};
use hydrus_downloader_exchange::subscriptions::{self as exchange, Query, Subscription};
use hydrus_store::{Store, queues, settings};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Reference caches and example seeds retained by native queue identity.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Headers(pub BTreeMap<i64, Value>);
impl settings::Setting for Headers {
    const KEY: &'static str = "subscription_reference_headers";
}

/// The actual missing-history import confirmation; Cancel leaves that object out.
pub fn missing_history_question(name: &str) -> crate::subscriptions_dialog::Choice {
    crate::subscriptions_dialog::Choice {
        title: "missing query log data!".into(),
        message: format!(
            "When importing this subscription, \"{name}\", there was missing log data! I will still let you add it, but some of its queries are incomplete. If you are ok with this, ok and then immediately re-open the manage subscriptions dialog to reinitialise the missing data back to zero (and clear any orphaned data that came with this). If you are not ok with this, cancel out now or cancel out of the whole manage subs dialog."
        ),
        choices: vec!["import it anyway".into(), "back out now".into()],
    }
}

/// The question the next opening of manage subscriptions asks about queries
/// whose imported history was missing (`ClientGUI._ManageSubscriptions`).
pub fn missing_logs_question(count: usize) -> crate::subscriptions_dialog::Choice {
    let count = hydrus_core::numbers::human_int(count as u64);
    crate::subscriptions_dialog::Choice {
        title: "Missing Query Logs!".into(),
        message: format!(
            "{count} subscription queries had missing database data! This is a serious error!\n\nIf you continue, the client will now create and save empty file/search logs for those queries, essentially resetting them, but if you know you need to exit and fix your database in a different way, cancel out now.\n\nIf you do not know why this happened, you may have had a hard drive fault. Please check the 'Recovery->Help my db is broke' document in the help, and you may want to contact hydrus dev."
        ),
        choices: vec!["continue".into(), "back out".into()],
    }
}

/// "continue": give the missing queries empty logs and reset their headers
/// (`SubscriptionQueryHeader.Reset`: never checked, alive, unpaused, no files),
/// then forget that they were missing.
pub fn reinitialise_missing_logs(
    conn: &rusqlite::Connection,
    queues: &[i64],
    now: i64,
) -> hydrus_store::Result<()> {
    for &queue in queues {
        if let Some(saved) = hydrus_store::subscriptions::query(conn, queue)? {
            let mut state = saved.state;
            state.last_check_time = 0;
            state.next_check_time = 0;
            state.dead = false;
            state.paused = false;
            hydrus_store::subscriptions::set_query_state(conn, queue, &state)?;
            queues::remove_file_seeds(
                conn,
                queue,
                &(0..=10)
                    .filter_map(queues::SeedStatus::from_code)
                    .collect::<Vec<_>>(),
            )?;
            update_file_status(conn, queue, now)?;
        }
        hydrus_store::subscriptions::set_missing_log(conn, queue, false)?;
    }
    Ok(())
}

/// Validate that the entire imported history can run in native queues.
pub fn validate(subscriptions: &[Subscription]) -> Result<(), String> {
    for subscription in subscriptions {
        for query in &subscription.queries {
            if let Some(log) = &query.log {
                for seed in &log.file_seeds {
                    if !matches!(seed.seed_type, 0 | 1)
                        || queues::SeedStatus::from_code(seed.status).is_none()
                    {
                        return Err(format!(
                            "Unsupported file seed type/status in {}.",
                            subscription.name
                        ));
                    }
                }
                if log
                    .gallery_seeds
                    .iter()
                    .any(|s| queues::SeedStatus::from_code(s.status).is_none())
                {
                    return Err(format!(
                        "Unsupported gallery seed status in {}.",
                        subscription.name
                    ));
                }
            }
        }
    }
    Ok(())
}
/// Stage a complete import with fresh history names and casefold name collisions.
/// Missing histories are reinitialised only after the caller confirms them.
pub fn stage(dialog: &mut Subscriptions, incoming: Vec<Subscription>) -> Result<(), String> {
    validate(&incoming)?;
    for subscription in incoming {
        let queries = subscription
            .queries
            .into_iter()
            .map(|mut query| {
                exchange::rename_history(
                    &mut query,
                    hydrus_core::pages::PageKey::random().to_hex(),
                );
                let mut draft = DialogQuery::new(query.state.clone());
                if let Some(log) = &query.log {
                    for seed in &log.file_seeds {
                        let status = queues::SeedStatus::from_code(seed.status)
                            .expect("validated history status");
                        *draft.files.entry(status).or_default() += 1;
                        draft.seed_times.push(hydrus_core::subscriptions::SeedTime {
                            source_time: seed.source_time,
                            created: seed.created,
                        });
                        if status == queues::SeedStatus::Vetoed {
                            draft.ignored_notes.push(seed.note.clone());
                        }
                    }
                }
                draft.exchange = Some(query);
                draft
            })
            .collect();
        dialog.add_edited(&subscription.name, subscription.settings, queries);
    }
    Ok(())
}
/// Restore an imported draft and its reference header inside the owner's write.
pub fn restore(conn: &rusqlite::Connection, queue: i64, query: &Query) -> hydrus_store::Result<()> {
    if let Some(log) = &query.log {
        hydrus_store::import::restore_subscription_log(conn, queue, log)?;
    } else {
        // (accepted without its history: the reference finds it missing)
        hydrus_store::subscriptions::set_missing_log(conn, queue, true)?;
    }
    let header = exchange::query_header_tuple(query)
        .map_err(|e| hydrus_store::StoreError::Invalid(e.to_string()))?;
    let mut headers: Headers = settings::get(conn)?;
    headers.0.insert(queue, header);
    settings::set(conn, &headers)
}
/// Keep the retained export cache consistent with a committed reset/retry.
pub fn update_file_status(
    conn: &rusqlite::Connection,
    queue: i64,
    now: i64,
) -> hydrus_store::Result<()> {
    let mut headers: Headers = settings::get(conn)?;
    let Some(header) = headers.0.get(&queue).cloned() else {
        return Ok(());
    };
    let Some(saved) = hydrus_store::subscriptions::query(conn, queue)? else {
        return Ok(());
    };
    let name = header[2][0].as_str().unwrap_or_default().to_owned();
    let log = history(conn, queue, &name).map_err(hydrus_store::StoreError::Invalid)?;
    let mut query = Query {
        state: saved.state,
        log: Some(log),
        log_name: name,
        reference_header: Some(header),
    };
    exchange::update_file_status(&mut query, now)
        .map_err(|e| hydrus_store::StoreError::Invalid(e.to_string()))?;
    headers
        .0
        .insert(queue, query.reference_header.expect("refreshed cache"));
    settings::set(conn, &headers)
}

/// Carry a checker options edit into a saved query's cached header.
pub fn apply_checker_edit(
    conn: &rusqlite::Connection,
    queue: i64,
    edit: &exchange::CheckerEdit,
    now: i64,
) -> hydrus_store::Result<()> {
    let Some(saved) = hydrus_store::subscriptions::query(conn, queue)? else {
        return Ok(());
    };
    let mut headers: Headers = settings::get(conn)?;
    let header = headers.0.get(&queue).cloned();
    let name = header.as_ref().and_then(|h| h[2][0].as_str()).map_or_else(
        || hydrus_core::pages::PageKey::random().to_hex(),
        str::to_owned,
    );
    let log = history(conn, queue, &name).map_err(hydrus_store::StoreError::Invalid)?;
    let mut query = Query {
        state: saved.state,
        log: Some(log),
        log_name: name,
        reference_header: header,
    };
    exchange::apply_checker_edit(&mut query, edit, now)
        .map_err(|e| hydrus_store::StoreError::Invalid(e.to_string()))?;
    headers
        .0
        .insert(queue, query.reference_header.expect("edited header"));
    settings::set(conn, &headers)
}

/// Copy cached header metadata while giving the duplicated history a fresh name.
pub fn copy_header(conn: &rusqlite::Connection, from: i64, to: i64) -> hydrus_store::Result<()> {
    let mut headers: Headers = settings::get(conn)?;
    if let Some(mut header) = headers.0.get(&from).cloned() {
        header[2][0] = serde_json::json!(hydrus_core::pages::PageKey::random().to_hex());
        header[2][8] = serde_json::json!(1);
        header[2][13] = serde_json::json!([0, 1]);
        header[2][14] = serde_json::json!("unknown");
        headers.0.insert(to, header);
        settings::set(conn, &headers)?;
    }
    Ok(())
}
/// Read both current native histories without stale dialog snapshots.
pub fn history(
    conn: &rusqlite::Connection,
    queue: i64,
    name: &str,
) -> Result<hydrus_downloader_exchange::subscriptions::QueryLog, String> {
    let files = queues::file_seeds(conn, queue).map_err(|e| e.to_string())?;
    let galleries = queues::gallery_seeds(conn, queue).map_err(|e| e.to_string())?;
    let files: Value = serde_json::from_str(&crate::file_log::export_objects(
        &files.iter().collect::<Vec<_>>(),
    )?)
    .map_err(|e| e.to_string())?;
    let galleries: Value = serde_json::from_str(&crate::search_log::export_objects(
        &galleries.iter().collect::<Vec<_>>(),
    )?)
    .map_err(|e| e.to_string())?;
    exchange::decode_log(&serde_json::json!([
        86,
        name,
        1,
        [[67, 1, galleries], [8, 8, files]]
    ]))
    .map_err(|e| e.to_string())
}
/// Export selected drafts, applying staged reset/retry commands to the copy.
pub fn selected(
    store: &Store,
    dialog: &Subscriptions,
    now: i64,
) -> Result<Vec<Subscription>, String> {
    let mut out = Vec::new();
    for key in dialog.selected(now) {
        let Some(subscription) = dialog.get(key) else {
            continue;
        };
        let mut queries = Vec::new();
        for draft in &subscription.queries {
            let mut query = if let Some(query) = &draft.exchange {
                query.clone()
            } else {
                Query {
                    state: draft.state.clone(),
                    log: None,
                    log_name: hydrus_core::pages::PageKey::random().to_hex(),
                    reference_header: None,
                }
            };
            query.state.clone_from(&draft.state);
            if let Some(queue) = draft.queue.or(draft.copy_of) {
                query.log = Some(
                    store
                        .read(|conn| {
                            history(conn, queue, &query.log_name)
                                .map_err(hydrus_store::StoreError::Invalid)
                        })
                        .map_err(|e| e.to_string())?,
                );
                if query.reference_header.is_none() {
                    query.reference_header = store
                        .read(|conn| Ok(settings::get::<Headers>(conn)?.0.get(&queue).cloned()))
                        .map_err(|e| e.to_string())?;
                }
            }
            if draft.queue.is_some()
                && let Some(header) = &query.reference_header
                && let Some(name) = header[2][0].as_str()
            {
                name.clone_into(&mut query.log_name);
                if let Some(log) = &mut query.log {
                    log.name.clone_from(&query.log_name);
                }
            }
            if query.log.is_none() {
                query.log = Some(exchange::QueryLog {
                    name: query.log_name.clone(),
                    file_seeds: Vec::new(),
                    gallery_seeds: Vec::new(),
                });
            }
            for change in &draft.log_changes {
                let log = query.log.as_mut().expect("initialised history");
                match change {
                    crate::edit_subscription::LogChange::Reset => log.file_seeds.clear(),
                    crate::edit_subscription::LogChange::RetryFailed => {
                        for seed in &mut log.file_seeds {
                            if seed.status == 4 {
                                seed.status = 0;
                                seed.note.clear();
                                seed.hashes.clear();
                                seed.modified = now;
                            }
                        }
                    }
                    crate::edit_subscription::LogChange::RetryIgnored(which) => {
                        for seed in &mut log.file_seeds {
                            if seed.status == 7 && which.matches(&seed.note) {
                                seed.status = 0;
                                seed.note.clear();
                                seed.hashes.clear();
                                seed.modified = now;
                            }
                        }
                    }
                }
            }
            if query.reference_header.is_none() || !draft.log_changes.is_empty() {
                exchange::update_file_status(&mut query, now).map_err(|e| e.to_string())?;
            }
            if let Some(edit) = &draft.checker_edit {
                exchange::apply_checker_edit(&mut query, edit, now).map_err(|e| e.to_string())?;
            }
            // (the reference reads a history it exports into the dialog)
            draft.loaded.set(true);
            queries.push(query);
        }
        out.push(Subscription {
            name: subscription.name.clone(),
            settings: subscription.settings.clone(),
            queries,
            orphaned_logs: Vec::new(),
        });
    }
    Ok(out)
}
