use std::collections::{BTreeMap, BTreeSet};

use proptest::prelude::*;
use rusqlite::Connection;

use hydrus_core::{HashId, ServiceId, ServiceKey, TagId};

use super::*;
use crate::services::{self, RepositoryConfig};
use crate::{counts, schema};

const NOW: i64 = 1_700_000_000_000;

/// A client with two local file domains, two local tag services and a tag
/// repository, and some siblings and parents.
struct World {
    conn: Connection,
    snap: Snapshot,
    roles: DomainRoles,
    second_local: ServiceId,
    tag_services: Vec<ServiceId>,
}

fn world() -> World {
    let mut conn = Connection::open_in_memory().unwrap();
    schema::configure(&conn).unwrap();
    schema::migrate(&mut conn).unwrap();
    for (key, name, kind) in services::default_services() {
        services::insert(&conn, &key, &name, &kind).unwrap();
    }
    let second_local = services::insert(
        &conn,
        &ServiceKey::new(vec![7; 32]),
        "second files",
        &ServiceKind::LocalFiles,
    )
    .unwrap();
    let extra_tags = services::insert(
        &conn,
        &ServiceKey::new(vec![8; 32]),
        "extra tags",
        &ServiceKind::LocalTags,
    )
    .unwrap();
    let repo = services::insert(
        &conn,
        &ServiceKey::new(vec![9; 32]),
        "tag repo",
        &ServiceKind::TagRepository(RepositoryConfig::default()),
    )
    .unwrap();
    let registry = ServiceRegistry::load(&conn).unwrap();
    let my_tags = registry
        .builtin(hydrus_core::service::builtin_keys::MY_TAGS)
        .unwrap()
        .id;
    // my tags: 1 -> 2 <- 3 siblings; 2 -> 4, 5 -> 4, 4 -> 6 parents
    for (bad, good) in [(1, 2), (3, 2)] {
        conn.execute(
            "INSERT INTO tag_siblings (service_id, status, bad_tag_id, good_tag_id) VALUES (?1, 0, ?2, ?3)",
            rusqlite::params![my_tags, bad, good],
        )
        .unwrap();
    }
    for (child, parent) in [(2, 4), (5, 4), (4, 6)] {
        conn.execute(
            "INSERT INTO tag_parents (service_id, status, child_tag_id, parent_tag_id) VALUES (?1, 0, ?2, ?3)",
            rusqlite::params![my_tags, child, parent],
        )
        .unwrap();
    }
    // the extra service displays with my tags' siblings, and its own parent 1 -> 3
    conn.execute(
        "INSERT INTO tag_parents (service_id, status, child_tag_id, parent_tag_id) VALUES (?1, 0, 1, 3)",
        [extra_tags],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO tag_display_application VALUES (?1, 0, 0, ?2), (?1, 0, 1, ?1)",
        [extra_tags, my_tags],
    )
    .unwrap();
    let snap = Snapshot::load(&conn).unwrap();
    let roles = DomainRoles::new(&snap.services).unwrap();
    World {
        conn,
        snap,
        roles,
        second_local,
        tag_services: vec![my_tags, extra_tags, repo],
    }
}

fn hashes(ids: &[u32]) -> Vec<HashId> {
    ids.iter().map(|&i| HashId(i)).collect()
}

impl World {
    fn writer(&self) -> ContentWriter<'_> {
        ContentWriter::new(&self.conn, &self.snap, NOW).unwrap()
    }

    fn current(&self, domain: ServiceId) -> BTreeMap<HashId, Option<i64>> {
        let mut stmt = self
            .conn
            .prepare("SELECT hash_id, added_ms FROM file_domain_current WHERE service_id = ?1")
            .unwrap();
        stmt.query_map([domain], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    fn current_set(&self, domain: ServiceId) -> BTreeSet<HashId> {
        self.current(domain).into_keys().collect()
    }

    fn deleted(&self, domain: ServiceId) -> BTreeMap<HashId, (Option<i64>, Option<i64>)> {
        let mut stmt = self
            .conn
            .prepare("SELECT hash_id, deleted_ms, original_added_ms FROM file_domain_deleted WHERE service_id = ?1")
            .unwrap();
        stmt.query_map([domain], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?))))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    /// All count rows, storage and display, of every tag service.
    fn all_counts(&self) -> BTreeMap<(ServiceId, bool, u32, u32), (i64, i64)> {
        let mut out = BTreeMap::new();
        for &service in &self.tag_services {
            let t = schema::MappingTables::new(service);
            for (display, table) in [(false, &t.counts), (true, &t.display_counts)] {
                let mut stmt = self
                    .conn
                    .prepare(&format!(
                        "SELECT domain_id, tag_id, current, pending FROM {table}"
                    ))
                    .unwrap();
                let rows = stmt
                    .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
                    .unwrap();
                for row in rows {
                    let (domain, tag, c, p): (u32, u32, i64, i64) = row.unwrap();
                    out.insert((service, display, domain, tag), (c, p));
                }
            }
        }
        out
    }

    fn assert_counts_match_rebuild(&self) {
        let incremental = self.all_counts();
        counts::rebuild_all(&self.conn).unwrap();
        let rebuilt = self.all_counts();
        assert_eq!(
            incremental, rebuilt,
            "incremental counts differ from a rebuild"
        );
    }

    /// The umbrella domains are exactly what their definitions say.
    fn assert_domain_invariants(&self) {
        let r = &self.roles;
        let mut local_union = BTreeSet::new();
        for &d in &r.local {
            local_union.extend(self.current_set(d));
        }
        let clm = self.current_set(r.combined_local_media);
        assert_eq!(
            clm, local_union,
            "combined local media != union of local domains"
        );
        let trash = self.current_set(r.trash);
        assert!(
            trash.is_disjoint(&clm),
            "a file is both trashed and in a local domain"
        );
        let mut stored = clm.clone();
        stored.extend(&trash);
        stored.extend(self.current_set(r.local_updates));
        assert_eq!(
            self.current_set(r.local_file_storage),
            stored,
            "local file storage != its parts"
        );
        let mut deleted_covered = BTreeSet::new();
        for &d in &r.covered_by_combined_deleted {
            deleted_covered.extend(self.deleted(d).into_keys());
        }
        assert_eq!(
            self.current_set(r.combined_deleted),
            deleted_covered,
            "combined deleted out of sync"
        );
        // a file is never both current and deleted in one domain
        for &d in &r.counted {
            let current = self.current_set(d);
            let deleted: BTreeSet<HashId> = self.deleted(d).into_keys().collect();
            assert!(
                current.is_disjoint(&deleted),
                "current and deleted at once in {d}"
            );
        }
    }
}

#[test]
fn import_trash_undelete_and_purge() {
    let w = world();
    let my_files = w.roles.local[0];
    let h = hashes(&[1]);
    let mut c = w.writer();
    c.add_files(my_files, &[(HashId(1), Some(100))]).unwrap();
    c.conn
        .execute("INSERT INTO file_inbox VALUES (1)", [])
        .unwrap();
    c.finish().unwrap();
    for d in [
        my_files,
        w.roles.combined_local_media,
        w.roles.local_file_storage,
    ] {
        assert_eq!(w.current(d), BTreeMap::from([(HashId(1), Some(100))]));
    }
    w.assert_domain_invariants();

    let mut c = w.writer();
    c.delete_files(my_files, &h, Some("boring")).unwrap();
    c.finish().unwrap();
    assert!(w.current(my_files).is_empty());
    assert_eq!(
        w.current(w.roles.trash),
        BTreeMap::from([(HashId(1), Some(NOW))])
    );
    assert_eq!(
        w.deleted(my_files),
        BTreeMap::from([(HashId(1), (Some(NOW), Some(100)))])
    );
    assert_eq!(
        w.current_set(w.roles.combined_deleted),
        BTreeSet::from([HashId(1)])
    );
    let reason: String = w
        .conn
        .query_row(
            "SELECT text FROM file_deletion_reasons JOIN texts ON text_id = reason_id WHERE hash_id = 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(reason, "boring");
    w.assert_domain_invariants();

    let mut c = w.writer();
    c.undelete_files(w.roles.trash, &h).unwrap();
    c.finish().unwrap();
    assert_eq!(
        w.current(my_files),
        BTreeMap::from([(HashId(1), Some(100))])
    );
    assert!(w.current(w.roles.trash).is_empty());
    assert!(w.deleted(my_files).is_empty());
    assert!(w.current(w.roles.combined_deleted).is_empty());
    w.assert_domain_invariants();

    let mut c = w.writer();
    c.delete_files(my_files, &h, None).unwrap();
    c.delete_files(w.roles.trash, &h, None).unwrap();
    c.finish().unwrap();
    assert!(w.current(w.roles.local_file_storage).is_empty());
    let queued: i64 = w
        .conn
        .query_row(
            "SELECT count(*) FROM deferred_physical_deletes WHERE hash_id = 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(queued, 1);
    let inboxed: i64 = w
        .conn
        .query_row("SELECT count(*) FROM file_inbox", [], |r| r.get(0))
        .unwrap();
    assert_eq!(inboxed, 0, "leaving local storage archives");
    w.assert_domain_invariants();
    w.assert_counts_match_rebuild();
}

#[test]
fn deleting_from_one_of_two_local_domains_does_not_trash() {
    let w = world();
    let my_files = w.roles.local[0];
    let mut c = w.writer();
    c.add_files(my_files, &[(HashId(1), Some(1))]).unwrap();
    c.add_files(w.second_local, &[(HashId(1), Some(2))])
        .unwrap();
    c.delete_files(my_files, &hashes(&[1]), None).unwrap();
    c.finish().unwrap();
    assert!(w.current(w.roles.trash).is_empty());
    assert_eq!(
        w.current(w.roles.combined_local_media),
        BTreeMap::from([(HashId(1), Some(1))])
    );
    // deleted from a covered domain, so in combined deleted despite being local
    assert_eq!(
        w.current_set(w.roles.combined_deleted),
        BTreeSet::from([HashId(1)])
    );
    w.assert_domain_invariants();
}

#[test]
fn mapping_state_machine() {
    let world = world();
    let repo = world.tag_services[2];
    let tables = schema::MappingTables::new(repo);
    let files = hashes(&[1, 2]);
    let state = |world: &World| -> Vec<(String, u32)> {
        let mut out = Vec::new();
        for (name, table) in [
            ("c", &tables.current),
            ("d", &tables.deleted),
            ("p", &tables.pending),
            ("x", &tables.petitioned),
        ] {
            let mut stmt = world
                .conn
                .prepare(&format!("SELECT hash_id FROM {table} ORDER BY hash_id"))
                .unwrap();
            for id in stmt.query_map([], |r| r.get::<_, u32>(0)).unwrap() {
                out.push((name.to_owned(), id.unwrap()));
            }
        }
        out
    };
    let expect = |v: &[(&str, u32)]| -> Vec<(String, u32)> {
        v.iter().map(|(a, b)| ((*a).to_owned(), *b)).collect()
    };
    let mut c = world.writer();
    assert_eq!(
        c.update_mappings(repo, &MappingAction::Pend, TagId(9), &files)
            .unwrap(),
        2
    );
    assert_eq!(
        c.update_mappings(repo, &MappingAction::Pend, TagId(9), &files)
            .unwrap(),
        0
    );
    let petition = MappingAction::Petition {
        reason: "wrong".into(),
    };
    assert_eq!(
        c.update_mappings(repo, &petition, TagId(9), &files)
            .unwrap(),
        0,
        "can't petition pending"
    );
    assert_eq!(
        c.update_mappings(repo, &MappingAction::Add, TagId(9), &files[..1])
            .unwrap(),
        1
    );
    c.finish().unwrap();
    assert_eq!(state(&world), expect(&[("c", 1), ("p", 2)]));
    let mut c = world.writer();
    assert_eq!(
        c.update_mappings(repo, &petition, TagId(9), &files[..1])
            .unwrap(),
        1
    );
    assert_eq!(
        c.update_mappings(repo, &MappingAction::Pend, TagId(9), &files[..1])
            .unwrap(),
        0
    );
    assert_eq!(
        c.update_mappings(repo, &MappingAction::Delete, TagId(9), &files)
            .unwrap(),
        2
    );
    c.finish().unwrap();
    assert_eq!(state(&world), expect(&[("d", 1), ("d", 2), ("p", 2)]));
    world.assert_counts_match_rebuild();
}

#[derive(Debug, Clone)]
enum Op {
    Import {
        domain: usize,
        files: Vec<u32>,
    },
    Delete {
        domain: usize,
        files: Vec<u32>,
    },
    Undelete {
        domain: usize,
        files: Vec<u32>,
    },
    ClearDeleteRecords {
        files: Option<Vec<u32>>,
    },
    Map {
        service: usize,
        action: usize,
        tag: u32,
        files: Vec<u32>,
    },
    /// Commit what's accumulated so far and start a new writer.
    Flush,
}

fn files() -> impl Strategy<Value = Vec<u32>> {
    proptest::collection::vec(1u32..=6, 1..4)
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (0usize..3, files()).prop_map(|(domain, files)| Op::Import { domain, files }),
        (0usize..6, files()).prop_map(|(domain, files)| Op::Delete { domain, files }),
        (0usize..5, files()).prop_map(|(domain, files)| Op::Undelete { domain, files }),
        proptest::option::of(files()).prop_map(|files| Op::ClearDeleteRecords { files }),
        (0usize..3, 0usize..6, 1u32..=7, files()).prop_map(|(service, action, tag, files)| {
            Op::Map {
                service,
                action,
                tag,
                files,
            }
        }),
        (0usize..3, 0usize..6, 1u32..=7, files()).prop_map(|(service, action, tag, files)| {
            Op::Map {
                service,
                action,
                tag,
                files,
            }
        }),
        Just(Op::Flush),
    ]
}

impl World {
    fn apply(&self, ops: &[Op]) {
        let r = &self.roles;
        let (my_files, second) = (r.local[0], self.second_local);
        let import_domains = [my_files, second, r.local_updates];
        let delete_domains = [
            my_files,
            second,
            r.combined_local_media,
            r.trash,
            r.local_file_storage,
            r.local_updates,
        ];
        let undelete_domains = [
            my_files,
            second,
            r.combined_local_media,
            r.trash,
            r.local_file_storage,
        ];
        let mut c = self.writer();
        for op in ops {
            match op {
                Op::Import { domain, files } => {
                    let rows: Vec<_> = files
                        .iter()
                        .map(|&f| (HashId(f), Some(i64::from(f))))
                        .collect();
                    c.add_files(import_domains[*domain], &rows).unwrap();
                }
                Op::Delete { domain, files } => {
                    c.delete_files(delete_domains[*domain], &hashes(files), Some("test"))
                        .unwrap();
                }
                Op::Undelete { domain, files } => {
                    c.undelete_files(undelete_domains[*domain], &hashes(files))
                        .unwrap();
                }
                Op::ClearDeleteRecords { files } => {
                    let files = files.as_deref().map(hashes);
                    c.clear_local_delete_records(files.as_deref()).unwrap();
                }
                Op::Map {
                    service,
                    action,
                    tag,
                    files,
                } => {
                    let action = match action {
                        0 => MappingAction::Add,
                        1 => MappingAction::Delete,
                        2 => MappingAction::Pend,
                        3 => MappingAction::RescindPend,
                        4 => MappingAction::Petition { reason: "r".into() },
                        _ => MappingAction::RescindPetition,
                    };
                    c.update_mappings(
                        self.tag_services[*service],
                        &action,
                        TagId(*tag),
                        &hashes(files),
                    )
                    .unwrap();
                }
                Op::Flush => {
                    c.finish().unwrap();
                    c = self.writer();
                }
            }
        }
        c.finish().unwrap();
    }
}

proptest! {
    #[test]
    fn incremental_counts_equal_rebuild(ops in proptest::collection::vec(op(), 1..40)) {
        let w = world();
        w.apply(&ops);
        w.assert_domain_invariants();
        w.assert_counts_match_rebuild();
    }
}

#[test]
fn file_info_round_trips() {
    use hydrus_core::time::TimestampMs;
    use hydrus_core::{Mime, Sha256};

    use crate::media::{FileFlags, FileInfo};

    let w = world();
    let info = FileInfo {
        size: 12_345,
        mime: Mime::ImagePng,
        original_mime: Some(Mime::ImageJpeg),
        width: Some(640),
        height: Some(480),
        duration_ms: None,
        num_frames: None,
        has_audio: false,
        num_words: None,
        file_modified: Some(TimestampMs::from_millis(1_600_000_000_123)),
        pixel_hash: Some(Sha256([7; 32])),
        blurhash: Some("LEHV6nWB2yk8pyo0adR*.7kCMdnj".into()),
        flags: FileFlags(FileFlags::EXIF | FileFlags::TRANSPARENCY),
    };
    let mut c = w.writer();
    let id = crate::master::intern_hash(c.conn(), &Sha256([1; 32])).unwrap();
    c.add_file_info(id, &info, false).unwrap();
    // an existing record is kept unless overwriting
    let other = FileInfo {
        size: 1,
        ..info.clone()
    };
    c.add_file_info(id, &other, false).unwrap();
    c.add_files(w.roles.local[0], &[(id, Some(5))]).unwrap();
    c.finish().unwrap();
    let batch = crate::media::load(&w.conn, &w.snap.services, None, &[id]).unwrap();
    assert_eq!(batch.results[0].info.as_ref(), Some(&info));
}
