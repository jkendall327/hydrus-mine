//! Actual Qt pruning states, accepted filter removal and independent saved preferences.
use hydrus_core::{HashId, Sha256};
use hydrus_gui_model::{
    archive_delete::ArchiveDeleteFilter,
    file_view_removal::{self, Change},
};
use hydrus_search::LocationContext;
use hydrus_store::{
    Store,
    content::TransferKind,
    settings::{self, FileViewRemoval},
};
fn fixture_store() -> (tempfile::TempDir, std::sync::Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    (dir, store)
}
fn ids(store: &Store, v: &serde_json::Value) -> Vec<HashId> {
    v["hashes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            let hash: Sha256 = s.as_str().unwrap().parse().unwrap();
            store
                .read(|c| hydrus_store::master::hash_id(c, &hash))
                .unwrap()
                .unwrap()
        })
        .collect()
}
fn restore(store: &Store, files: &[HashId], two: bool) {
    let files = files.to_vec();
    store
        .write_content(move |w| {
            let source = w.snapshot().services.by_name("art").unwrap().id;
            let dest = w.snapshot().services.by_name("my files").unwrap().id;
            w.add_files(
                source,
                &files
                    .iter()
                    .map(|&id| (id, Some(1234567890000)))
                    .collect::<Vec<_>>(),
            )?;
            w.delete_files(dest, &files, None)?;
            if two {
                w.add_files(
                    dest,
                    &files
                        .iter()
                        .map(|&id| (id, Some(1234567890000)))
                        .collect::<Vec<_>>(),
                )?;
            }
            Ok(())
        })
        .unwrap();
}
#[test]
fn actual_qt_media_list_trash_move_and_physical_pruning_matrix() {
    let fixture = hydrus_testkit::fixture_json("files_view_removal.json");
    let (_dir, store) = fixture_store();
    let files = ids(&store, &fixture);
    let file = files[0];
    let snap = store.snapshot();
    let source = snap.services.by_name("art").unwrap().id;
    let dest = snap.services.by_name("my files").unwrap().id;
    let roles = hydrus_store::content::DomainRoles::new(&snap.services).unwrap();
    for case in fixture["content_events"].as_array().unwrap() {
        let prefs = FileViewRemoval {
            trashed: case["trash"].as_bool().unwrap(),
            moved: case["move"].as_bool().unwrap(),
            ..Default::default()
        };
        store
            .write(move |ctx| settings::set(ctx.conn(), &prefs))
            .unwrap();
        restore(&store, &[file], case["local_count"] == 2);
        let action = case["action"].as_str().unwrap();
        let change = match action {
            "trash" => {
                store
                    .write_content(move |w| w.delete_files(source, &[file], None))
                    .unwrap();
                Change::Deleted(source)
            }
            "move" => {
                let source_key = snap.services.get(source).unwrap().key.clone();
                let dest_key = snap.services.get(dest).unwrap().key.clone();
                store
                    .write_content(move |w| {
                        w.transfer_local_files(
                            TransferKind::Merge,
                            &dest_key,
                            Some(&source_key),
                            &[file],
                        )
                    })
                    .unwrap();
                Change::Moved(source)
            }
            "physical" => Change::Deleted(roles.local_file_storage),
            _ => panic!("unexpected recorded action"),
        };
        let domains = match case["location"].as_str().unwrap() {
            "source" => vec![source],
            "both" => vec![source, dest],
            "combined" => vec![roles.combined_local_media],
            "storage" => vec![roles.local_file_storage],
            "trash" => vec![roles.trash],
            _ => panic!(),
        };
        let location = LocationContext::new(
            domains
                .iter()
                .map(|id| snap.services.get(*id).unwrap().key.clone()),
            [],
        );
        let removed = file_view_removal::removed(&store, &location, &[file], change);
        assert_eq!(
            removed.is_empty(),
            !case["remaining"].as_array().unwrap().is_empty(),
            "{case}"
        );
    }
}
#[test]
fn filter_removal_matches_recorded_commit_gates_and_skipped_return_file() {
    let fixture = hydrus_testkit::fixture_json("files_view_removal.json");
    let (_dir, store) = fixture_store();
    let files = ids(&store, &fixture);
    for case in fixture["filters"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["outcome"] == "accept")
    {
        restore(&store, &files, false);
        let prefs = FileViewRemoval {
            filtered: case["remove"].as_bool().unwrap(),
            skipped: case["skipped"].as_bool().unwrap(),
            ..Default::default()
        };
        store
            .write(move |ctx| settings::set(ctx.conn(), &prefs))
            .unwrap();
        let source = store
            .snapshot()
            .services
            .by_name("art")
            .unwrap()
            .key
            .clone();
        let mut filter = ArchiveDeleteFilter::new(
            store.clone(),
            files.clone(),
            LocationContext::single(source),
        )
        .unwrap();
        filter.keep();
        filter.delete();
        filter.skip();
        let hashes = store
            .read(|conn| hydrus_store::media::load_basic(conn, &filter.removed_from_view()))
            .unwrap()
            .iter()
            .map(|m| m.hash.to_hex())
            .collect::<std::collections::BTreeSet<_>>();
        let expected: std::collections::BTreeSet<String> = case["removed"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect();
        assert_eq!(hashes, expected);
        let returned = filter.return_file().unwrap();
        let hash = store
            .read(|conn| hydrus_store::media::load_basic(conn, &[returned]))
            .unwrap()[0]
            .hash
            .to_hex();
        assert_eq!(hash, case["current"]);
        filter.commit().unwrap();
        let media = store
            .read(|conn| hydrus_store::media::load_basic(conn, &files))
            .unwrap();
        assert!(!media.iter().find(|m| m.hash_id == files[0]).unwrap().inbox);
    }
}

#[test]
fn filter_content_pruning_reports_only_domain_departures_after_intervening_trash() {
    let fixture = hydrus_testkit::fixture_json("files_view_removal.json");
    let (_dir, store) = fixture_store();
    let files = ids(&store, &fixture);
    restore(&store, &files[..2], false);
    let source = store.snapshot().services.by_name("art").unwrap().clone();
    let location = LocationContext::single(source.key);
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &FileViewRemoval {
                    trashed: true,
                    ..Default::default()
                },
            )
        })
        .unwrap();
    let mut filter =
        ArchiveDeleteFilter::new(store.clone(), files[..2].to_vec(), location.clone()).unwrap();
    filter.delete();
    filter.delete();
    let already_trashed = files[0];
    store
        .write_content(move |w| w.delete_files(source.id, &[already_trashed], None))
        .unwrap();
    let affected = filter.commit_changed().unwrap();
    assert_eq!(affected, vec![files[1]]);
    assert_eq!(
        file_view_removal::deleted(
            &store,
            &location,
            &affected,
            &hydrus_gui_model::media_actions::Deletion::ToTrash
        ),
        vec![files[1]]
    );
    assert!(filter.removed_from_view().is_empty());
}
