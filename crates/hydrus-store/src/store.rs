//! The store: the database plus the in-memory state derived from it.
//!
//! Readers get an immutable [`Snapshot`] of services and display graphs.
//! Writes that change them publish a new snapshot after they commit (via
//! [`crate::WriteCtx::after_commit`]), so a snapshot never reflects
//! uncommitted state.

use std::path::{Path, PathBuf};
use std::sync::{Arc, atomic::AtomicBool};

use arc_swap::ArcSwap;
use rusqlite::Connection;

use crate::conn::{Db, WriteCtx};
use crate::content::ContentWriter;
use crate::display::DisplayGraphs;
use crate::domains::DomainCache;
use crate::duplicates::cache::PairCache;
use crate::error::Result;
use crate::services::{self, ServiceRegistry};
use crate::settings;
use crate::storage::FileStorage;
use hydrus_core::thumbnail::ThumbnailSettings;
use hydrus_core::url::{UrlClassSettings, UrlClasses};

/// Name of the database file inside a store directory.
pub const DB_FILE_NAME: &str = "hydrus.db";

/// The lock file `hydrus serve` holds in a store directory while it runs
/// (and the commands doing its work hold while they do it).
pub const SERVE_LOCK_FILE: &str = "serve.lock";

/// The lock file the desktop client holds in a store directory while it is
/// open.
pub const GUI_LOCK_FILE: &str = "gui.lock";

/// Changes to snapshot-backed state, shared between independently opened stores.
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct SnapshotRevision(u64);

impl settings::Setting for SnapshotRevision {
    const KEY: &'static str = "store_snapshot_revision";
}

/// Take the lock in `name`, in the store directory `dir`: `None` if another
/// process holds it. It is let go when the file returned is dropped (or the
/// process ends, however it ends).
fn lock(dir: &Path, name: &str) -> std::io::Result<Option<std::fs::File>> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join(name))?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(std::fs::TryLockError::Error(e)) => Err(e),
    }
}

/// Take the lock `hydrus serve` holds while it runs on the store in `dir`
/// (see [`lock`]).
pub fn lock_serving(dir: &Path) -> std::io::Result<Option<std::fs::File>> {
    lock(dir, SERVE_LOCK_FILE)
}

/// Take the lock the desktop client holds while it is open on the store in
/// `dir` (see [`lock`]).
pub fn lock_gui(dir: &Path) -> std::io::Result<Option<std::fs::File>> {
    lock(dir, GUI_LOCK_FILE)
}

/// Whether the desktop client is open on the store in `dir`.
pub fn gui_open(dir: &Path) -> bool {
    // (the lock is let go at once, if it was free)
    matches!(lock_gui(dir), Ok(None))
}

/// Immutable view of the in-memory state.
#[derive(Debug, Default)]
pub struct Snapshot {
    /// Revision of services, display graphs and other snapshot-backed settings.
    pub revision: u64,
    pub services: ServiceRegistry,
    pub display: DisplayGraphs,
    pub storage: FileStorage,
    pub thumbnails: ThumbnailSettings,
    /// The client's URL classes, ready for matching.
    pub url_classes: UrlClasses,
    /// The files of each file domain, shared by every snapshot of a store.
    pub domains: Arc<DomainCache>,
    /// The potential duplicate pairs, shared by every snapshot of a store.
    pub duplicates: Arc<PairCache>,
}

impl Snapshot {
    pub fn load(conn: &Connection) -> Result<Self> {
        let services = ServiceRegistry::load(conn)?;
        let display = DisplayGraphs::load(conn, &services)?;
        let storage = FileStorage::load(conn)?;
        let thumbnails = settings::get(conn)?;
        let url_classes = UrlClasses::new(settings::get::<UrlClassSettings>(conn)?);
        Ok(Self {
            revision: settings::get::<SnapshotRevision>(conn)?.0,
            services,
            display,
            storage,
            thumbnails,
            url_classes,
            domains: Arc::default(),
            duplicates: Arc::default(),
        })
    }

    fn reload(conn: &Connection, old: &Self) -> Result<Self> {
        let mut fresh = Self::load(conn)?;
        fresh.domains = Arc::clone(&old.domains);
        fresh.duplicates = Arc::clone(&old.duplicates);
        Ok(fresh)
    }
}

/// Files whose media someone is writing or deleting on disk right now.
///
/// An importer claims a file before writing it into storage and holds the
/// claim until the database knows about it; the purge job only deletes
/// unclaimed files. So a file being re-imported is never purged under it.
#[derive(Debug, Default, Clone)]
pub struct MediaClaims(Arc<parking_lot::Mutex<std::collections::HashSet<hydrus_core::Sha256>>>);

/// Held while a file's media is being written or deleted.
#[derive(Debug)]
pub struct MediaClaim {
    claims: MediaClaims,
    hash: hydrus_core::Sha256,
}

impl MediaClaims {
    /// Claim `hash`, or `None` if someone else holds it.
    pub fn try_claim(&self, hash: hydrus_core::Sha256) -> Option<MediaClaim> {
        self.0.lock().insert(hash).then(|| MediaClaim {
            claims: self.clone(),
            hash,
        })
    }

    /// Claim `hash`, waiting for any other holder to finish.
    pub fn claim(&self, hash: hydrus_core::Sha256) -> MediaClaim {
        loop {
            if let Some(claim) = self.try_claim(hash) {
                return claim;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}

impl Drop for MediaClaim {
    fn drop(&mut self) {
        self.claims.0.lock().remove(&self.hash);
    }
}

/// A hydrus-rs database directory, open.
#[derive(Debug)]
pub struct Store {
    dir: PathBuf,
    db: Db,
    snapshot: Arc<ArcSwap<Snapshot>>,
    claims: MediaClaims,
    migration_active: Arc<AtomicBool>,
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
            let media = dir.join("client_files");
            db.write(move |ctx| {
                for (key, name, kind) in services::default_services() {
                    services::insert(ctx.conn(), &key, &name, &kind)?;
                }
                crate::network::create_defaults(ctx.conn())?;
                create_default_storage(ctx.conn(), &media)
            })?;
        }
        let snapshot = db.read(Snapshot::load)?;
        Ok(Arc::new(Self {
            dir: dir.to_path_buf(),
            db,
            snapshot: Arc::new(ArcSwap::from_pointee(snapshot)),
            claims: MediaClaims::default(),
            migration_active: Arc::new(AtomicBool::new(false)),
        }))
    }

    /// Reserve one migration while leaving at least one pooled reader for the UI.
    pub(crate) fn claim_tag_migration(&self) -> Result<crate::tag_migration::Guard> {
        crate::tag_migration::Guard::claim(self.migration_active.clone())
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Claims on files whose media is being written or deleted.
    pub fn media_claims(&self) -> MediaClaims {
        self.claims.clone()
    }

    /// The current in-memory state.
    pub fn snapshot(&self) -> Arc<Snapshot> {
        self.snapshot.load_full()
    }

    /// Stop all database work until the guard is dropped, with the
    /// database file complete (see [`Db::pause`]).
    pub fn pause(&self) -> Result<crate::Paused> {
        self.db.pause()
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

    /// Load the in-memory snapshot again (another process changed what it
    /// holds: the thumbnail settings, say).
    pub fn refresh(&self) -> Result<()> {
        self.change_and_refresh(|_| Ok(()), false)
    }

    /// Notice another store's committed snapshot changes. Plain thumbnail
    /// settings writes are also supported, as the daemon previously watched them.
    /// Refreshing never advances the revision, so stores cannot wake each other
    /// indefinitely by merely reloading their snapshots.
    pub fn refresh_if_changed(&self) -> Result<bool> {
        let old = self.snapshot();
        let changed = self.read(|conn| {
            Ok(settings::get::<SnapshotRevision>(conn)?.0 != old.revision
                || settings::get::<ThumbnailSettings>(conn)? != old.thumbnails)
        })?;
        if changed {
            self.refresh()?;
        }
        Ok(changed)
    }

    /// Run a write that changes services or tag relations, republishing the
    /// in-memory snapshot once it commits. It commits alone, so every write
    /// queued after it sees the new snapshot.
    pub fn write_and_refresh<R: Send + 'static>(
        &self,
        f: impl FnOnce(&mut WriteCtx<'_>) -> Result<R> + Send + 'static,
    ) -> Result<R> {
        self.change_and_refresh(f, true)
    }

    fn change_and_refresh<R: Send + 'static>(
        &self,
        f: impl FnOnce(&mut WriteCtx<'_>) -> Result<R> + Send + 'static,
        changed: bool,
    ) -> Result<R> {
        let snapshot = Arc::clone(&self.snapshot);
        self.db.write_alone(move |ctx| {
            let result = f(ctx)?;
            if changed {
                let revision = settings::get::<SnapshotRevision>(ctx.conn())?
                    .0
                    .checked_add(1)
                    .ok_or_else(|| {
                        crate::StoreError::Invalid("snapshot revision overflow".into())
                    })?;
                settings::set(ctx.conn(), &SnapshotRevision(revision))?;
            }
            // a service's files go with it, and its id may be used again
            crate::domains::changed(ctx.conn())?;
            let old = snapshot.load();
            let fresh = Snapshot::reload(ctx.conn(), &old)?;
            ctx.after_commit(move || snapshot.store(Arc::new(fresh)));
            Ok(result)
        })
    }
}

impl Store {
    /// Run content changes in one write: `f` gets a [`ContentWriter`] over
    /// the current snapshot, and derived data is flushed before commit.
    pub fn write_content<R: Send + 'static>(
        &self,
        f: impl FnOnce(&mut ContentWriter<'_>) -> Result<R> + Send + 'static,
    ) -> Result<R> {
        let snapshot = Arc::clone(&self.snapshot);
        self.db.write(move |ctx| {
            // loaded on the writer thread: it reflects every committed write
            let old = snapshot.load_full();
            let snap = if settings::get::<SnapshotRevision>(ctx.conn())?.0 == old.revision {
                old
            } else {
                // Another process may have edited relations since our poll.
                // Use its graph for this write's derived counts immediately.
                let fresh = Arc::new(Snapshot::reload(ctx.conn(), &old)?);
                let committed = Arc::clone(&fresh);
                ctx.after_commit(move || snapshot.store(committed));
                fresh
            };
            let mut writer = ContentWriter::new(
                ctx.conn(),
                &snap,
                hydrus_core::time::TimestampMs::now().millis(),
            )?;
            let result = f(&mut writer)?;
            writer.finish()?;
            Ok(result)
        })
    }
}

/// A new store keeps its media in one location, `client_files`, with the
/// reference's default layout (256 file and 256 thumbnail subfolders).
fn create_default_storage(conn: &Connection, media: &Path) -> Result<()> {
    conn.execute(
        "INSERT INTO storage_locations (location_id, path, ideal_weight, max_bytes, is_thumbnail_override)
         VALUES (1, ?1, 1, NULL, 0)",
        [media.to_string_lossy()],
    )?;
    let mut stmt =
        conn.prepare("INSERT INTO storage_subfolders (prefix, location_id) VALUES (?1, 1)")?;
    for kind in ['f', 't'] {
        for byte in 0..=255u8 {
            stmt.execute([format!("{kind}{byte:02x}")])?;
        }
    }
    Ok(())
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

    #[test]
    fn independent_stores_notice_committed_snapshot_changes_without_refresh_loops() {
        let dir = tempfile::tempdir().unwrap();
        let editor = Store::open(dir.path()).unwrap();
        let daemon = Store::open(dir.path()).unwrap();
        let old = daemon.snapshot();
        assert!(!daemon.refresh_if_changed().unwrap());
        let classes = UrlClassSettings {
            url_classes: vec![hydrus_core::url::UrlClass {
                name: "edited class".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let saved = classes.clone();
        editor
            .write_and_refresh(move |ctx| {
                services::insert(
                    ctx.conn(),
                    &hydrus_core::ServiceKey::new(vec![3; 32]),
                    "new tag service",
                    &services::ServiceKind::LocalTags,
                )?;
                settings::set(ctx.conn(), &saved)
            })
            .unwrap();
        assert!(
            daemon
                .snapshot()
                .services
                .by_name("new tag service")
                .is_none()
        );
        assert!(daemon.refresh_if_changed().unwrap());
        let fresh = daemon.snapshot();
        assert!(fresh.services.by_name("new tag service").is_some());
        assert_eq!(fresh.url_classes.settings(), &classes);
        assert!(Arc::ptr_eq(&old.domains, &fresh.domains));
        assert!(Arc::ptr_eq(&old.duplicates, &fresh.duplicates));
        assert!(old.services.by_name("new tag service").is_none());
        assert!(!daemon.refresh_if_changed().unwrap());
        assert!(!editor.refresh_if_changed().unwrap());
        editor.refresh().unwrap();
        assert!(!daemon.refresh_if_changed().unwrap());

        let revision = fresh.revision;
        let failed = editor.write_and_refresh(|ctx| {
            services::insert(
                ctx.conn(),
                &hydrus_core::ServiceKey::new(vec![4; 32]),
                "rolled back service",
                &services::ServiceKind::LocalTags,
            )?;
            Err::<(), _>(crate::StoreError::Invalid("cancelled".into()))
        });
        assert!(failed.is_err());
        assert_eq!(editor.snapshot().revision, revision);
        assert!(!daemon.refresh_if_changed().unwrap());

        // Preserve the daemon's existing support for plain thumbnail writes.
        editor
            .write(|ctx| {
                let mut thumbnails: ThumbnailSettings = settings::get(ctx.conn())?;
                thumbnails.bounding_width += 1;
                settings::set(ctx.conn(), &thumbnails)
            })
            .unwrap();
        assert!(daemon.refresh_if_changed().unwrap());
        assert_eq!(daemon.snapshot().revision, revision);
        assert!(!daemon.refresh_if_changed().unwrap());
    }

    #[test]
    fn content_writes_use_foreign_relation_graphs_before_the_next_poll() {
        use crate::content::MappingAction;
        use hydrus_core::{Sha256, Tag};

        let dir = tempfile::tempdir().unwrap();
        let editor = Store::open(dir.path()).unwrap();
        let daemon = Store::open(dir.path()).unwrap();
        let old = daemon.snapshot();
        let service = old.services.by_name("my tags").unwrap().id;
        let all_files = crate::content::DomainRoles::new(&old.services)
            .unwrap()
            .all_known_files
            .unwrap();
        let (hash, bad, good) = editor
            .write(|ctx| {
                Ok((
                    crate::master::intern_hash(ctx.conn(), &Sha256([1; 32]))?,
                    crate::master::intern_tag(ctx.conn(), &Tag::new("bad").unwrap())?,
                    crate::master::intern_tag(ctx.conn(), &Tag::new("good").unwrap())?,
                ))
            })
            .unwrap();
        editor
            .write_and_refresh(move |ctx| {
                ctx.conn().execute(
                    "INSERT INTO tag_siblings (service_id, status, bad_tag_id, good_tag_id) VALUES (?1, 0, ?2, ?3)",
                    rusqlite::params![service, bad, good],
                )?;
                crate::counts::rebuild_all(ctx.conn())
            })
            .unwrap();
        let failed = daemon.write_content(move |writer| {
            assert_eq!(writer.snapshot().display.get(service).ideal(bad), good);
            writer.update_mappings(service, &MappingAction::Add, bad, &[hash])?;
            Err::<(), _>(crate::StoreError::Invalid("cancelled".into()))
        });
        assert!(failed.is_err());
        assert!(Arc::ptr_eq(&old, &daemon.snapshot()));
        daemon
            .write_content(move |writer| {
                assert_eq!(writer.snapshot().display.get(service).ideal(bad), good);
                writer.update_mappings(service, &MappingAction::Add, bad, &[hash])
            })
            .unwrap();
        let table = crate::schema::MappingTables::new(service);
        let counts: Vec<(hydrus_core::TagId, i64)> = daemon
            .read(|conn| {
                let mut stmt = conn.prepare(&format!(
                    "SELECT tag_id, current FROM {} WHERE domain_id=?1 ORDER BY tag_id",
                    table.display_counts
                ))?;
                Ok(stmt
                    .query_map([all_files], |row| Ok((row.get(0)?, row.get(1)?)))?
                    .collect::<std::result::Result<_, _>>()?)
            })
            .unwrap();
        assert_eq!(counts, vec![(good, 1)]);
        assert_eq!(old.display.get(service).ideal(bad), bad);
        assert_eq!(daemon.snapshot().display.get(service).ideal(bad), good);
        assert!(!daemon.refresh_if_changed().unwrap());
    }
}
