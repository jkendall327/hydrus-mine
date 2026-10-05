//! Saved ffmpeg call deadline. Providers capture it separately for each process.
use crate::{
    Result, StoreError,
    settings::{self, Setting},
};
use hydrus_legacy::{
    objects::ClientOptions,
    serialisable::{SerialisableObject, SerialisableType},
};
use rusqlite::Connection;

/// Raw imported seconds survive until Qt-style Options Apply clamps 1..600.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct FfmpegPolicy {
    pub seconds: i64,
}
impl Default for FfmpegPolicy {
    fn default() -> Self {
        Self { seconds: 15 }
    }
}
impl Setting for FfmpegPolicy {
    const KEY: &'static str = "ffmpeg_policy";
}
impl FfmpegPolicy {
    pub fn from_legacy(options: &ClientOptions) -> Self {
        Self {
            seconds: options
                .integers
                .get("ffmpeg_subprocess_timeout")
                .copied()
                .unwrap_or(15),
        }
    }
    /// Match the reference's three-second communicate/check slices. The
    /// raw/displayed integer is unchanged; malformed imports are bounded.
    pub fn timeout(self) -> std::time::Duration {
        std::time::Duration::from_secs((self.seconds.clamp(1, 600) as u64).div_ceil(3) * 3)
    }
}
/// An explicit native policy takes precedence over the retained reference tuple.
pub fn load(conn: &Connection) -> Result<FfmpegPolicy> {
    let native: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM settings WHERE key=?)",
        [FfmpegPolicy::KEY],
        |row| row.get(0),
    )?;
    if native {
        return settings::get(conn);
    }
    let kind = SerialisableType::CLIENT_OPTIONS;
    let Some((version, info)) = crate::legacy::singleton(conn, u32::from(kind.0))? else {
        return Ok(FfmpegPolicy::default());
    };
    let object = SerialisableObject::from_stored(kind, None, version, &info)
        .map_err(|e| StoreError::Corrupt(e.to_string()))?;
    let options =
        ClientOptions::from_object(&object).map_err(|e| StoreError::Corrupt(e.to_string()))?;
    Ok(FfmpegPolicy::from_legacy(&options))
}

/// Read the current deadline per call without retaining its Store or connections.
/// A retired/unreadable Store supplies the ordinary fifteen-second default.
pub fn reader(
    store: &crate::Store,
) -> std::sync::Arc<dyn Fn() -> std::time::Duration + Send + Sync> {
    let weak = store.downgrade();
    std::sync::Arc::new(move || {
        weak.upgrade()
            .and_then(|store| store.read(load).ok())
            .unwrap_or_default()
            .timeout()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reader_from_a_borrowed_store_is_live_and_does_not_retain_its_owner() {
        let dir = tempfile::tempdir().unwrap();
        let store = crate::Store::open(dir.path()).unwrap();
        let weak = store.downgrade();
        let count = std::sync::Arc::strong_count(&store);
        let reader = reader(&store);
        assert_eq!(std::sync::Arc::strong_count(&store), count);
        store
            .write(|c| settings::set(c.conn(), &FfmpegPolicy { seconds: 1 }))
            .unwrap();
        assert_eq!(reader(), std::time::Duration::from_secs(3));
        store
            .write(|c| settings::set(c.conn(), &FfmpegPolicy { seconds: 4 }))
            .unwrap();
        assert_eq!(reader(), std::time::Duration::from_secs(6));
        drop(store);
        assert!(weak.upgrade().is_none());
        assert_eq!(reader(), std::time::Duration::from_secs(15));
    }
    #[test]
    fn actual_legacy_saved_policy_backfill_native_override_and_poll_adapter() {
        let fixture = hydrus_testkit::fixture_json("ffmpeg_timeout.json");
        let tuple = &fixture["legacy"];
        let object = SerialisableObject::from_tuple_str(&tuple.to_string()).unwrap();
        let mut options = ClientOptions::from_object(&object).unwrap();
        assert_eq!(FfmpegPolicy::from_legacy(&options).seconds, 1);
        options.integers.remove("ffmpeg_subprocess_timeout");
        assert_eq!(FfmpegPolicy::from_legacy(&options), FfmpegPolicy::default());
        assert_eq!(
            serde_json::from_str::<FfmpegPolicy>("{}").unwrap(),
            FfmpegPolicy::default()
        );
        let dir = tempfile::tempdir().unwrap();
        let store = crate::Store::open(dir.path()).unwrap();
        assert_eq!(store.read(load).unwrap(), FfmpegPolicy::default());
        let info = tuple[2].to_string();
        let version = tuple[1].as_i64().unwrap();
        store.write(move|c|{c.conn().execute("INSERT INTO legacy_objects(source,type_id,name,version,timestamp_ms,dump) VALUES('json_dumps',?,'',?,0,?)",rusqlite::params![u32::from(SerialisableType::CLIENT_OPTIONS.0),version,info])?;Ok(())}).unwrap();
        assert_eq!(store.read(load).unwrap().seconds, 1);
        store
            .write(|c| settings::set(c.conn(), &FfmpegPolicy { seconds: 5 }))
            .unwrap();
        assert_eq!(
            crate::Store::open(store.dir())
                .unwrap()
                .read(load)
                .unwrap()
                .seconds,
            5
        );
        for (seconds, effective) in [
            (0, 3),
            (1, 3),
            (2, 3),
            (3, 3),
            (4, 6),
            (5, 6),
            (6, 6),
            (15, 15),
            (600, 600),
            (601, 600),
        ] {
            assert_eq!(
                FfmpegPolicy { seconds }.timeout(),
                std::time::Duration::from_secs(effective)
            );
        }
        store
            .write(|c| {
                c.conn().execute(
                    "UPDATE settings SET value='broken' WHERE key=?",
                    [FfmpegPolicy::KEY],
                )?;
                Ok(())
            })
            .unwrap();
        assert!(store.read(load).is_err());
    }
}
