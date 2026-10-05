//! Database > db maintenance > review vacuum data (`GetVacuumData`,
//! `CheckCanVacuumIntoData`, `_Vacuum`): the database file's page counts,
//! whether there is room to vacuum it, and the vacuum itself.
//!
//! The reference vacuums into a copy with its connections closed, then
//! swaps the copy in. hydrus-rs pauses every connection of the store and
//! runs SQLite's `VACUUM` on a connection of its own, which rewrites the
//! file in place to the same effect.

use std::path::{Path, PathBuf};

use crate::store::DB_FILE_NAME;
use crate::{Result, Store};

/// The database's one file, as the reference names its files.
pub const NAME: &str = "main";

/// One database file's vacuum facts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VacuumData {
    pub name: String,
    pub path: PathBuf,
    pub page_size: u64,
    pub page_count: u64,
    pub freelist_count: u64,
    pub last_vacuumed_ms: Option<i64>,
}

impl VacuumData {
    pub fn total_size(&self) -> u64 {
        self.page_size * self.page_count
    }

    pub fn free_size(&self) -> u64 {
        self.page_size * self.freelist_count
    }

    /// What vacuuming would leave (`db_size`).
    pub fn used_size(&self) -> u64 {
        self.page_size * self.page_count.saturating_sub(self.freelist_count)
    }
}

/// When each file was last vacuumed (`vacuum_timestamps`), by name.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct VacuumTimes(pub std::collections::BTreeMap<String, i64>);

impl crate::settings::Setting for VacuumTimes {
    const KEY: &'static str = "vacuum_timestamps";
}

/// The store's vacuum facts (`GetVacuumData`).
pub fn data(store: &Store) -> Result<Vec<VacuumData>> {
    let path = store.dir().join(DB_FILE_NAME);
    store.read(|conn| {
        let pragma = |name: &str| -> Result<u64> {
            let value: i64 = conn.query_row(&format!("PRAGMA {name}"), [], |r| r.get(0))?;
            Ok(u64::try_from(value).unwrap_or(0))
        };
        let times: VacuumTimes = crate::settings::get(conn)?;
        Ok(vec![VacuumData {
            name: NAME.to_owned(),
            path: path.clone(),
            page_size: pragma("page_size")?,
            page_count: pragma("page_count")?,
            freelist_count: pragma("freelist_count")?,
            last_vacuumed_ms: times.0.get(NAME).copied(),
        }])
    })
}

/// Whether `data` can be vacuumed with `free` bytes free on its disk
/// (`CheckHasSpaceForDBTransaction`, needing no temporary space): the
/// reason if not.
pub fn check(data: &VacuumData, free: Option<u64>) -> std::result::Result<(), String> {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let needed = (data.used_size() as f64 * 1.1) as u64;
    match free {
        Some(free) if free < needed => Err(format!(
            "I believe you need about {} on your db's disk partition (perhaps only temporarily), but you only seem to have {}.",
            hydrus_core::numbers::human_bytes(needed),
            hydrus_core::numbers::human_bytes(free)
        )),
        _ => Ok(()),
    }
}

/// The free space on the disk holding `path`.
pub fn free_space(path: &Path) -> Option<u64> {
    fs4::available_space(path.parent().unwrap_or(path)).ok()
}

/// How long vacuuming `size` bytes takes, in seconds
/// (`GetApproxVacuumIntoDuration`): 10MB/s over 1.2 times the size.
pub fn approx_duration(size: u64) -> u64 {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let estimate = (size as f64 * 1.2) as u64;
    estimate / (1_048_576 * 10)
}

/// Vacuum the store's database, every other connection paused, and
/// record when (`_Vacuum`, `RegisterSuccessfulVacuum`).
pub fn vacuum(store: &Store, now_ms: i64) -> Result<()> {
    let path = store.dir().join(DB_FILE_NAME);
    {
        let _paused = store.pause()?;
        let conn = rusqlite::Connection::open(&path)?;
        conn.busy_timeout(std::time::Duration::from_secs(30))?;
        conn.execute_batch("VACUUM; PRAGMA wal_checkpoint(TRUNCATE);")?;
    }
    store.write(move |ctx| {
        let mut times: VacuumTimes = crate::settings::get(ctx.conn())?;
        times.0.insert(NAME.to_owned(), now_ms);
        crate::settings::set(ctx.conn(), &times)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vacuuming_frees_pages_and_is_recorded() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        store
            .write(|ctx| {
                ctx.conn().execute_batch(
                    "CREATE TABLE bloat (x BLOB); INSERT INTO bloat SELECT zeroblob(100000) FROM (WITH RECURSIVE c(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM c WHERE n < 50) SELECT n FROM c); DROP TABLE bloat;",
                )?;
                Ok(())
            })
            .unwrap();
        let before = data(&store).unwrap().remove(0);
        assert!(before.freelist_count > 0);
        assert_eq!(before.last_vacuumed_ms, None);
        vacuum(&store, 1_234).unwrap();
        let after = data(&store).unwrap().remove(0);
        assert!(after.page_count < before.page_count);
        assert_eq!(after.last_vacuumed_ms, Some(1_234));
        // the store keeps working
        assert!(
            store
                .read(|c| Ok(c.query_row("SELECT 1", [], |r| r.get::<_, i64>(0))?))
                .is_ok()
        );
    }

    #[test]
    fn room_is_checked_as_the_reference_checks() {
        let data = VacuumData {
            name: NAME.into(),
            path: PathBuf::new(),
            page_size: 4096,
            page_count: 1000,
            freelist_count: 0,
            last_vacuumed_ms: None,
        };
        assert_eq!(check(&data, None), Ok(()));
        assert_eq!(check(&data, Some(10_000_000)), Ok(()));
        assert!(
            check(&data, Some(1_000))
                .unwrap_err()
                .starts_with("I believe you need about ")
        );
        assert_eq!(approx_duration(100 * 1_048_576), 12);
    }
}
