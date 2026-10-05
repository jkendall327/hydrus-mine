//! A duplicates page's filtering tab: the search edits, the pairs
//! `_SetDuplicates` makes, its questions, and counts on the basic client.
use hydrus_core::HashId;
use hydrus_core::duplicates::{DuplicatesSearch, PairSearchKind, PixelDuplicates};
use hydrus_core::pages::DuplicatesPage;
use hydrus_core::search::context::{FileSearchContext, LocationContext};
use hydrus_core::service::builtin_keys;
use hydrus_gui_model::duplicates_filtering::{
    Which, add_predicates, count, count_text, directions, pairs, question, random_group,
    remove_predicate, second_search_shown,
};
use hydrus_store::duplicates::PairRelationship;

fn page() -> DuplicatesPage {
    let search = FileSearchContext {
        location: LocationContext::new(
            [hydrus_core::ServiceKey::new(
                builtin_keys::COMBINED_LOCAL_FILE_DOMAINS,
            )],
            [],
        ),
        ..FileSearchContext::default()
    };
    DuplicatesPage::new(DuplicatesSearch {
        search_1: search.clone(),
        search_2: search,
        kind: PairSearchKind::OneFileMatchesOneSearch,
        pixel_duplicates: PixelDuplicates::Allowed,
        max_hamming_distance: 8,
    })
}

#[test]
fn edits_and_texts_follow_the_reference() {
    let mut p = page();
    assert!(!second_search_shown(&p));
    p.search.kind = PairSearchKind::BothFilesMatchDifferentSearches;
    assert!(second_search_shown(&p));
    add_predicates(&mut p, Which::Second, "system:inbox").unwrap();
    assert_eq!(p.search.search_2.predicates.len(), 1);
    assert!(remove_predicate(&mut p, Which::Second, 0).is_some());
    assert!(add_predicates(&mut p, Which::First, "system:nonsense here").is_err());
    assert_eq!(count_text(0, 0), "no potential pairs in this file domain!");
    assert_eq!(count_text(1234, 5), "1,234 pairs searched; 5 match");
    assert_eq!(
        directions(hydrus_core::duplicates::PairOrder::Similarity)[0],
        ("most similar first", true)
    );
    let files: Vec<HashId> = (1..=4).map(HashId::from).collect();
    assert_eq!(pairs(&files, PairRelationship::SameQuality).len(), 3);
    assert_eq!(pairs(&files, PairRelationship::Alternate).len(), 6);
    assert_eq!(
        question(PairRelationship::SameQuality, false, 4, 3).0,
        "Are you sure you want to apply \"same quality\" (with default duplicate metadata merge options) for the 4 selected files?"
    );
    assert_eq!(
        question(PairRelationship::Alternate, false, 4, 6).0,
        "Are you sure you want to apply \"alternates\" for the 4 selected files? The relationship will be applied between every pair combination in the file selection (6 pairs)."
    );
    let (_, yes, no) = question(PairRelationship::FalsePositive, false, 20, 190);
    assert_eq!((yes, no), ("I know what I am doing", "step back for now"));
}

#[test]
fn counts_and_random_groups_read_the_basic_client() {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = hydrus_store::Store::open(dir.path()).unwrap();
    let p = page();
    let (total, matching) = count(&store, &p).unwrap();
    assert!(matching <= total);
    let group = random_group(&store, &p).unwrap();
    assert!(total == 0 || group.len() >= 2);
}

#[test]
fn resync_drops_pairs_of_files_out_of_local_storage() {
    use hydrus_store::similar::{resync_potentials_to_local_storage, resync_text};
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = hydrus_store::Store::open(dir.path()).unwrap();
    // two groups whose best files were never in local storage
    let cleared = store
        .write(|ctx| {
            ctx.conn().execute_batch(
                "INSERT INTO dup_groups (group_id, king_hash_id) VALUES (900001, 900001), (900002, 900002);
                 INSERT INTO potential_pairs (smaller_group_id, larger_group_id, distance) VALUES (900001, 900002, 0);
                 INSERT INTO similar_search_status (hash_id, searched_distance) VALUES (900001, 0);",
            )?;
            resync_potentials_to_local_storage(ctx.conn())
        })
        .unwrap();
    assert_eq!(cleared, 2);
    let (pairs, searching): (i64, i64) = store
        .read(|conn| {
            Ok((
                conn.query_row(
                    "SELECT COUNT(*) FROM potential_pairs WHERE smaller_group_id = 900001",
                    [],
                    |r| r.get(0),
                )?,
                conn.query_row(
                    "SELECT COUNT(*) FROM similar_search_status WHERE hash_id = 900001",
                    [],
                    |r| r.get(0),
                )?,
            ))
        })
        .unwrap();
    assert_eq!((pairs, searching), (0, 0));
    assert_eq!(
        store
            .write(|ctx| resync_potentials_to_local_storage(ctx.conn()))
            .unwrap(),
        0
    );
    assert_eq!(resync_text(0), "Done! No orphan pairs found!");
    assert_eq!(
        resync_text(2),
        "Done! Pairs for 2 out-of-domain files cleared out."
    );
}
