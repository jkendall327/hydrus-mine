//! Independent raw gallery/watcher list-update controls, including legacy values.
use crate::{Result, settings};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Gallery,
    Watcher,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub gallery_minimum_ms: i64,
    pub gallery_denominator: i64,
    pub watcher_minimum_ms: i64,
    pub watcher_denominator: i64,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            gallery_minimum_ms: 1000,
            gallery_denominator: 30,
            watcher_minimum_ms: 1000,
            watcher_denominator: 30,
        }
    }
}
impl settings::Setting for Preferences {
    const KEY: &'static str = "downloader_update_times";
}
impl Preferences {
    pub fn policy(&self, kind: Kind) -> (i64, i64) {
        match kind {
            Kind::Gallery => (self.gallery_minimum_ms, self.gallery_denominator),
            Kind::Watcher => (self.watcher_minimum_ms, self.watcher_denominator),
        }
    }
    pub fn apply_legacy(&mut self, integers: &BTreeMap<String, i64>) {
        for (key, destination) in [
            (
                "gallery_page_status_update_time_minimum_ms",
                &mut self.gallery_minimum_ms,
            ),
            (
                "gallery_page_status_update_time_ratio_denominator",
                &mut self.gallery_denominator,
            ),
            (
                "watcher_page_status_update_time_minimum_ms",
                &mut self.watcher_minimum_ms,
            ),
            (
                "watcher_page_status_update_time_ratio_denominator",
                &mut self.watcher_denominator,
            ),
        ] {
            if let Some(&value) = integers.get(key) {
                *destination = value;
            }
        }
    }
    /// Preserve concurrent changes when acceptance only normalizes an imported field.
    pub fn save_changed(
        &self,
        conn: &Connection,
        before: &Self,
        displayed_before: &Self,
    ) -> Result<()> {
        let mut latest: Self = settings::get(conn)?;
        for (after, before, displayed, field) in [
            (
                self.gallery_minimum_ms,
                before.gallery_minimum_ms,
                displayed_before.gallery_minimum_ms,
                &mut latest.gallery_minimum_ms,
            ),
            (
                self.gallery_denominator,
                before.gallery_denominator,
                displayed_before.gallery_denominator,
                &mut latest.gallery_denominator,
            ),
            (
                self.watcher_minimum_ms,
                before.watcher_minimum_ms,
                displayed_before.watcher_minimum_ms,
                &mut latest.watcher_minimum_ms,
            ),
            (
                self.watcher_denominator,
                before.watcher_denominator,
                displayed_before.watcher_denominator,
                &mut latest.watcher_denominator,
            ),
        ] {
            if after != before && (after != displayed || *field == before) {
                *field = after;
            }
        }
        settings::set(conn, &latest)
    }
}
