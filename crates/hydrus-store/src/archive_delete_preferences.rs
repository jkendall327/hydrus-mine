//! Saved policy for the archive/delete filter's owned finish choices.
use crate::{Result, settings};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// Independent domain simplification and accidental-commit protection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub all_domains: bool,
    pub delay_multiple: bool,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            all_domains: false,
            delay_multiple: true,
        }
    }
}
impl settings::Setting for Preferences {
    const KEY: &'static str = "archive_delete_finish";
}
impl Preferences {
    /// Decode the real ClientOptions keys without coupling GUI and persistence.
    pub fn from_legacy(options: &hydrus_legacy::objects::ClientOptions) -> Self {
        let mut value = Self::default();
        for (name, field) in [
            (
                "only_show_delete_from_all_local_domains_when_filtering",
                &mut value.all_domains,
            ),
            (
                "archive_delete_commit_panel_delays_multiple_delete_choices",
                &mut value.delay_multiple,
            ),
        ] {
            if let Some(saved) = options.booleans.get(name) {
                *field = *saved;
            }
        }
        value
    }
    /// Merge only the fields changed by this Options draft.
    pub fn save_changed(&self, conn: &Connection, before: &Self) -> Result<()> {
        if self == before {
            return Ok(());
        }
        let mut latest = load(conn)?;
        if self.all_domains != before.all_domains {
            latest.all_domains = self.all_domains;
        }
        if self.delay_multiple != before.delay_multiple {
            latest.delay_multiple = self.delay_multiple;
        }
        settings::set(conn, &latest)
    }
}
/// Native settings take precedence; old imports retain their original policy.
pub fn load(conn: &Connection) -> Result<Preferences> {
    use crate::settings::Setting as _;
    use hydrus_legacy::{
        objects::ClientOptions,
        serialisable::{SerialisableObject, SerialisableType},
    };
    let saved: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM settings WHERE key=?)",
        [Preferences::KEY],
        |row| row.get(0),
    )?;
    if saved {
        return settings::get(conn);
    }
    let kind = SerialisableType::CLIENT_OPTIONS;
    let Some((version, info)) = crate::legacy::singleton(conn, u32::from(kind.0))? else {
        return Ok(Preferences::default());
    };
    let object = SerialisableObject::from_stored(kind, None, version, &info)
        .map_err(|error| crate::error::StoreError::Corrupt(error.to_string()))?;
    let options = ClientOptions::from_object(&object)
        .map_err(|error| crate::error::StoreError::Corrupt(error.to_string()))?;
    Ok(Preferences::from_legacy(&options))
}
