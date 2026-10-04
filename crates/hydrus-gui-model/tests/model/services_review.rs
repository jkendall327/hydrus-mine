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
