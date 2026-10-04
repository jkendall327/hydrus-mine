//! Native service facts match the real reference service review fetchers.
use hydrus_gui_model::services_review;
use hydrus_store::Store;

#[test]
fn review_matches_reference_statistics() {
    let recorded = hydrus_testkit::fixture_json("services.json");
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    let rows = services_review::rows(&store).unwrap();
    for recorded in recorded["rows"].as_array().unwrap() {
        let row = rows
            .iter()
            .find(|r| r.key.to_hex() == recorded["key"].as_str().unwrap())
            .unwrap();
        assert_eq!(row.name, recorded["row"][0]);
        assert_eq!(row.service_type, recorded["row"][1]);
        if let Some(statistics) = recorded["statistics"].as_str() {
            assert_eq!(row.statistics, statistics, "{}", row.name);
        }
    }
}

#[test]
fn local_bulk_actions_match_recorded_confirmations_and_reject_wrong_service() {
    use hydrus_gui_model::services_review::Action;
    use hydrus_store::content::RatingClearScope;
    let fixture = hydrus_testkit::fixture_json("service_bulk.json");
    for event in fixture["events"].as_array().unwrap() {
        let action = match event["action"].as_str().unwrap() {
            "clear" => Action::ClearTrash,
            "undelete" => Action::UndeleteTrash,
            "delete_for_deleted_files" => Action::ClearRatings(RatingClearScope::Deleted),
            "delete_for_non_local_files" => Action::ClearRatings(RatingClearScope::NonLocal),
            "delete_for_all_files" => Action::ClearRatings(RatingClearScope::All),
            unknown => panic!("unrecorded action {unknown}"),
        };
        assert_eq!(action.question(), event["asked"][0]["message"]);
        assert_eq!(event["asked"][0]["yes_label"], "do it");
        assert_eq!(event["asked"][0]["no_label"], "forget it");
    }
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    let before = services_review::rows(&store).unwrap();
    let tags = store
        .snapshot()
        .services
        .by_name("my tags")
        .unwrap()
        .key
        .clone();
    for index in 0..=4 {
        assert!(
            services_review::apply(&store, tags.clone(), Action::from_index(index).unwrap())
                .is_err()
        );
    }
    assert_eq!(services_review::rows(&store).unwrap(), before);
    assert!(Action::from_index(5).is_none());
}
