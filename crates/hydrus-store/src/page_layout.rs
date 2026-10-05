//! Global defaults for live page splitters; session content remains unchanged.
use crate::{
    Result, StoreError,
    settings::{self, Setting},
};
use hydrus_legacy::objects::{LegacyOptions, YamlValue};
use rusqlite::Connection;

/// The reference's signed sash positions and independent layout preferences.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct PageLayout {
    pub hpos: i64,
    pub vpos: i64,
    pub hide_preview: bool,
    pub save_on_exit: bool,
}
impl Default for PageLayout {
    fn default() -> Self {
        Self {
            hpos: 400,
            vpos: -240,
            hide_preview: false,
            save_on_exit: true,
        }
    }
}
impl Setting for PageLayout {
    const KEY: &'static str = "page_layout";
}
impl PageLayout {
    /// Import old YAML sizes alongside the separate ClientOptions exit switch.
    pub fn apply_legacy(
        &mut self,
        old: &LegacyOptions,
        booleans: &std::collections::BTreeMap<String, bool>,
    ) {
        if let Some(v) = old.get("hpos").and_then(YamlValue::as_i64) {
            self.hpos = v;
        }
        if let Some(v) = old.get("vpos").and_then(YamlValue::as_i64) {
            self.vpos = v;
        }
        if let Some(v) = old.get("hide_preview").and_then(YamlValue::as_bool) {
            self.hide_preview = v;
        }
        if let Some(&v) = booleans.get("saving_sash_positions_on_exit") {
            self.save_on_exit = v;
        }
    }
}
/// Native preferences override preserved YAML; old imported databases need no migration.
pub fn load(conn: &Connection) -> Result<PageLayout> {
    let saved: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM settings WHERE key = ?)",
        [PageLayout::KEY],
        |r| r.get(0),
    )?;
    if saved {
        return settings::get(conn);
    }
    let old = crate::legacy::old_options(conn)?;
    let old = LegacyOptions::parse(old.as_deref()).map_err(StoreError::Corrupt)?;
    let mut value = PageLayout::default();
    value.apply_legacy(&old, &std::collections::BTreeMap::new());
    // Exit saving was already imported into other settings only for older slices.
    // Retained ClientOptions is the authoritative fallback for this new field.
    let kind = hydrus_legacy::serialisable::SerialisableType::CLIENT_OPTIONS;
    if let Some((version, info)) = crate::legacy::singleton(conn, u32::from(kind.0))? {
        let object = hydrus_legacy::serialisable::SerialisableObject::from_stored(
            kind, None, version, &info,
        )
        .map_err(|e| StoreError::Corrupt(e.to_string()))?;
        let options = hydrus_legacy::objects::ClientOptions::from_object(&object)
            .map_err(|e| StoreError::Corrupt(e.to_string()))?;
        value.apply_legacy(&old, &options.booleans);
    }
    Ok(value)
}
/// Options changes only its hide preference, preserving concurrent saves and toggles.
pub fn save_changed(conn: &Connection, after: &PageLayout, before: &PageLayout) -> Result<()> {
    if after.hide_preview != before.hide_preview {
        let mut current = load(conn)?;
        current.hide_preview = after.hide_preview;
        settings::set(conn, &current)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn imported_yaml_signed_sizes_exit_switch_and_native_override_survive_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let store = crate::Store::open(dir.path()).unwrap();
        store.write(|ctx|{ctx.conn().execute("INSERT INTO legacy_objects(source,type_id,name,version,timestamp_ms,dump) VALUES('options',0,'',0,0,?)",["hpos: -950\nvpos: 580\nhide_preview: true\n"])?;Ok(())}).unwrap();
        let mut expected = PageLayout {
            hpos: -950,
            vpos: 580,
            hide_preview: true,
            ..Default::default()
        };
        assert_eq!(store.read(load).unwrap(), expected);
        let old =
            LegacyOptions::parse(Some("hpos: -950\nvpos: 580\nhide_preview: true\n")).unwrap();
        expected.apply_legacy(
            &old,
            &std::collections::BTreeMap::from([("saving_sash_positions_on_exit".into(), false)]),
        );
        assert!(!expected.save_on_exit);
        assert_eq!(
            serde_json::from_str::<PageLayout>("{}").unwrap(),
            PageLayout::default()
        );
        let native = PageLayout {
            hpos: 0,
            vpos: -375,
            hide_preview: false,
            save_on_exit: false,
        };
        let save = native.clone();
        store
            .write(move |ctx| settings::set(ctx.conn(), &save))
            .unwrap();
        drop(store);
        let reopened = crate::Store::open(dir.path()).unwrap();
        assert_eq!(reopened.read(load).unwrap(), native);
        reopened
            .write(|ctx| {
                ctx.conn().execute(
                    "UPDATE settings SET value='bad json' WHERE key=?",
                    [PageLayout::KEY],
                )?;
                Ok(())
            })
            .unwrap();
        assert!(
            reopened.read(load).is_err(),
            "corrupt native setting cannot silently revert to legacy"
        );
    }
}
