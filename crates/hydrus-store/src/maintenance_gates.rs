//! Admission of automatic trash/physical maintenance during ordinary activity.
use crate::{
    Result, StoreError,
    settings::{self, Setting},
};
use hydrus_legacy::{
    objects::ClientOptions,
    serialisable::{SerialisableObject, SerialisableType},
};
use rusqlite::Connection;

/// The two independent automatic-worker gates; explicit user commands bypass them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub trash_normal: bool,
    pub deferred_normal: bool,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            trash_normal: true,
            deferred_normal: true,
        }
    }
}
impl Setting for Preferences {
    const KEY: &'static str = "maintenance_gates";
}
/// Which real worker is seeking one pass admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Worker {
    Trash,
    Deferred,
}
impl Preferences {
    /// Capture retained ClientOptions booleans without changing their defaults.
    pub fn from_legacy(options: &ClientOptions) -> Self {
        Self {
            trash_normal: options
                .booleans
                .get("maintain_trash_in_normal_time")
                .copied()
                .unwrap_or(true),
            deferred_normal: options
                .booleans
                .get("deferred_file_deletes_in_normal_time")
                .copied()
                .unwrap_or(true),
        }
    }
    /// Qt admits a pass once when idle, or when its normal-time gate is checked.
    pub fn allows(self, worker: Worker, idle: bool) -> bool {
        idle || match worker {
            Worker::Trash => self.trash_normal,
            Worker::Deferred => self.deferred_normal,
        }
    }
    /// Edit only the gate changed by this Options draft, preserving its live peer.
    pub fn save_changed(&self, conn: &Connection, before: &Self) -> Result<()> {
        if self == before {
            return Ok(());
        }
        let mut latest = load(conn)?;
        if self.trash_normal != before.trash_normal {
            latest.trash_normal = self.trash_normal;
        }
        if self.deferred_normal != before.deferred_normal {
            latest.deferred_normal = self.deferred_normal;
        }
        settings::set(conn, &latest)
    }
}
/// Native preferences win; older imported stores retain the original saved gates.
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
    fn retained_client_options_native_wins_reopen_and_corruption() {
        let fixture = hydrus_testkit::fixture_json("normal_time_maintenance.json");
        let tuple = &fixture["legacy_options"];
        let directory = tempfile::tempdir().unwrap();
        let store = crate::Store::open(directory.path()).unwrap();
        let version = i64::try_from(tuple[1].as_u64().unwrap()).unwrap();
        let info = tuple[2].to_string();
        store.write(move |ctx| {
            ctx.conn().execute("INSERT INTO legacy_objects(source,type_id,name,version,timestamp_ms,dump) VALUES('json_dumps',?,'',?,0,?)",rusqlite::params![u32::from(SerialisableType::CLIENT_OPTIONS.0),version,info])?;Ok(())
        }).unwrap();
        assert_eq!(
            store.read(load).unwrap(),
            Preferences {
                trash_normal: false,
                deferred_normal: false
            }
        );
        store
            .write(|ctx| {
                settings::set(
                    ctx.conn(),
                    &Preferences {
                        trash_normal: true,
                        deferred_normal: false,
                    },
                )
            })
            .unwrap();
        assert_eq!(
            crate::Store::open(directory.path())
                .unwrap()
                .read(load)
                .unwrap(),
            Preferences {
                trash_normal: true,
                deferred_normal: false
            }
        );
        store
            .write(|ctx| {
                ctx.conn().execute(
                    "UPDATE settings SET value='{\"trash_normal\":\"invalid\"}' WHERE key=?",
                    [Preferences::KEY],
                )?;
                Ok(())
            })
            .unwrap();
        assert!(store.read(load).is_err());
    }
}
