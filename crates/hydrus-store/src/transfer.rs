//! Bringing an imported install's media files under the new store.
//!
//! After the database import, the native store's storage locations still
//! point into the reference install's `client_files`. A transfer gives the
//! new store its own copy of the tree, laid out identically (ADR-2):
//!
//! - **hardlink** (the default): new directory entries for the same files.
//!   No extra space, instant, and the two installs are independent: deleting
//!   a file in one leaves the other's link.
//! - **copy**: an independent copy. Needs the space.
//! - **move**: takes the files away from the reference install, which is
//!   left without its media. Fast on one filesystem.
//!
//! Each filesystem's files stay on it: storage locations on the media
//! directory's filesystem come into it, and those on another drive into a
//! new `<location>-hydrus-rs` directory beside the first location there,
//! each becoming a storage location of the store with the weights and
//! limits of those it came from.
//! - **in place**: keep using the reference install's files. The store then
//!   never deletes media from disk, since the reference install still needs
//!   them.
//!
//! Only media the database says is stored comes across (with its
//! thumbnail): files the reference was about to delete for good, and
//! anything else in its folders, are left where they are.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::{Result, StoreError};
use crate::settings::{self, Setting};

/// How to bring media files across.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransferMode {
    #[default]
    Hardlink,
    Copy,
    Move,
    InPlace,
}

/// Who owns the media files on disk.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaOwnership {
    /// Set when the files belong to another install (an in-place import):
    /// the store must never delete them.
    pub shared_with: Option<PathBuf>,
}

impl Setting for MediaOwnership {
    const KEY: &'static str = "media_ownership";
}

/// What a transfer did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TransferReport {
    pub files: u64,
    pub bytes: u64,
    /// Files left behind: media no longer stored (the reference had it
    /// waiting to be deleted) and anything else in its folders.
    pub skipped: u64,
    /// The store's media directories afterwards: the media directory asked
    /// for, then one for each other drive the media was on.
    pub destinations: Vec<PathBuf>,
}

/// One storage subfolder (e.g. `f3a`, or `f3a/b` at granularity 3).
#[derive(Debug, Clone)]
struct Subfolder {
    prefix: String,
    /// Relative to the location, e.g. `f3a/b`.
    relative: PathBuf,
    location_id: i64,
    location: PathBuf,
    /// The media directory it goes into.
    destination: PathBuf,
}

/// A storage location of the imported install.
#[derive(Debug, Clone)]
struct Location {
    id: i64,
    path: PathBuf,
    weight: Option<i64>,
    max_bytes: Option<i64>,
    thumbnail_override: bool,
}

fn locations(conn: &Connection) -> Result<Vec<Location>> {
    let mut stmt = conn.prepare(
        "SELECT location_id, path, ideal_weight, max_bytes, is_thumbnail_override
         FROM storage_locations ORDER BY location_id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Location {
            id: r.get(0)?,
            path: PathBuf::from(r.get::<_, String>(1)?),
            weight: r.get(2)?,
            max_bytes: r.get(3)?,
            thumbnail_override: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Which filesystem `path` is on (that of its nearest existing ancestor),
/// as far as the platform says; `None` when it can't tell.
fn filesystem(path: &Path) -> Option<u64> {
    let absolute = std::path::absolute(path).ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let mut probe: &Path = &absolute;
        loop {
            if let Ok(meta) = std::fs::metadata(probe) {
                return Some(meta.dev());
            }
            probe = probe.parent()?;
        }
    }
    #[cfg(windows)]
    {
        // (the drive or share)
        use std::hash::{Hash, Hasher};
        match absolute.components().next()? {
            std::path::Component::Prefix(prefix) => {
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                prefix
                    .as_os_str()
                    .to_string_lossy()
                    .to_uppercase()
                    .hash(&mut hasher);
                Some(hasher.finish())
            }
            _ => None,
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = absolute;
        None
    }
}

/// Where each location's media goes: `destination` for those on its
/// filesystem (or where that can't be told), and for each other
/// filesystem, `<location>-hydrus-rs` beside the first location on it.
/// The destinations, `destination` first, and each location's.
fn plan_destinations(
    locations: &[Location],
    destination: &Path,
    filesystem: &dyn Fn(&Path) -> Option<u64>,
) -> (Vec<PathBuf>, BTreeMap<i64, usize>) {
    let home = filesystem(destination);
    let mut destinations = vec![destination.to_path_buf()];
    let mut by_filesystem: BTreeMap<u64, usize> = BTreeMap::new();
    let mut of_location = BTreeMap::new();
    for location in locations {
        let index = match filesystem(&location.path) {
            Some(fs) if Some(fs) != home => *by_filesystem.entry(fs).or_insert_with(|| {
                let mut name = location
                    .path
                    .components()
                    .collect::<PathBuf>()
                    .into_os_string();
                name.push("-hydrus-rs");
                destinations.push(PathBuf::from(name));
                destinations.len() - 1
            }),
            _ => 0,
        };
        of_location.insert(location.id, index);
    }
    (destinations, of_location)
}

fn subfolders(conn: &Connection) -> Result<Vec<Subfolder>> {
    let mut stmt = conn.prepare(
        "SELECT s.prefix, s.location_id, l.path FROM storage_subfolders s
         JOIN storage_locations l USING (location_id) ORDER BY s.prefix",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, String>(2)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (prefix, location_id, location) = row?;
        // 'f3ab' -> 'f3a/b': the first folder keeps the kind letter and two hex digits
        let split = prefix.len().min(3);
        let mut relative = PathBuf::from(&prefix[..split]);
        let mut rest = &prefix[split..];
        while !rest.is_empty() {
            let take = rest.len().min(2);
            relative.push(&rest[..take]);
            rest = &rest[take..];
        }
        out.push(Subfolder {
            prefix,
            relative,
            location_id,
            location: PathBuf::from(location),
            destination: PathBuf::new(),
        });
    }
    Ok(out)
}

/// The regular files directly in `dir` (subfolders at deeper granularity are
/// separate [`Subfolder`]s).
fn files_in(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    match std::fs::read_dir(dir) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry?;
                if entry.file_type()?.is_file() {
                    out.push(entry.path());
                }
            }
        }
        // a subfolder with no files yet may not exist
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    Ok(out)
}

/// The hashes (hex) of the media the store holds: files current in local
/// file storage. A media file or thumbnail is named by its hash.
fn stored_hashes(conn: &Connection) -> Result<HashSet<String>> {
    let services = crate::services::ServiceRegistry::load(conn)?;
    let storage = crate::content::DomainRoles::new(&services)?.local_file_storage;
    let mut stmt = conn.prepare(
        "SELECT h.sha256 FROM file_domain_current d JOIN hashes h USING (hash_id)
         WHERE d.service_id = ?1",
    )?;
    let rows = stmt.query_map([storage], |r| r.get::<_, Vec<u8>>(0))?;
    let mut out = HashSet::new();
    for row in rows {
        out.insert(hex::encode(row?));
    }
    Ok(out)
}

/// Whether a file in the media folders belongs to stored media.
fn is_stored(path: &Path, stored: &HashSet<String>) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| n.split('.').next())
        .is_some_and(|hash| stored.contains(hash))
}

fn is_cross_device(e: &std::io::Error) -> bool {
    e.kind() == std::io::ErrorKind::CrossesDevices
}

/// Give the store at `db_path` its own media under `destination`, and point
/// its storage locations there. On failure, whatever was created under
/// `destination` is removed and the store still points at the source.
pub fn transfer_media(
    db_path: &Path,
    destination: &Path,
    mode: TransferMode,
) -> Result<TransferReport> {
    transfer_media_on(db_path, destination, mode, &filesystem)
}

/// [`transfer_media`], telling filesystems apart with `filesystem`.
fn transfer_media_on(
    db_path: &Path,
    destination: &Path,
    mode: TransferMode,
    filesystem: &dyn Fn(&Path) -> Option<u64>,
) -> Result<TransferReport> {
    let conn = Connection::open(db_path)?;
    if mode == TransferMode::InPlace {
        let source = conn
            .query_row(
                "SELECT path FROM storage_locations ORDER BY location_id LIMIT 1",
                [],
                |r| r.get::<_, String>(0),
            )
            .map(PathBuf::from)
            .unwrap_or_default();
        settings::set(
            &conn,
            &MediaOwnership {
                shared_with: Some(source.clone()),
            },
        )?;
        return Ok(TransferReport {
            destinations: vec![source],
            ..TransferReport::default()
        });
    }
    let sources = locations(&conn)?;
    let (destinations, of_location) = plan_destinations(&sources, destination, filesystem);
    for d in &destinations {
        if d.exists() {
            return Err(StoreError::Invalid(format!(
                "{} already exists; choose a new directory for the media",
                d.display()
            )));
        }
    }
    let mut folders = subfolders(&conn)?;
    for folder in &mut folders {
        let index = of_location.get(&folder.location_id).copied().unwrap_or(0);
        folder.destination.clone_from(&destinations[index]);
    }
    let stored = stored_hashes(&conn)?;
    for d in &destinations {
        std::fs::create_dir_all(d)?;
    }
    let result = match mode {
        TransferMode::Move => move_folders(&folders, &stored),
        _ => link_or_copy(&folders, mode, &stored),
    };
    let mut report = match result {
        Ok(report) => report,
        Err(e) => {
            if mode != TransferMode::Move {
                for d in &destinations {
                    let _ = std::fs::remove_dir_all(d);
                }
            }
            return Err(e);
        }
    };
    // a location for each destination that received some, weighing what
    // its sources weighed (no limit if any had none)
    let used: Vec<usize> = (0..destinations.len())
        .filter(|&i| i > 0 || of_location.values().any(|&j| j == 0))
        .collect();
    let used = if used.is_empty() { vec![0] } else { used };
    for (i, d) in destinations.iter().enumerate() {
        if !used.contains(&i) {
            let _ = std::fs::remove_dir(d);
        }
    }
    report.destinations = used.iter().map(|&i| destinations[i].clone()).collect();
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM storage_locations", [])?;
    for &index in &used {
        let from: Vec<&Location> = sources
            .iter()
            .filter(|l| of_location.get(&l.id) == Some(&index))
            .collect();
        let weight = from
            .iter()
            .filter_map(|l| l.weight)
            .reduce(i64::saturating_add)
            .unwrap_or(1);
        let max_bytes: Option<i64> = if from.is_empty() {
            None
        } else {
            from.iter()
                .map(|l| l.max_bytes)
                .try_fold(0i64, |sum, m| m.map(|m| sum.saturating_add(m)))
        };
        let thumbnail_override = from.iter().any(|l| l.thumbnail_override);
        tx.execute(
            "INSERT INTO storage_locations (location_id, path, ideal_weight, max_bytes, is_thumbnail_override)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                index as i64 + 1,
                destinations[index].to_string_lossy(),
                weight,
                max_bytes,
                thumbnail_override
            ],
        )?;
    }
    tx.execute("DELETE FROM storage_subfolders", [])?;
    for folder in &folders {
        let index = of_location.get(&folder.location_id).copied().unwrap_or(0);
        tx.execute(
            "INSERT OR IGNORE INTO storage_subfolders (prefix, location_id) VALUES (?1, ?2)",
            rusqlite::params![folder.prefix, index as i64 + 1],
        )?;
    }
    settings::set(&tx, &MediaOwnership::default())?;
    tx.commit()?;
    Ok(report)
}

fn link_or_copy(
    folders: &[Subfolder],
    mode: TransferMode,
    stored: &HashSet<String>,
) -> Result<TransferReport> {
    use rayon::prelude::*;
    let files = AtomicU64::new(0);
    let bytes = AtomicU64::new(0);
    let skipped = AtomicU64::new(0);
    folders.par_iter().try_for_each(|folder| -> Result<()> {
        let target = folder.destination.join(&folder.relative);
        std::fs::create_dir_all(&target)?;
        for source in files_in(&folder.location.join(&folder.relative))? {
            let name = source.file_name().expect("read_dir entries have names");
            let dest = target.join(name);
            // (a subfolder in two locations, as mid-rebalance, may hold a
            // file twice)
            if !is_stored(&source, stored) || dest.exists() {
                skipped.fetch_add(1, Ordering::Relaxed);
                continue;
            }
            let size = match mode {
                TransferMode::Hardlink => {
                    std::fs::hard_link(&source, &dest).map_err(|e| {
                        if is_cross_device(&e) {
                            StoreError::Invalid(format!(
                                "{} is on a different filesystem from {}, so it can't be hardlinked; \
                                 copy the files instead, or choose a media directory on the same filesystem",
                                source.display(),
                                folder.destination.display()
                            ))
                        } else {
                            e.into()
                        }
                    })?;
                    std::fs::metadata(&dest)?.len()
                }
                _ => crate::paths::copy_file(&source, &dest)?,
            };
            files.fetch_add(1, Ordering::Relaxed);
            bytes.fetch_add(size, Ordering::Relaxed);
        }
        Ok(())
    })?;
    Ok(TransferReport {
        files: files.into_inner(),
        bytes: bytes.into_inner(),
        skipped: skipped.into_inner(),
        destinations: Vec::new(),
    })
}

/// Move whole subfolders; undo the moves made so far if one fails.
fn move_folders(folders: &[Subfolder], stored: &HashSet<String>) -> Result<TransferReport> {
    let mut done: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut report = TransferReport::default();
    let result = (|| -> Result<()> {
        // deepest first, so 'f3a/b' moves before 'f3a' would swallow it
        let mut ordered: Vec<&Subfolder> = folders.iter().collect();
        ordered.sort_by_key(|f| std::cmp::Reverse(f.relative.components().count()));
        for folder in ordered {
            let source = folder.location.join(&folder.relative);
            let target = folder.destination.join(&folder.relative);
            let (files, others): (Vec<PathBuf>, Vec<PathBuf>) = files_in(&source)?
                .into_iter()
                .partition(|f| is_stored(f, stored));
            report.skipped += others.len() as u64;
            if files.is_empty() {
                continue;
            }
            for file in &files {
                report.bytes += std::fs::metadata(file)?.len();
            }
            report.files += files.len() as u64;
            std::fs::create_dir_all(&target)?;
            for file in files {
                let dest = target.join(file.file_name().expect("read_dir entries have names"));
                std::fs::rename(&file, &dest).map_err(|e| {
                    if is_cross_device(&e) {
                        StoreError::Invalid(format!(
                            "{} is on a different filesystem from {}; copy the files instead",
                            file.display(),
                            folder.destination.display()
                        ))
                    } else {
                        StoreError::from(e)
                    }
                })?;
                done.push((file, dest));
            }
        }
        Ok(())
    })();
    if let Err(e) = result {
        for (source, dest) in done.into_iter().rev() {
            let _ = std::fs::rename(&dest, &source);
        }
        return Err(e);
    }
    Ok(report)
}

/// Transfer a fixture's media and check the store can still find every file.
#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::import::tests::import_basic;

    fn stem(path: &Path) -> String {
        let name = path.file_name().unwrap().to_str().unwrap();
        name.split('.').next().unwrap().to_owned()
    }

    /// The hashes of the media and thumbnails the reference install had
    /// waiting to be deleted.
    fn pending_deletes(install: &Path) -> BTreeSet<String> {
        let legacy = hydrus_legacy::LegacyDb::open(install).unwrap();
        let ids: Vec<_> = legacy
            .deferred_physical_file_deletes()
            .unwrap()
            .chain(legacy.deferred_physical_thumbnail_deletes().unwrap())
            .map(Result::unwrap)
            .collect();
        let all: std::collections::HashMap<_, _> =
            legacy.hashes().unwrap().map(Result::unwrap).collect();
        let hashes: BTreeSet<String> = ids.iter().map(|id| all[id].to_hex()).collect();
        assert!(!hashes.is_empty(), "the fixture has some");
        hashes
    }

    fn file_count(dir: &Path) -> usize {
        walk(dir).len()
    }

    fn walk(dir: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                out.extend(walk(&path));
            } else {
                out.push(path);
            }
        }
        out
    }

    #[test]
    fn hardlink_copy_and_move_give_the_store_its_own_media() {
        for mode in [
            TransferMode::Hardlink,
            TransferMode::Copy,
            TransferMode::Move,
        ] {
            let (source, dest_dir, db) = import_basic();
            let source_files = source.path().join("client_files");
            let before = file_count(&source_files);
            let pending = pending_deletes(source.path());
            assert!(before > 0);
            let media = dest_dir.path().join("media");
            let report = transfer_media(&db, &media, mode).unwrap();
            // everything but what the reference was about to delete
            let pending_files = walk(&source_files)
                .iter()
                .filter(|p| pending.contains(&stem(p)))
                .count();
            let stored = before - pending_files;
            assert_eq!(report.files as usize, stored, "{mode:?}");
            assert_eq!(report.skipped as usize, pending_files, "{mode:?}");
            assert_eq!(file_count(&media), stored, "{mode:?}");
            let left: BTreeSet<String> = walk(&source_files).iter().map(|p| stem(p)).collect();
            if mode == TransferMode::Move {
                assert_eq!(left, pending, "only the pending deletes stay");
            } else {
                assert_eq!(file_count(&source_files), before, "{mode:?}");
            }
            assert!(
                walk(&media).iter().all(|p| !pending.contains(&stem(p))),
                "{mode:?}"
            );

            // every file the store knows is found under the new location
            let conn = Connection::open(&db).unwrap();
            let storage = crate::storage::FileStorage::load(&conn).unwrap();
            assert_eq!(storage.locations().len(), 1);
            let mut stmt = conn
                .prepare("SELECT h.sha256, f.mime FROM files f JOIN hashes h USING (hash_id)")
                .unwrap();
            let rows: Vec<(Vec<u8>, u8)> = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap()
                .map(Result::unwrap)
                .collect();
            let mut found = 0;
            for (hash, mime) in rows {
                let hash = hydrus_core::Sha256::from_slice(&hash).unwrap();
                let Some(mime) = hydrus_core::Mime::from_code(mime) else {
                    continue;
                };
                if storage.file_path(&hash, mime).unwrap().is_file() {
                    found += 1;
                }
            }
            assert!(found > 0, "{mode:?}");
            let ownership: MediaOwnership = settings::get(&conn).unwrap();
            assert_eq!(ownership.shared_with, None);
        }
    }

    /// The files the store knows (but those in `skip`) that it finds where
    /// its storage says.
    fn found(db: &Path, skip: &BTreeSet<String>) -> usize {
        let conn = Connection::open(db).unwrap();
        let storage = crate::storage::FileStorage::load(&conn).unwrap();
        let mut stmt = conn
            .prepare("SELECT h.sha256, f.mime FROM files f JOIN hashes h USING (hash_id)")
            .unwrap();
        let rows: Vec<(Vec<u8>, u8)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        rows.into_iter()
            .filter(|(hash, mime)| {
                let hash = hydrus_core::Sha256::from_slice(hash).unwrap();
                !skip.contains(&hash.to_hex())
                    && hydrus_core::Mime::from_code(*mime)
                        .and_then(|m| storage.file_path(&hash, m))
                        .is_some_and(|p| p.is_file())
            })
            .count()
    }

    /// Media on two drives stays on each: the second drive's goes into a
    /// directory beside its location there, which becomes a storage
    /// location with that location's weight and limit.
    #[test]
    fn each_drive_s_media_stays_on_it() {
        for mode in [
            TransferMode::Hardlink,
            TransferMode::Copy,
            TransferMode::Move,
        ] {
            let (source, dest_dir, db) = import_basic();
            let first = source.path().join("client_files");
            let second = source.path().join("other drive").join("client_files");
            // half the subfolders (those for hashes starting 0-7) on the second drive
            {
                let conn = Connection::open(&db).unwrap();
                conn.execute(
                    "INSERT INTO storage_locations (location_id, path, ideal_weight, max_bytes, is_thumbnail_override)
                     VALUES (2, ?1, 3, 1000000000, 0)",
                    [second.to_string_lossy()],
                )
                .unwrap();
                let moved: Vec<String> = conn
                    .prepare(
                        "SELECT prefix FROM storage_subfolders WHERE substr(prefix, 2, 1) IN ('0','1','2','3','4','5','6','7')",
                    )
                    .unwrap()
                    .query_map([], |r| r.get(0))
                    .unwrap()
                    .map(Result::unwrap)
                    .collect();
                assert!(!moved.is_empty());
                for prefix in &moved {
                    let from = first.join(prefix);
                    if from.exists() {
                        std::fs::create_dir_all(&second).unwrap();
                        std::fs::rename(&from, second.join(prefix)).unwrap();
                    }
                    conn.execute(
                        "UPDATE storage_subfolders SET location_id = 2 WHERE prefix = ?1",
                        [prefix],
                    )
                    .unwrap();
                }
            }
            // (what the reference was about to delete is left behind)
            let pending = pending_deletes(source.path());
            let known = found(&db, &pending);
            let on_second = file_count(&second);
            assert!(on_second > 0 && file_count(&first) > 0);

            let media = dest_dir.path().join("media");
            let other_drive = |p: &Path| -> Option<u64> {
                Some(if p.starts_with(source.path().join("other drive")) {
                    2
                } else {
                    1
                })
            };
            let report = transfer_media_on(&db, &media, mode, &other_drive).unwrap();
            let beside = source
                .path()
                .join("other drive")
                .join("client_files-hydrus-rs");
            assert_eq!(
                report.destinations,
                [media.clone(), beside.clone()],
                "{mode:?}"
            );
            let stored_on_second = if mode == TransferMode::Move {
                file_count(&beside)
            } else {
                walk(&second)
                    .iter()
                    .filter(|p| !pending.contains(&stem(p)))
                    .count()
            };
            assert_eq!(file_count(&beside), stored_on_second, "{mode:?}");
            assert!(stored_on_second > 0);
            assert!(
                walk(&media).iter().all(|p| {
                    let name = p
                        .strip_prefix(&media)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned();
                    !('0'..='7').any(|c| {
                        name.starts_with(&format!("f{c}")) || name.starts_with(&format!("t{c}"))
                    })
                }),
                "{mode:?}"
            );
            assert_eq!(
                found(&db, &pending),
                known,
                "{mode:?}: every file is still found"
            );

            let conn = Connection::open(&db).unwrap();
            let locations: Vec<(String, Option<i64>, Option<i64>)> = conn
                .prepare("SELECT path, ideal_weight, max_bytes FROM storage_locations ORDER BY location_id")
                .unwrap()
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .unwrap()
                .map(Result::unwrap)
                .collect();
            assert_eq!(locations.len(), 2, "{locations:?}");
            assert_eq!(locations[0].0, media.to_string_lossy());
            assert_eq!(
                locations[1],
                (
                    beside.to_string_lossy().into_owned(),
                    Some(3),
                    Some(1_000_000_000)
                )
            );
        }
    }

    #[test]
    fn in_place_marks_the_media_as_shared() {
        let (source, _dest_dir, db) = import_basic();
        transfer_media(&db, Path::new("/nonexistent"), TransferMode::InPlace).unwrap();
        let conn = Connection::open(&db).unwrap();
        let ownership: MediaOwnership = settings::get(&conn).unwrap();
        assert_eq!(
            ownership.shared_with,
            Some(source.path().join("client_files"))
        );
    }

    #[test]
    fn refuses_an_existing_destination() {
        let (_source, dest_dir, db) = import_basic();
        assert!(transfer_media(&db, dest_dir.path(), TransferMode::Copy).is_err());
    }
}
