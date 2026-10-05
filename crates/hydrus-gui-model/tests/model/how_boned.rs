//! Database > how boned am I? against `oracle/record_how_boned.py`.
use hydrus_core::ServiceKey;
use hydrus_core::search::context::{FileSearchContext, LocationContext, TagContext};
use hydrus_core::service::builtin_keys;
use hydrus_gui_model::how_boned::{boned, duplicates_tab, files_tab, special_message, views_tab};
use hydrus_store::Store;
use serde_json::Value;

fn search(domain: &[u8]) -> FileSearchContext {
    FileSearchContext {
        location: LocationContext::new([ServiceKey::new(domain)], []),
        tags: TagContext::new(ServiceKey::new(builtin_keys::COMBINED_TAG), true, true),
        predicates: Vec::new(),
    }
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn the_basic_client_s_statistics_read_as_the_reference_s() {
    let fixture = hydrus_testkit::fixture_json("how_boned.json");
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    let now = fixture["now"].as_i64().unwrap();
    let offset = jiff::tz::Offset::from_seconds(
        i32::try_from(fixture["utc_offset_seconds"].as_i64().unwrap()).unwrap(),
    )
    .unwrap();
    for (name, domain) in [
        ("default", builtin_keys::COMBINED_LOCAL_FILE_DOMAINS),
        ("my_files", builtin_keys::MY_FILES),
    ] {
        let recorded = &fixture[name];
        let search = search(domain);
        let stats = boned(&store, &search).unwrap();
        let files = files_tab(&stats, now, offset);
        let rows: Vec<Vec<String>> = files.rows.iter().map(|r| r.to_vec()).collect();
        let recorded_rows: Vec<Vec<String>> = recorded["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(strings)
            .collect();
        assert_eq!(rows, recorded_rows, "{name}");
        assert_eq!(
            files.earliest.as_deref(),
            recorded["earliest"].as_str(),
            "{name}"
        );
        assert_eq!(
            views_tab(&stats).to_vec(),
            strings(&recorded["views"]),
            "{name}"
        );
        assert_eq!(
            duplicates_tab(&stats).to_vec(),
            strings(&recorded["duplicates"]),
            "{name}"
        );
        assert_eq!(
            special_message(&stats, &search),
            recorded["bones_text"].as_str(),
            "{name}"
        );
    }
}
