//! Subscriptions: named sets of gallery queries checked on a schedule.
//!
//! A subscription keeps its settings; each of its queries keeps its state
//! (text, timing, paused or dead) and has an import queue of kind
//! [`QueueKind::Subscription`] holding what it has found (file seeds) and
//! the pages it has read (gallery seeds).

use rusqlite::{Connection, OptionalExtension, params};

use hydrus_core::import_options::ImportOptionsSlice;
use hydrus_core::subscriptions::{QueryState, SubscriptionSettings};

use crate::error::{Result, StoreError};
use crate::queues::{self, QueueKind};

#[derive(Debug, Clone, PartialEq)]
pub struct Subscription {
    pub id: i64,
    pub name: String,
    pub settings: SubscriptionSettings,
}

/// A subscription's query; `queue_id` names it and its history.
#[derive(Debug, Clone, PartialEq)]
pub struct SubscriptionQuery {
    pub queue_id: i64,
    pub subscription_id: i64,
    pub state: QueryState,
}

fn json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("plain data serialises")
}

fn parse<T: for<'de> serde::Deserialize<'de>>(text: &str, what: &str) -> Result<T> {
    serde_json::from_str(text).map_err(|e| StoreError::Corrupt(format!("{what}: {e}")))
}

/// Add a subscription; `Ok(None)` if the name is taken.
pub fn create_subscription(
    conn: &Connection,
    name: &str,
    settings: &SubscriptionSettings,
) -> Result<Option<i64>> {
    let added = conn
        .prepare_cached("INSERT OR IGNORE INTO subscriptions (name, settings) VALUES (?, ?)")?
        .execute(params![name, json(settings)])?;
    Ok((added == 1).then(|| conn.last_insert_rowid()))
}

fn subscription_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<(i64, String, String)> {
    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
}

fn finish_subscription((id, name, settings): (i64, String, String)) -> Result<Subscription> {
    Ok(Subscription {
        id,
        settings: parse(&settings, &format!("subscription {name:?} settings"))?,
        name,
    })
}

/// Every subscription, by name.
pub fn subscriptions(conn: &Connection) -> Result<Vec<Subscription>> {
    let mut stmt = conn.prepare_cached(
        "SELECT subscription_id, name, settings FROM subscriptions ORDER BY name",
    )?;
    let rows = stmt
        .query_map([], subscription_from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter().map(finish_subscription).collect()
}

pub fn subscription(conn: &Connection, id: i64) -> Result<Option<Subscription>> {
    conn.prepare_cached(
        "SELECT subscription_id, name, settings FROM subscriptions WHERE subscription_id = ?",
    )?
    .query_row([id], subscription_from_row)
    .optional()?
    .map(finish_subscription)
    .transpose()
}

pub fn find_subscription(conn: &Connection, name: &str) -> Result<Option<Subscription>> {
    conn.prepare_cached("SELECT subscription_id, name, settings FROM subscriptions WHERE name = ?")?
        .query_row([name], subscription_from_row)
        .optional()?
        .map(finish_subscription)
        .transpose()
}

pub fn set_subscription_settings(
    conn: &Connection,
    id: i64,
    settings: &SubscriptionSettings,
) -> Result<()> {
    conn.prepare_cached("UPDATE subscriptions SET settings = ? WHERE subscription_id = ?")?
        .execute(params![json(settings), id])?;
    Ok(())
}

/// Rename a subscription; `false` if the name is taken.
pub fn rename_subscription(conn: &Connection, id: i64, name: &str) -> Result<bool> {
    let taken = find_subscription(conn, name)?.is_some_and(|s| s.id != id);
    if !taken {
        conn.prepare_cached("UPDATE subscriptions SET name = ? WHERE subscription_id = ?")?
            .execute(params![name, id])?;
    }
    Ok(!taken)
}

/// Delete a subscription with its queries and their histories.
pub fn delete_subscription(conn: &Connection, id: i64) -> Result<()> {
    for query in queries(conn, id)? {
        remove_query(conn, query.queue_id)?;
    }
    conn.prepare_cached("DELETE FROM subscriptions WHERE subscription_id = ?")?
        .execute([id])?;
    Ok(())
}

/// Add a query to the end of a subscription's; its queue's id.
pub fn add_query(
    conn: &Connection,
    subscription_id: i64,
    state: &QueryState,
    now: i64,
) -> Result<i64> {
    let queue = queues::create_queue(
        conn,
        QueueKind::Subscription,
        &state.query_text,
        None,
        &ImportOptionsSlice::default(),
        now,
    )?;
    let position: i64 = conn
        .prepare_cached(
            "SELECT COALESCE(MAX(position), 0) + 1 FROM subscription_queries WHERE subscription_id = ?",
        )?
        .query_row([subscription_id], |r| r.get(0))?;
    conn.prepare_cached(
        "INSERT INTO subscription_queries (queue_id, subscription_id, position, state) VALUES (?, ?, ?, ?)",
    )?
    .execute(params![queue, subscription_id, position, json(state)])?;
    Ok(queue)
}

fn query_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<(i64, i64, String)> {
    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
}

fn finish_query(
    (queue_id, subscription_id, state): (i64, i64, String),
) -> Result<SubscriptionQuery> {
    Ok(SubscriptionQuery {
        queue_id,
        subscription_id,
        state: parse(&state, &format!("subscription query {queue_id} state"))?,
    })
}

/// A subscription's queries, in order.
pub fn queries(conn: &Connection, subscription_id: i64) -> Result<Vec<SubscriptionQuery>> {
    let mut stmt = conn.prepare_cached(
        "SELECT queue_id, subscription_id, state FROM subscription_queries WHERE subscription_id = ? ORDER BY position",
    )?;
    let rows = stmt
        .query_map([subscription_id], query_from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter().map(finish_query).collect()
}

pub fn query(conn: &Connection, queue_id: i64) -> Result<Option<SubscriptionQuery>> {
    conn.prepare_cached(
        "SELECT queue_id, subscription_id, state FROM subscription_queries WHERE queue_id = ?",
    )?
    .query_row([queue_id], query_from_row)
    .optional()?
    .map(finish_query)
    .transpose()
}

pub fn set_query_state(conn: &Connection, queue_id: i64, state: &QueryState) -> Result<()> {
    conn.prepare_cached("UPDATE subscription_queries SET state = ? WHERE queue_id = ?")?
        .execute(params![json(state), queue_id])?;
    conn.prepare_cached("UPDATE import_queues SET name = ? WHERE queue_id = ?")?
        .execute(params![state.query_text, queue_id])?;
    Ok(())
}

/// Move a query, with its history, to the end of another subscription's
/// (merging and separating subscriptions).
pub fn move_query(conn: &Connection, queue_id: i64, subscription_id: i64) -> Result<()> {
    let position: i64 = conn
        .prepare_cached(
            "SELECT COALESCE(MAX(position), 0) + 1 FROM subscription_queries WHERE subscription_id = ?",
        )?
        .query_row([subscription_id], |r| r.get(0))?;
    conn.prepare_cached(
        "UPDATE subscription_queries SET subscription_id = ?, position = ? WHERE queue_id = ?",
    )?
    .execute(params![subscription_id, position, queue_id])?;
    Ok(())
}

/// Remove a query with its history.
pub fn remove_query(conn: &Connection, queue_id: i64) -> Result<()> {
    conn.prepare_cached("DELETE FROM subscription_queries WHERE queue_id = ?")?
        .execute([queue_id])?;
    queues::delete_queue(conn, queue_id)
}
