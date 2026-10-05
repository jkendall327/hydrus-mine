//! Live menu-choice wheel policy, including retained legacy values in older stores.
use crate::{
    Result, StoreError,
    settings::{self, Setting},
};
use hydrus_legacy::{
    objects::ClientOptions,
    serialisable::{SerialisableObject, SerialisableType},
};
use rusqlite::Connection;

/// Permit a wheel event to cycle one underlying menu choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct MenuChoiceWheel {
    pub enabled: bool,
}
impl Default for MenuChoiceWheel {
    fn default() -> Self {
        Self { enabled: true }
    }
}
impl Setting for MenuChoiceWheel {
    const KEY: &'static str = "menu_choice_wheel";
}
impl MenuChoiceWheel {
    pub fn from_legacy(options: &ClientOptions) -> Self {
        Self {
            enabled: options
                .booleans
                .get("menu_choice_buttons_can_mouse_scroll")
                .copied()
                .unwrap_or(true),
        }
    }
}
/// Native value wins; old imported stores retain the original ClientOptions.
pub fn load(conn: &Connection) -> Result<MenuChoiceWheel> {
    let native: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM settings WHERE key=?)",
        [MenuChoiceWheel::KEY],
        |r| r.get(0),
    )?;
    if native {
        return settings::get(conn);
    }
    let kind = SerialisableType::CLIENT_OPTIONS;
    let Some((version, info)) = crate::legacy::singleton(conn, u32::from(kind.0))? else {
        return Ok(MenuChoiceWheel::default());
    };
    let object = SerialisableObject::from_stored(kind, None, version, &info)
        .map_err(|e| StoreError::Corrupt(e.to_string()))?;
    let options =
        ClientOptions::from_object(&object).map_err(|e| StoreError::Corrupt(e.to_string()))?;
    Ok(MenuChoiceWheel::from_legacy(&options))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_saved_false_legacy_fallback_native_override_and_durable_reopen() {
        let reference = hydrus_testkit::fixture_json("menu_choice_wheel.json");
        let tuple = &reference["legacy"];
        let object = SerialisableObject::from_tuple_str(&tuple.to_string()).unwrap();
        let mut options = ClientOptions::from_object(&object).unwrap();
        assert!(!MenuChoiceWheel::from_legacy(&options).enabled);
        options
            .booleans
            .remove("menu_choice_buttons_can_mouse_scroll");
        assert_eq!(
            MenuChoiceWheel::from_legacy(&options),
            MenuChoiceWheel::default()
        );
        assert_eq!(
            serde_json::from_str::<MenuChoiceWheel>("{}").unwrap(),
            MenuChoiceWheel::default()
        );
        assert_eq!(
            MenuChoiceWheel::default().enabled,
            reference["default"].as_bool().unwrap()
        );
        let directory = tempfile::tempdir().unwrap();
        let store = crate::Store::open(directory.path()).unwrap();
        assert!(store.read(load).unwrap().enabled);
        let info = tuple[2].to_string();
        let version = tuple[1].as_i64().unwrap();
        store.write(move |c| {
            c.conn().execute("INSERT INTO legacy_objects(source,type_id,name,version,timestamp_ms,dump) VALUES('json_dumps',?,'',?,0,?)", rusqlite::params![u32::from(SerialisableType::CLIENT_OPTIONS.0), version, info])?;
            Ok(())
        }).unwrap();
        assert!(!store.read(load).unwrap().enabled);
        store
            .write(|c| settings::set(c.conn(), &MenuChoiceWheel::default()))
            .unwrap();
        assert!(store.read(load).unwrap().enabled);
        let reopened = crate::Store::open(directory.path()).unwrap();
        assert!(reopened.read(load).unwrap().enabled);
        store
            .write(|c| {
                c.conn().execute(
                    "UPDATE settings SET value='invalid' WHERE key=?",
                    [MenuChoiceWheel::KEY],
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
