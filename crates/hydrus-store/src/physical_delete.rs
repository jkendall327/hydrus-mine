//! The physical deletion loop's captured per-pass wait, independent of its idle gate.
use crate::{
    Result, StoreError,
    settings::{self, Setting},
};
use hydrus_legacy::{
    objects::ClientOptions,
    serialisable::{SerialisableObject, SerialisableType},
};
use rusqlite::Connection;

/// Raw imported milliseconds are preserved until Options accepts its displayed fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Preferences {
    /// Raw delay captured by each physical maintenance pass.
    pub wait_ms: i64,
}
impl Default for Preferences {
    fn default() -> Self {
        Self { wait_ms: 600 }
    }
}
impl Setting for Preferences {
    const KEY: &'static str = "physical_delete";
}
impl Preferences {
    /// Preserve the original ClientOptions integer without display normalization.
    pub fn from_legacy(options: &ClientOptions) -> Self {
        Self {
            wait_ms: options
                .integers
                .get("ms_to_wait_between_physical_file_deletes")
                .copied()
                .unwrap_or(600),
        }
    }
    /// TimeDeltaWidget.SetValue clamps its minimum, then truncates the fractional remainder.
    pub fn displayed_seconds(&self) -> f64 {
        let seconds = (self.wait_ms as f64 / 1000.0).max(0.02);
        seconds.trunc().clamp(0.0, 59.0)
            + ((seconds % 1.0) * 1000.0).trunc().clamp(0.0, 999.0) / 1000.0
    }
    /// HydrusTime.MillisecondiseS converts entered fields only once on acceptance.
    pub fn set_seconds(&mut self, seconds: f64) {
        self.wait_ms = (seconds.max(0.02) * 1000.0) as i64;
    }
    /// Save explicit fields, or normalize unchanged raw input only if it is still current.
    pub fn save_changed(&self, conn: &Connection, before: &Self) -> Result<()> {
        let latest = load(conn)?;
        let mut displayed = *before;
        displayed.set_seconds(before.displayed_seconds());
        if (self != &displayed || latest == *before) && self != &latest {
            settings::set(conn, self)?;
        }
        Ok(())
    }
}
/// Native value wins; retained ClientOptions supports stores imported before this key existed.
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
    fn recorded_retained_options_native_override_constructor_and_reopen() {
        let fixture = hydrus_testkit::fixture_json("physical_delete_delay.json");
        let tuple = &fixture["legacy_options"];
        let directory = tempfile::tempdir().unwrap();
        let store = crate::Store::open(directory.path()).unwrap();
        let info = tuple[2].to_string();
        let version = i64::try_from(tuple[1].as_u64().unwrap()).unwrap();
        store.write(move |ctx|{ctx.conn().execute("INSERT INTO legacy_objects(source,type_id,name,version,timestamp_ms,dump) VALUES('json_dumps',?,'',?,0,?)",rusqlite::params![u32::from(SerialisableType::CLIENT_OPTIONS.0),version,info])?;Ok(())}).unwrap();
        assert_eq!(store.read(load).unwrap().wait_ms, 1234);
        for case in fixture["controls"].as_array().unwrap() {
            let raw = Preferences {
                wait_ms: case["raw"].as_i64().unwrap(),
            };
            let seconds = raw.displayed_seconds();
            assert_eq!(seconds.trunc() as i64, case["before"][0].as_i64().unwrap());
            assert_eq!(
                ((seconds % 1.0) * 1000.0).round() as i64,
                case["before"][1].as_i64().unwrap()
            );
        }
        store
            .write(|ctx| settings::set(ctx.conn(), &Preferences { wait_ms: 45 }))
            .unwrap();
        assert_eq!(
            crate::Store::open(directory.path())
                .unwrap()
                .read(load)
                .unwrap()
                .wait_ms,
            45
        );
        store
            .write(|ctx| {
                ctx.conn().execute(
                    "UPDATE settings SET value='{\"wait_ms\":\"invalid\"}' WHERE key=?",
                    [Preferences::KEY],
                )?;
                Ok(())
            })
            .unwrap();
        assert!(store.read(load).is_err());
    }
}
