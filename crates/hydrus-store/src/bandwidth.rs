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
