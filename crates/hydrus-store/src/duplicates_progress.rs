//! Saved suppression of the preparation tab percentage only while >99% searched.
use crate::{
    Result, StoreError,
    settings::{self, Setting},
};
use hydrus_legacy::{
    objects::ClientOptions,
    serialisable::{SerialisableObject, SerialisableType},
};
use rusqlite::Connection;

/// The reference's finite preparation-title switch, enabled by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Presentation {
    pub hide_caught_up: bool,
}
impl Default for Presentation {
    fn default() -> Self {
        Self {
            hide_caught_up: true,
        }
    }
}
impl Setting for Presentation {
    const KEY: &'static str = "duplicates_progress";
}
impl Presentation {
    pub fn from_legacy(options: &ClientOptions) -> Self {
        Self {
            hide_caught_up: options
                .booleans
                .get("hide_duplicates_needs_work_message_when_reasonably_caught_up")
                .copied()
                .unwrap_or(true),
        }
    }
}
/// Native value wins; old imported stores retain the original ClientOptions.
pub fn load(conn: &Connection) -> Result<Presentation> {
    let native: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM settings WHERE key=?)",
        [Presentation::KEY],
        |r| r.get(0),
    )?;
    if native {
        return settings::get(conn);
    }
    let kind = SerialisableType::CLIENT_OPTIONS;
    let Some((version, info)) = crate::legacy::singleton(conn, u32::from(kind.0))? else {
        return Ok(Presentation::default());
    };
    let object = SerialisableObject::from_stored(kind, None, version, &info)
        .map_err(|e| StoreError::Corrupt(e.to_string()))?;
    let options =
        ClientOptions::from_object(&object).map_err(|e| StoreError::Corrupt(e.to_string()))?;
    Ok(Presentation::from_legacy(&options))
}

#[cfg(test)]
mod tests {
    use super::*;
    // leaf: audit-options-duplicates-duplicates-filter-page-hide-the-x-done-notification-on-preparation-tab-when-99-searched
    #[test]
    fn real_saved_disabled_legacy_policy_backfills_native_override_and_reopens() {
        let fixture = hydrus_testkit::fixture_json("duplicates_progress_option.json");
        let tuple = &fixture["legacy"];
        let object = SerialisableObject::from_tuple_str(&tuple.to_string()).unwrap();
        let legacy = ClientOptions::from_object(&object).unwrap();
        assert!(!Presentation::from_legacy(&legacy).hide_caught_up);
        assert_eq!(
            serde_json::from_str::<Presentation>("{}").unwrap(),
            Presentation::default()
        );
        let dir = tempfile::tempdir().unwrap();
        let store = crate::Store::open(dir.path()).unwrap();
        assert!(store.read(load).unwrap().hide_caught_up);
        let info = tuple[2].to_string();
        let version = i64::try_from(tuple[1].as_u64().unwrap()).unwrap();
        store.write(move |c| {
            c.conn().execute("INSERT INTO legacy_objects(source,type_id,name,version,timestamp_ms,dump) VALUES('json_dumps',?,'',?,0,?)", rusqlite::params![u32::from(SerialisableType::CLIENT_OPTIONS.0), version, info])?; Ok(())
        }).unwrap();
        assert!(!store.read(load).unwrap().hide_caught_up);
        for case in fixture["cases"].as_array().unwrap() {
            let enabled = case["saved"].as_bool().unwrap();
            store
                .write(move |c| {
                    settings::set(
                        c.conn(),
                        &Presentation {
                            hide_caught_up: enabled,
                        },
                    )
                })
                .unwrap();
            let reopened = crate::Store::open(store.dir()).unwrap();
            assert_eq!(
                reopened.read(load).unwrap().hide_caught_up,
                case["reopened"].as_bool().unwrap()
            );
        }
        store
            .write(|c| {
                c.conn().execute(
                    "UPDATE settings SET value='invalid' WHERE key=?",
                    [Presentation::KEY],
                )?;
                Ok(())
            })
            .unwrap();
        assert!(store.read(load).is_err());
    }
}
