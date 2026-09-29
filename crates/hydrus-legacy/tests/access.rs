//! How a reference database is opened: never written, usable while the
//! reference client is running, and only at version 688.

mod common;

use std::collections::BTreeMap;
use std::path::Path;

use common::collect;
use hydrus_core::{ContentStatus, HashId, ServiceId};
use hydrus_legacy::db::DATABASE_FILES;
use hydrus_legacy::{LegacyDb, LegacyError, OpenMode, OpenOptions};
use sha2::Digest;

fn digests(dir: &Path) -> BTreeMap<String, String> {
    DATABASE_FILES
        .iter()
        .map(|(_, file)| {
            let bytes = std::fs::read(dir.join(file)).unwrap();
            ((*file).to_owned(), hex::encode(sha2::Sha256::digest(bytes)))
        })
        .collect()
}

fn listing(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// Touch every file and a good spread of readers.
fn read_everything(db: &LegacyDb) {
    let _snapshot = db.snapshot().unwrap();
    db.services().unwrap();
    collect(db.hashes());
    collect(db.tags());
    collect(db.files_info());
    for service in db.service_infos() {
        if service.service_type.is_real_tag_service() {
            for status in ContentStatus::ALL {
                collect(db.mappings(service.id, *status));
            }
        }
    }
    collect(db.file_maintenance_jobs());
    db.client_options().unwrap().unwrap();
    db.legacy_options().unwrap();
}

#[test]
fn reading_never_modifies_the_database_files() {
    let dir = hydrus_testkit::legacy_fixture("basic");
    let before = digests(dir.path());
    let listing_before = listing(dir.path());
    {
        let db = LegacyDb::open(dir.path()).unwrap();
        assert_eq!(db.mode(), OpenMode::Shared);
        read_everything(&db);
    }
    assert_eq!(digests(dir.path()), before);

    // WAL readers need -wal and -shm files; SQLite creates them if absent,
    // empty, and that is the only change to the directory.
    let created: Vec<String> = listing(dir.path())
        .into_iter()
        .filter(|name| !listing_before.contains(name))
        .collect();
    for name in &created {
        assert!(
            name.ends_with(".db-wal") || name.ends_with(".db-shm"),
            "unexpected new file {name}"
        );
        if name.ends_with("-wal") {
            let len = std::fs::metadata(dir.path().join(name)).unwrap().len();
            assert_eq!(len, 0, "{name} is not empty");
        }
    }
}

#[test]
fn immutable_mode_creates_nothing() {
    let dir = hydrus_testkit::legacy_fixture("basic");
    let before = digests(dir.path());
    let listing_before = listing(dir.path());
    {
        let db = LegacyDb::open_with(
            dir.path(),
            OpenOptions {
                mode: OpenMode::Immutable,
                ..OpenOptions::default()
            },
        )
        .unwrap();
        assert_eq!(db.mode(), OpenMode::Immutable);
        read_everything(&db);
    }
    assert_eq!(digests(dir.path()), before);
    assert_eq!(listing(dir.path()), listing_before);
}

/// On read-only media (or a directory we may not write to) with no writer
/// running, SQLite cannot create the WAL reader files, so the open falls
/// back to immutable mode. File permissions are not enforced for root, so
/// this only tests something when run unprivileged (as CI does).
#[cfg(unix)]
#[test]
fn read_only_directory_falls_back_to_immutable() {
    use std::os::unix::fs::PermissionsExt;

    let dir = hydrus_testkit::legacy_fixture("basic");
    let listing_before = listing(dir.path());
    let set_mode = |mode| {
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(mode)).unwrap();
    };
    set_mode(0o555);
    let probe = dir.path().join("probe");
    if std::fs::write(&probe, b"").is_ok() {
        eprintln!("permissions are not enforced for this user; skipping");
        std::fs::remove_file(probe).unwrap();
        set_mode(0o755);
        return;
    }
    let result = LegacyDb::open(dir.path()).map(|db| {
        let mode = db.mode();
        read_everything(&db);
        mode
    });
    let after = listing(dir.path());
    set_mode(0o755);
    assert_eq!(result.unwrap(), OpenMode::Immutable);
    assert_eq!(after, listing_before);
}

#[test]
fn reads_alongside_a_running_writer() {
    let dir = hydrus_testkit::legacy_fixture("basic");
    // stand-in for the reference client: a read-write WAL connection that
    // holds a write transaction open, as the reference does between commits
    let writer = rusqlite::Connection::open(dir.path().join("client.db")).unwrap();
    let mode: String = writer
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
    writer
        .execute_batch("BEGIN IMMEDIATE; INSERT INTO file_inbox (hash_id) VALUES (99999);")
        .unwrap();

    let db = LegacyDb::open(dir.path()).unwrap();
    let new_file = HashId(99_999);
    assert!(
        !collect(db.inbox()).contains(&new_file),
        "uncommitted write seen"
    );

    let snapshot = db.snapshot().unwrap();
    writer.execute_batch("COMMIT").unwrap();
    assert!(
        !collect(db.inbox()).contains(&new_file),
        "a snapshot must not see later commits"
    );
    drop(snapshot);
    assert!(collect(db.inbox()).contains(&new_file), "commit not seen");
}

fn set_version(dir: &Path, version: i64) {
    let connection = rusqlite::Connection::open(dir.join("client.db")).unwrap();
    connection
        .execute("UPDATE version SET version = ?1", [version])
        .unwrap();
}

#[test]
fn rejects_other_versions_with_advice() {
    for (version, advice) in [(687, "update it first"), (689, "newer hydrus client")] {
        let dir = hydrus_testkit::legacy_fixture("basic");
        set_version(dir.path(), version);
        let error = LegacyDb::open(dir.path()).unwrap_err();
        assert!(
            matches!(error, LegacyError::UnsupportedVersion { found, .. } if found == version),
            "{error:?}"
        );
        let message = error.to_string();
        assert!(message.contains(&version.to_string()), "{message}");
        assert!(message.contains("688"), "{message}");
        assert!(message.contains(advice), "{message}");
    }
}

#[test]
fn rejects_directories_that_are_not_databases() {
    let empty = tempfile::tempdir().unwrap();
    let error = LegacyDb::open(empty.path()).unwrap_err();
    assert!(
        matches!(&error, LegacyError::NotADatabase { reason, .. } if reason.contains("client.db")),
        "{error:?}"
    );

    let dir = hydrus_testkit::legacy_fixture("basic");
    std::fs::remove_file(dir.path().join("client.mappings.db")).unwrap();
    let error = LegacyDb::open(dir.path()).unwrap_err();
    assert!(error.to_string().contains("client.mappings.db"), "{error}");

    assert!(matches!(
        LegacyDb::open(empty.path().join("nope")).unwrap_err(),
        LegacyError::Io { .. }
    ));
}

#[test]
fn per_service_readers_check_the_service() {
    let fixture = common::basic();
    let db = &fixture.db;
    let my_files = ServiceId(6);
    let my_tags = ServiceId(9);
    assert!(matches!(
        db.mappings(my_files, ContentStatus::Current).unwrap_err(),
        LegacyError::BadService { service_id: 6, .. }
    ));
    assert!(matches!(
        db.current_files(my_tags).unwrap_err(),
        LegacyError::BadService { service_id: 9, .. }
    ));
    assert!(matches!(
        db.tag_siblings(ServiceId(999), ContentStatus::Current)
            .unwrap_err(),
        LegacyError::BadService {
            service_id: 999,
            ..
        }
    ));
    assert!(matches!(
        db.repository_updates(my_tags).unwrap_err(),
        LegacyError::BadService { .. }
    ));
}

#[test]
fn pagination_does_not_change_results() {
    let dir = hydrus_testkit::legacy_fixture("basic");
    let db = LegacyDb::open(dir.path()).unwrap();
    let all = collect(db.mappings(ServiceId(9), ContentStatus::Current));
    assert_eq!(all.len(), 129);
    for batch_size in [1, 2, 3, 128, 129, 130] {
        db.set_batch_size(batch_size);
        assert_eq!(
            collect(db.mappings(ServiceId(9), ContentStatus::Current)),
            all,
            "batch size {batch_size}"
        );
        assert_eq!(collect(db.hashes()).len(), 63);
    }
    // an iterator can be dropped half way and the database used again
    db.set_batch_size(5);
    let first: Vec<_> = db.hashes().unwrap().take(12).collect();
    assert_eq!(first.len(), 12);
    assert_eq!(collect(db.hashes()).len(), 63);
}
