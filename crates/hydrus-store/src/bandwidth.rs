//! Bandwidth rules (a setting) and each network context's usage, kept across
//! restarts so a daily limit stays a daily limit. The rules and the counting
//! are `hydrus_core::bandwidth`'s; the network engine holds them in memory
//! and saves the usage that changed every so often.

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use hydrus_core::bandwidth::{Rules, Tracker, default_rules};
use hydrus_core::network::NetworkContext;

use crate::error::{Result, StoreError};
use crate::settings::Setting;

/// The bandwidth rules per network context, and the options that pace
/// gallery pages and let file downloads skip the queue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BandwidthSettings {
    /// Each context's rules, the kinds' defaults included.
    pub rules: Vec<(NetworkContext, Rules)>,
    /// Seconds between gallery pages of one site, for gallery downloaders
    /// (`gallery_page_wait_period_pages`).
    pub gallery_page_wait_pages: i64,
    /// ... for subscriptions (`gallery_page_wait_period_subscriptions`).
    pub gallery_page_wait_subscriptions: i64,
    /// ... for thread checks (`watcher_page_wait_period`).
    pub watcher_page_wait: i64,
    /// A file found on a post page waits at most a few seconds for
    /// bandwidth (`override_bandwidth_on_file_urls_from_post_urls`).
    pub override_on_file_urls_from_posts: bool,
}

impl Default for BandwidthSettings {
    fn default() -> Self {
        Self {
            rules: default_rules(),
            gallery_page_wait_pages: 15,
            gallery_page_wait_subscriptions: 5,
            watcher_page_wait: 5,
            override_on_file_urls_from_posts: true,
        }
    }
}

impl Setting for BandwidthSettings {
    const KEY: &'static str = "bandwidth";
}

/// Every context's stored usage, as loaded at `now`.
pub fn usage(conn: &Connection, now: i64) -> Result<Vec<(NetworkContext, Tracker)>> {
    let mut stmt =
        conn.prepare_cached("SELECT context_kind, context_data, tracker FROM bandwidth_usage")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (kind, data, json) = row?;
        let tracker: Tracker = serde_json::from_str(&json)
            .map_err(|e| StoreError::Invalid(format!("stored bandwidth usage: {e}")))?;
        out.push((NetworkContext { kind, data }, tracker.loaded_at(now)));
    }
    Ok(out)
}

/// Keep these contexts' usage (replacing what was kept).
pub fn save_usage(conn: &Connection, usage: &[(NetworkContext, Tracker)]) -> Result<()> {
    let mut stmt = conn.prepare_cached(
        "INSERT OR REPLACE INTO bandwidth_usage (context_kind, context_data, tracker)
         VALUES (?, ?, ?)",
    )?;
    for (context, tracker) in usage {
        let json = serde_json::to_string(tracker)
            .map_err(|e| StoreError::Invalid(format!("bandwidth usage: {e}")))?;
        stmt.execute(params![context.kind, context.data, json])?;
    }
    Ok(())
}

/// Per-context reset generations prevent an already-running engine's stale saves
/// from restoring history the user deleted. Rules are independent of this record.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryResets(pub Vec<(NetworkContext, u64)>);
impl Setting for HistoryResets {
    const KEY: &'static str = "bandwidth_history_resets";
}
impl HistoryResets {
    /// The context's generation, zero until its first deletion.
    pub fn generation(&self, context: &NetworkContext) -> u64 {
        self.0
            .iter()
            .find(|(c, _)| c == context)
            .map_or(0, |(_, n)| *n)
    }
    /// Contexts whose trackers must be discarded before accepting new usage.
    pub fn changed_since(&self, old: &Self) -> Vec<NetworkContext> {
        self.0
            .iter()
            .filter(|(c, n)| *n > old.generation(c))
            .map(|(c, _)| c.clone())
            .collect()
    }
}

/// Delete all usage for these contexts and mark their live trackers for reset.
/// Snapshot and persisted usage are changed in the same writer transaction.
pub fn delete_history(conn: &Connection, contexts: &[NetworkContext]) -> Result<()> {
    let mut resets = crate::settings::get::<HistoryResets>(conn)?;
    for context in contexts {
        let generation = resets
            .generation(context)
            .checked_add(1)
            .ok_or_else(|| StoreError::Invalid("Bandwidth history generation exhausted.".into()))?;
        resets.0.retain(|(c, _)| c != context);
        resets.0.push((context.clone(), generation));
        conn.execute(
            "DELETE FROM bandwidth_usage WHERE context_kind = ?1 AND context_data = ?2",
            params![context.kind, context.data],
        )?;
    }
    crate::settings::set(conn, &resets)?;
    let mut snapshot = crate::settings::get::<crate::network_runtime::Snapshot>(conn)?;
    snapshot.usage.retain(|(c, _)| !contexts.contains(c));
    crate::settings::set(conn, &snapshot)
}

/// Save only usage counted after the most recent deletion, in the writer transaction.
pub fn save_usage_after_resets(
    conn: &Connection,
    usage: &[(NetworkContext, Tracker)],
    seen: &HistoryResets,
) -> Result<()> {
    let current = crate::settings::get::<HistoryResets>(conn)?;
    let fresh: Vec<_> = usage
        .iter()
        .filter(|(c, _)| current.generation(c) == seen.generation(c))
        .cloned()
        .collect();
    save_usage(conn, &fresh)
}

/// Add only newly counted bucket usage from an independent network engine.
/// The writer transaction reads the latest totals and reset generations so a
/// GUI test fetch cannot replace daemon traffic or restore deleted history.
pub fn save_usage_deltas_after_resets(
    conn: &Connection,
    current_usage: &[(NetworkContext, Tracker)],
    previously_saved: &[(NetworkContext, Tracker)],
    seen: &HistoryResets,
    now: i64,
) -> Result<()> {
    let current_resets = crate::settings::get::<HistoryResets>(conn)?;
    let stored = usage(conn, now)?;
    let mut merged = Vec::new();
    for (context, tracker) in current_usage {
        if context.is_ephemeral() || current_resets.generation(context) != seen.generation(context)
        {
            continue;
        }
        let current = tracker.to_counters();
        let previous = previously_saved
            .iter()
            .find(|(c, _)| c == context)
            .map_or_else(
                || std::array::from_fn(|_| Vec::new()),
                |(_, t)| t.to_counters(),
            );
        let existing = stored.iter().find(|(c, _)| c == context).map_or_else(
            || std::array::from_fn(|_| Vec::new()),
            |(_, t)| t.to_counters(),
        );
        let mut changed = false;
        let counters = std::array::from_fn(|i| {
            let before: std::collections::BTreeMap<_, _> = previous[i].iter().copied().collect();
            let mut totals: std::collections::BTreeMap<_, _> =
                existing[i].iter().copied().collect();
            for (at, count) in &current[i] {
                let delta = count.saturating_sub(before.get(at).copied().unwrap_or(0));
                if delta > 0 {
                    changed = true;
                    let total = totals.entry(*at).or_insert(0);
                    *total = total.saturating_add(delta);
                }
            }
            totals.into_iter().collect()
        });
        if changed {
            merged.push((context.clone(), Tracker::from_counters(counters, now)));
        }
    }
    save_usage(conn, &merged)
}
