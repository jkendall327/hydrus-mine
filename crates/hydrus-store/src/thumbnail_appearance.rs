//! Thumbnail recovery/paint preferences. Renderer choice is admitted by each new page.
use std::collections::BTreeMap;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::{Result, settings};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub fade: bool,
    pub blurhash: bool,
    pub background: Option<String>,
    pub new_renderer: bool,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            fade: true,
            blurhash: true,
            background: None,
            new_renderer: true,
        }
    }
}
impl settings::Setting for Preferences {
    const KEY: &'static str = "thumbnail_appearance";
}
impl Preferences {
    pub fn apply_legacy(
        &mut self,
        booleans: &BTreeMap<String, bool>,
        strings: &BTreeMap<String, Option<String>>,
    ) {
        for (key, destination) in [
            ("fade_thumbnails", &mut self.fade),
            ("allow_blurhash_fallback", &mut self.blurhash),
            ("test_thumbnails_graphics_view", &mut self.new_renderer),
        ] {
            if let Some(&value) = booleans.get(key) {
                *destination = value;
            }
        }
        if let Some(value) = strings.get("media_background_bmp_path") {
            self.background.clone_from(value);
        }
    }

    /// Qt treats exactly an empty path as None; whitespace is a literal path.
    pub fn save_changed(&self, conn: &Connection, before: &Self) -> Result<()> {
        let mut current = load(conn)?;
        if self.fade != before.fade {
            current.fade = self.fade;
        }
        if self.blurhash != before.blurhash {
            current.blurhash = self.blurhash;
        }
        if self.new_renderer != before.new_renderer {
            current.new_renderer = self.new_renderer;
        }
        let normalized = self
            .background
            .as_ref()
            .filter(|path| !path.is_empty())
            .cloned();
        if self.background != before.background
            || (normalized != before.background && current.background == before.background)
        {
            current.background = normalized;
        }
        settings::set(conn, &current)
    }
}

/// Existing native imports retain ClientOptions; native edited values win.
pub fn load(conn: &Connection) -> Result<Preferences> {
    use hydrus_legacy::{
        objects::ClientOptions,
        serialisable::{SerialisableObject, SerialisableType},
    };
    use settings::Setting;
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
        .map_err(|error| crate::StoreError::Corrupt(error.to_string()))?;
    let options = ClientOptions::from_object(&object)
        .map_err(|error| crate::StoreError::Corrupt(error.to_string()))?;
    let mut preferences = Preferences::default();
    preferences.apply_legacy(&options.booleans, &options.noneable_strings);
    Ok(preferences)
}

#[cfg(test)]
mod tests {
    use super::*;
    // leaf: audit-options-thumbnails-appearance-use-blurhash-missing-thumbnail-fallback
    #[test]
    fn retained_actual_options_upgrade_native_override_and_reopen() {
        let fixture = hydrus_testkit::fixture_json("thumbnail_appearance.json");
        let tuple = &fixture["legacy_options"];
        let directory = tempfile::tempdir().unwrap();
        let store = crate::Store::open(directory.path()).unwrap();
        let version = i64::try_from(tuple[1].as_u64().unwrap()).unwrap();
        let info = tuple[2].to_string();
        store.write(move |ctx|{ctx.conn().execute("INSERT INTO legacy_objects(source,type_id,name,version,timestamp_ms,dump) VALUES('json_dumps',?, '', ?, 0, ?)",rusqlite::params![u32::from(hydrus_legacy::serialisable::SerialisableType::CLIENT_OPTIONS.0), version, info])?;Ok(())}).unwrap();
        let legacy = store.read(load).unwrap();
        assert!(!legacy.new_renderer);
        assert!(!legacy.blurhash);
        assert!(legacy.fade);
        assert!(legacy.background.is_some());
        let native = Preferences {
            background: Some("native.png".into()),
            blurhash: true,
            ..Preferences::default()
        };
        let expected = native.clone();
        store
            .write(move |ctx| settings::set(ctx.conn(), &native))
            .unwrap();
        assert_eq!(store.read(load).unwrap(), expected);
        assert_eq!(
            crate::Store::open(directory.path())
                .unwrap()
                .read(load)
                .unwrap(),
            expected
        );
        store
            .write(|ctx| {
                ctx.conn().execute(
                    "UPDATE settings SET value='invalid' WHERE key='thumbnail_appearance'",
                    [],
                )?;
                Ok(())
            })
            .unwrap();
        assert!(store.read(load).is_err());
    }
}
