//! The store: the database plus the in-memory state derived from it.
//!
//! Readers get an immutable [`Snapshot`] of services and display graphs.
//! Writes that change them publish a new snapshot after they commit (via
//! [`crate::WriteCtx::after_commit`]), so a snapshot never reflects
//! uncommitted state.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use arc_swap::ArcSwap;
use rusqlite::Connection;

use crate::conn::{Db, WriteCtx};
use crate::display::DisplayGraphs;
use crate::error::Result;
use crate::services::{self, ServiceRegistry};
use crate::settings;
use crate::storage::FileStorage;
use hydrus_core::thumbnail::ThumbnailSettings;

/// Name of the database file inside a store directory.
pub const DB_FILE_NAME: &str = "hydrus.db";

/// Immutable view of the in-memory state.
#[derive(Debug, Default)]
pub struct Snapshot {
    pub services: ServiceRegistry,
    pub display: DisplayGraphs,
    pub storage: FileStorage,
    pub thumbnails: ThumbnailSettings,
}

impl Snapshot {
    pub fn load(conn: &Connection) -> Result<Self> {
        let services = ServiceRegistry::load(conn)?;
        let display = DisplayGraphs::load(conn, &services)?;
        let storage = FileStorage::load(conn)?;
        let thumbnails = settings::get(conn)?;
        Ok(Self {
            services,
            display,
            storage,
            thumbnails,
        })
    }
}

/// A hydrus-rs database directory, open.
#[derive(Debug)]
pub struct Store {
    dir: PathBuf,
    db: Db,
    snapshot: Arc<ArcSwap<Snapshot>>,
}

impl Store {
    /// Open the store in `dir`, creating a new, empty client (with the
    /// default services) if there is none.
    pub fn open(dir: &Path) -> Result<Arc<Self>> {
        std::fs::create_dir_all(dir)?;
        let db = Db::open(&dir.join(DB_FILE_NAME), reader_count())?;
        let empty: bool = db.read(|c| {
            Ok(
                c.query_row("SELECT NOT EXISTS (SELECT 1 FROM services)", [], |r| {
                    r.get(0)
                })?,
            )
        })?;
        if empty {
            db.write(|ctx| {
                for (key, name, kind) in services::default_services() {
                    services::insert(ctx.conn(), &key, &name, &kind)?;
                }
                Ok(())
            })?;
        }
        let snapshot = db.read(Snapshot::load)?;
        Ok(Arc::new(Self {
            dir: dir.to_path_buf(),
            db,
            snapshot: Arc::new(ArcSwap::from_pointee(snapshot)),
        }))
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The current in-memory state.
    pub fn snapshot(&self) -> Arc<Snapshot> {
        self.snapshot.load_full()
    }

    /// Run a read against a consistent database snapshot.
    pub fn read<R>(&self, f: impl FnOnce(&Connection) -> Result<R>) -> Result<R> {
        self.db.read(f)
    }

    /// Run a write; see [`Db::write`].
    pub fn write<R: Send + 'static>(
        &self,
        f: impl FnOnce(&mut WriteCtx<'_>) -> Result<R> + Send + 'static,
    ) -> Result<R> {
        self.db.write(f)
    }

    /// Run a write that changes services or tag relations, republishing the
    /// in-memory snapshot once it commits.
    pub fn write_and_refresh<R: Send + 'static>(
        &self,
        f: impl FnOnce(&mut WriteCtx<'_>) -> Result<R> + Send + 'static,
    ) -> Result<R> {
        let snapshot = Arc::clone(&self.snapshot);
        self.db.write(move |ctx| {
            let result = f(ctx)?;
            let fresh = Snapshot::load(ctx.conn())?;
            ctx.after_commit(move || snapshot.store(Arc::new(fresh)));
            Ok(result)
        })
    }
}

fn reader_count() -> usize {
    std::thread::available_parallelism().map_or(4, |n| n.get().clamp(2, 16))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_store_has_default_services() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        assert_eq!(store.snapshot().services.all().count(), 13);
        drop(store);
        // reopening doesn't duplicate them
        let store = Store::open(dir.path()).unwrap();
        assert_eq!(store.snapshot().services.all().count(), 13);
    }

    #[test]
    fn snapshots_refresh_only_after_commit() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let key = hydrus_core::ServiceKey::new(vec![1; 32]);
        let k = key.clone();
        store
            .write_and_refresh(move |ctx| {
                services::insert(
                    ctx.conn(),
                    &k,
                    "extra tags",
                    &services::ServiceKind::LocalTags,
                )?;
                Ok(())
            })
            .unwrap();
        assert_eq!(
            store.snapshot().services.by_key(&key).unwrap().name,
            "extra tags"
        );
        let failed = store.write_and_refresh(|ctx| {
            services::insert(
                ctx.conn(),
                &hydrus_core::ServiceKey::new(vec![2; 32]),
                "doomed",
                &services::ServiceKind::LocalTags,
            )?;
            Err::<(), _>(crate::StoreError::Invalid("no".into()))
        });
        assert!(failed.is_err());
        assert!(store.snapshot().services.by_name("doomed").is_none());
    }
}
