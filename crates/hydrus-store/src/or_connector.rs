//! The reference's saved one-line OR connector control. Its custom rendering
//! loop is disabled in Qt; storing this value does not change predicate syntax.
use crate::{
    Result, StoreError,
    settings::{self, Setting},
};
use hydrus_legacy::{
    objects::ClientOptions,
    serialisable::{SerialisableObject, SerialisableType},
};
use rusqlite::Connection;

/// Raw saved editor text, including blank and programmatically loaded newlines.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Connector {
    /// The QLineEdit value; current OR labels retain their literal separators.
    pub text: String,
}
impl Default for Connector {
    fn default() -> Self {
        Self {
            text: " OR ".into(),
        }
    }
}
impl Setting for Connector {
    const KEY: &'static str = "or_connector";
}
impl Connector {
    /// Retain the reference option without trimming or enabling its dormant renderer.
    pub fn from_legacy(options: &ClientOptions) -> Self {
        Self {
            text: options
                .strings
                .get("or_connector")
                .cloned()
                .unwrap_or_else(|| Self::default().text),
        }
    }
}
/// Saved native editor text wins; an old imported store backfills from its
/// retained ClientOptions without requiring a second import or schema change.
pub fn load(conn: &Connection) -> Result<Connector> {
    let saved: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM settings WHERE key=?)",
        [Connector::KEY],
        |r| r.get(0),
    )?;
    if saved {
        return settings::get(conn);
    }
    let kind = SerialisableType::CLIENT_OPTIONS;
    let Some((version, info)) = crate::legacy::singleton(conn, u32::from(kind.0))? else {
        return Ok(Connector::default());
    };
    let object = SerialisableObject::from_stored(kind, None, version, &info)
        .map_err(|e| StoreError::Corrupt(e.to_string()))?;
    let options =
        ClientOptions::from_object(&object).map_err(|e| StoreError::Corrupt(e.to_string()))?;
    Ok(Connector::from_legacy(&options))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_qt_serialised_raw_connector_backfills_and_native_override_survives_reopen() {
        let recorded = hydrus_testkit::fixture_json("or_connector.json");
        let tuple = &recorded["legacy"];
        let object = SerialisableObject::from_tuple_str(&tuple.to_string()).unwrap();
        let legacy = ClientOptions::from_object(&object).unwrap();
        assert_eq!(
            Connector::from_legacy(&legacy).text,
            recorded["legacy_value"]
        );
        let directory = tempfile::tempdir().unwrap();
        let store = crate::Store::open(directory.path()).unwrap();
        let info = tuple[2].to_string();
        let version = i64::try_from(tuple[1].as_u64().unwrap()).unwrap();
        store.write(move |c| {
            c.conn().execute("INSERT INTO legacy_objects(source,type_id,name,version,timestamp_ms,dump) VALUES('json_dumps',?,'',?,0,?)",rusqlite::params![u32::from(SerialisableType::CLIENT_OPTIONS.0),version,info])?;Ok(())
        }).unwrap();
        assert_eq!(store.read(load).unwrap().text, recorded["legacy_value"]);
        for case in recorded["cases"].as_array().unwrap() {
            let value = Connector {
                text: case["saved"].as_str().unwrap().into(),
            };
            let saved = value.clone();
            store
                .write(move |c| settings::set(c.conn(), &saved))
                .unwrap();
            let reopened = crate::Store::open(store.dir()).unwrap();
            assert_eq!(reopened.read(load).unwrap(), value);
        }
        assert_eq!(
            serde_json::from_str::<Connector>("{}").unwrap(),
            Connector::default()
        );
        store
            .write(|c| {
                c.conn().execute(
                    "UPDATE settings SET value='bad json' WHERE key=?",
                    [Connector::KEY],
                )?;
                Ok(())
            })
            .unwrap();
        assert!(
            store.read(load).is_err(),
            "corrupt native value cannot silently fall back to legacy"
        );
    }
}
