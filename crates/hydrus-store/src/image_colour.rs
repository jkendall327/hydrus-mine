//! Saved embedded ICC policy. PNG gamma/chromaticity is a separate decode rule.
use crate::{
    Result, StoreError,
    settings::{self, Setting},
};
use hydrus_legacy::{
    objects::ClientOptions,
    serialisable::{SerialisableObject, SerialisableType},
};
use rusqlite::Connection;

/// The reference's finite embedded-profile switch, enabled by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ImageColour {
    pub normalise_icc: bool,
}
impl Default for ImageColour {
    fn default() -> Self {
        Self {
            normalise_icc: true,
        }
    }
}
impl Setting for ImageColour {
    const KEY: &'static str = "image_colour";
}
impl ImageColour {
    pub fn from_legacy(options: &ClientOptions) -> Self {
        Self {
            normalise_icc: options
                .booleans
                .get("do_icc_profile_normalisation")
                .copied()
                .unwrap_or(true),
        }
    }
}
/// Native value wins; old imported stores retain the original ClientOptions.
pub fn load(conn: &Connection) -> Result<ImageColour> {
    let native: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM settings WHERE key=?)",
        [ImageColour::KEY],
        |r| r.get(0),
    )?;
    if native {
        return settings::get(conn);
    }
    let kind = SerialisableType::CLIENT_OPTIONS;
    let Some((version, info)) = crate::legacy::singleton(conn, u32::from(kind.0))? else {
        return Ok(ImageColour::default());
    };
    let object = SerialisableObject::from_stored(kind, None, version, &info)
        .map_err(|e| StoreError::Corrupt(e.to_string()))?;
    let options =
        ClientOptions::from_object(&object).map_err(|e| StoreError::Corrupt(e.to_string()))?;
    Ok(ImageColour::from_legacy(&options))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_saved_disabled_legacy_policy_backfills_native_override_and_reopens() {
        let fixture = hydrus_testkit::fixture_json("image_decoder_policies.json");
        let tuple = &fixture["legacy"];
        let object = SerialisableObject::from_tuple_str(&tuple.to_string()).unwrap();
        let legacy = ClientOptions::from_object(&object).unwrap();
        assert!(!ImageColour::from_legacy(&legacy).normalise_icc);
        assert_eq!(
            serde_json::from_str::<ImageColour>("{}").unwrap(),
            ImageColour::default()
        );
        let dir = tempfile::tempdir().unwrap();
        let store = crate::Store::open(dir.path()).unwrap();
        assert!(store.read(load).unwrap().normalise_icc);
        let info = tuple[2].to_string();
        let version = tuple[1].as_u64().unwrap();
        store.write(move |c| {
            c.conn().execute("INSERT INTO legacy_objects(source,type_id,name,version,timestamp_ms,dump) VALUES('json_dumps',?,'',?,0,?)", rusqlite::params![u32::from(SerialisableType::CLIENT_OPTIONS.0), version, info])?; Ok(())
        }).unwrap();
        assert!(!store.read(load).unwrap().normalise_icc);
        for case in fixture["cases"].as_array().unwrap() {
            let enabled = case["saved"][0].as_bool().unwrap();
            store
                .write(move |c| {
                    settings::set(
                        c.conn(),
                        &ImageColour {
                            normalise_icc: enabled,
                        },
                    )
                })
                .unwrap();
            let reopened = crate::Store::open(store.dir()).unwrap();
            assert_eq!(
                reopened.read(load).unwrap().normalise_icc,
                case["reopened"][0].as_bool().unwrap()
            );
        }
        store
            .write(|c| {
                c.conn().execute(
                    "UPDATE settings SET value='invalid' WHERE key=?",
                    [ImageColour::KEY],
                )?;
                Ok(())
            })
            .unwrap();
        assert!(store.read(load).is_err());
    }
}
