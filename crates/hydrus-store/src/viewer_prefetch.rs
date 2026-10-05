//! Saved native viewer neighbourhood policy, independent from decoded cache ownership.
use crate::{
    Result, StoreError,
    settings::{self, Setting},
};
use hydrus_legacy::{
    objects::ClientOptions,
    serialisable::{SerialisableObject, SerialisableType},
};
use rusqlite::Connection;

/// Counts are circular neighbours; percentage limits each ordered viewer pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub previous: u64,
    pub next: u64,
    pub percentage: u64,
    /// Retained supporting duplicate-filter count; no native editor in this slice.
    pub duplicate_pairs: u64,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            previous: 2,
            next: 3,
            percentage: 25,
            duplicate_pairs: 3,
        }
    }
}
impl Setting for Preferences {
    const KEY: &'static str = "viewer_prefetch";
}
impl Preferences {
    pub fn from_legacy(options: &ClientOptions) -> Self {
        let default = Self::default();
        let integer = |name, fallback| {
            options
                .integers
                .get(name)
                .map_or(fallback, |value| (*value).max(0) as u64)
        };
        Self {
            previous: integer("media_viewer_prefetch_num_previous", default.previous),
            next: integer("media_viewer_prefetch_num_next", default.next),
            percentage: integer("image_cache_prefetch_limit_percentage", default.percentage),
            duplicate_pairs: integer(
                "duplicate_filter_prefetch_num_pairs",
                default.duplicate_pairs,
            ),
        }
    }
    /// Actual initial spinbox bounds; passive Apply normalizes only unchanged fields.
    #[must_use]
    pub fn displayed(self) -> Self {
        Self {
            previous: self.previous.min(50),
            next: self.next.min(50),
            percentage: self.percentage.clamp(10, 50),
            duplicate_pairs: self.duplicate_pairs,
        }
    }
    pub fn save_changed(self, conn: &Connection, before: Self) -> Result<()> {
        if self == before {
            return Ok(());
        }
        let displayed = before.displayed();
        let mut latest = load(conn)?;
        for (after, before, displayed, field) in [
            (
                self.previous,
                before.previous,
                displayed.previous,
                &mut latest.previous,
            ),
            (self.next, before.next, displayed.next, &mut latest.next),
            (
                self.percentage,
                before.percentage,
                displayed.percentage,
                &mut latest.percentage,
            ),
        ] {
            if after != before && (after != displayed || *field == before) {
                *field = after;
            }
        }
        settings::set(conn, &latest)
    }
}
/// Typed native policy takes precedence over retained imported ClientOptions.
pub fn load(conn: &Connection) -> Result<Preferences> {
    let native: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM settings WHERE key=?)",
        [Preferences::KEY],
        |row| row.get(0),
    )?;
    if native {
        return settings::get(conn);
    }
    let kind = SerialisableType::CLIENT_OPTIONS;
    let Some((version, info)) = crate::legacy::singleton(conn, u32::from(kind.0))? else {
        return Ok(Preferences::default());
    };
    let object = SerialisableObject::from_stored(kind, None, version, &info)
        .map_err(|error| StoreError::Corrupt(error.to_string()))?;
    let options = ClientOptions::from_object(&object)
        .map_err(|error| StoreError::Corrupt(error.to_string()))?;
    Ok(Preferences::from_legacy(&options))
}
