//! Live radio Return policy, including retained legacy values in older stores.
use crate::{
    Result, StoreError,
    settings::{self, Setting},
};
use hydrus_legacy::{
    objects::ClientOptions,
    serialisable::{SerialisableObject, SerialisableType},
};
use rusqlite::Connection;

/// Force Enter/Return from a radio list to the dialog's OK route.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct RadioReturn {
    pub force_dialog_ok: bool,
}
impl Default for RadioReturn {
    fn default() -> Self {
        Self {
            force_dialog_ok: true,
        }
    }
}
impl Setting for RadioReturn {
    const KEY: &'static str = "radio_return";
}
impl RadioReturn {
    pub fn from_legacy(options: &ClientOptions) -> Self {
        Self {
            force_dialog_ok: options
                .booleans
                .get("force_enter_on_radio_buttons_to_do_dialog_ok")
                .copied()
                .unwrap_or(true),
        }
    }
}
/// Native value wins; old imported stores retain the original ClientOptions.
pub fn load(conn: &Connection) -> Result<RadioReturn> {
    let native: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM settings WHERE key=?)",
        [RadioReturn::KEY],
        |r| r.get(0),
    )?;
    if native {
        return settings::get(conn);
    }
    let kind = SerialisableType::CLIENT_OPTIONS;
    let Some((version, info)) = crate::legacy::singleton(conn, u32::from(kind.0))? else {
        return Ok(RadioReturn::default());
    };
    let object = SerialisableObject::from_stored(kind, None, version, &info)
        .map_err(|e| StoreError::Corrupt(e.to_string()))?;
    let options =
        ClientOptions::from_object(&object).map_err(|e| StoreError::Corrupt(e.to_string()))?;
    Ok(RadioReturn::from_legacy(&options))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_saved_false_legacy_fallback_native_override_and_durable_reopen() {
        let reference = hydrus_testkit::fixture_json("radio_return.json");
        let tuple = &reference["legacy"];
        let object = SerialisableObject::from_tuple_str(&tuple.to_string()).unwrap();
        let mut options = ClientOptions::from_object(&object).unwrap();
        assert!(!RadioReturn::from_legacy(&options).force_dialog_ok);
        options
            .booleans
            .remove("force_enter_on_radio_buttons_to_do_dialog_ok");
        assert_eq!(RadioReturn::from_legacy(&options), RadioReturn::default());
        assert_eq!(
            serde_json::from_str::<RadioReturn>("{}").unwrap(),
            RadioReturn::default()
        );
        assert_eq!(
            RadioReturn::default().force_dialog_ok,
            reference["default"].as_bool().unwrap()
        );
        let directory = tempfile::tempdir().unwrap();
        let store = crate::Store::open(directory.path()).unwrap();
        assert!(store.read(load).unwrap().force_dialog_ok);
        let info = tuple[2].to_string();
        let version = tuple[1].as_i64().unwrap();
        store.write(move |c| {
            c.conn().execute("INSERT INTO legacy_objects(source,type_id,name,version,timestamp_ms,dump) VALUES('json_dumps',?,'',?,0,?)", rusqlite::params![u32::from(SerialisableType::CLIENT_OPTIONS.0), version, info])?;
            Ok(())
        }).unwrap();
        assert!(!store.read(load).unwrap().force_dialog_ok);
        store
            .write(|c| settings::set(c.conn(), &RadioReturn::default()))
            .unwrap();
        assert!(store.read(load).unwrap().force_dialog_ok);
        let reopened = crate::Store::open(directory.path()).unwrap();
        assert!(reopened.read(load).unwrap().force_dialog_ok);
        store
            .write(|c| {
                c.conn().execute(
                    "UPDATE settings SET value='invalid' WHERE key=?",
                    [RadioReturn::KEY],
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
