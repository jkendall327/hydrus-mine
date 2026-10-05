//! Saved minimized-toaster freeze, with typed-native precedence over legacy options.
use crate::{
    Result, StoreError,
    settings::{self, Setting},
};
use hydrus_legacy::{
    objects::ClientOptions,
    serialisable::{SerialisableObject, SerialisableType},
};
use rusqlite::Connection;

/// Freeze toaster UI admission/refresh while its main owner is minimized.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub minimized: bool,
}
impl Setting for Preferences {
    const KEY: &'static str = "popup_freeze";
}
impl Preferences {
    pub fn from_legacy(options: &ClientOptions) -> Self {
        Self {
            minimized: options
                .booleans
                .get("freeze_message_manager_when_main_gui_minimised")
                .copied()
                .unwrap_or(false),
        }
    }
}
/// Native value wins; old imported stores retain the original ClientOptions.
pub fn load(conn: &Connection) -> Result<Preferences> {
    let native: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM settings WHERE key=?)",
        [Preferences::KEY],
        |r| r.get(0),
    )?;
    if native {
        return settings::get(conn);
    }
    let kind = SerialisableType::CLIENT_OPTIONS;
    let Some((version, info)) = crate::legacy::singleton(conn, u32::from(kind.0))? else {
        return Ok(Preferences::default());
    };
    let object = SerialisableObject::from_stored(kind, None, version, &info)
        .map_err(|e| StoreError::Corrupt(e.to_string()))?;
    let options =
        ClientOptions::from_object(&object).map_err(|e| StoreError::Corrupt(e.to_string()))?;
    Ok(Preferences::from_legacy(&options))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_saved_true_legacy_native_override_and_durable_reopen() {
        let reference = hydrus_testkit::fixture_json("popup_freeze.json");
        let tuple = &reference["legacy"];
        let object = SerialisableObject::from_tuple_str(&tuple.to_string()).unwrap();
        let mut options = ClientOptions::from_object(&object).unwrap();
        assert!(Preferences::from_legacy(&options).minimized);
        options
            .booleans
            .remove("freeze_message_manager_when_main_gui_minimised");
        assert_eq!(Preferences::from_legacy(&options), Preferences::default());
        assert_eq!(
            serde_json::from_str::<Preferences>("{}").unwrap(),
            Preferences::default()
        );
        assert_eq!(
            Preferences::default().minimized,
            reference["default"].as_bool().unwrap()
        );
        let directory = tempfile::tempdir().unwrap();
        let store = crate::Store::open(directory.path()).unwrap();
        assert!(!store.read(load).unwrap().minimized);
        let info = tuple[2].to_string();
        let version = tuple[1].as_i64().unwrap();
        store.write(move |c| {
            c.conn().execute("INSERT INTO legacy_objects(source,type_id,name,version,timestamp_ms,dump) VALUES('json_dumps',?,'',?,0,?)", rusqlite::params![u32::from(SerialisableType::CLIENT_OPTIONS.0), version, info])?;
            Ok(())
        }).unwrap();
        assert!(store.read(load).unwrap().minimized);
        store
            .write(|c| settings::set(c.conn(), &Preferences::default()))
            .unwrap();
        assert!(!store.read(load).unwrap().minimized);
        let reopened = crate::Store::open(directory.path()).unwrap();
        assert!(!reopened.read(load).unwrap().minimized);
        store
            .write(|c| {
                c.conn().execute(
                    "UPDATE settings SET value='invalid' WHERE key=?",
                    [Preferences::KEY],
                )?;
                Ok(())
            })
            .unwrap();
        assert!(
            store.read(load).is_err(),
            "corrupt native values must not silently use legacy"
        );
    }
}
