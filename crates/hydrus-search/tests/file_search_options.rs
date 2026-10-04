//! Real database queries and sort eligibility from the File Search recording.
use hydrus_core::{ServiceKey, service::builtin_keys};
use hydrus_search::{
    Clock, FileSearchContext, FileSort, LocationContext, Predicate, SortBy, SortOrder,
    SystemPredicate,
};
use hydrus_store::{
    Store,
    settings::{self, FileSearchSettings},
};

#[test]
fn implicit_limit_and_explicit_override_match_reference_database_queries() {
    let fixture = hydrus_testkit::fixture_json("file_search_limits.json");
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    for case in fixture["queries"].as_array().unwrap() {
        let implicit_limit = case["implicit"].as_u64();
        store
            .write(move |ctx| {
                settings::set(
                    ctx.conn(),
                    &FileSearchSettings {
                        implicit_limit,
                        ..FileSearchSettings::default()
                    },
                )
            })
            .unwrap();
        let mut search = FileSearchContext {
            location: LocationContext::single(ServiceKey::new(builtin_keys::MY_FILES.to_vec())),
            predicates: vec![Predicate::System(SystemPredicate::Everything)],
            ..FileSearchContext::default()
        };
        if let Some(limit) = case["explicit"].as_u64() {
            search
                .predicates
                .push(Predicate::System(SystemPredicate::Limit(limit)));
        }
        let found = store
            .read(|conn| {
                let ids = hydrus_search::search_files(
                    conn,
                    &store.snapshot(),
                    &search,
                    FileSort {
                        by: SortBy::Hash,
                        order: SortOrder::Ascending,
                    },
                    &Clock::system(),
                )
                .unwrap();
                let hashes = hydrus_store::master::hashes(conn, &ids)?;
                let mut hex = hashes
                    .values()
                    .map(hydrus_core::Sha256::to_hex)
                    .collect::<Vec<_>>();
                hex.sort();
                Ok(hex)
            })
            .unwrap();
        assert_eq!(
            found,
            case["hashes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|hash| hash.as_str().unwrap().to_owned())
                .collect::<Vec<_>>(),
            "implicit={implicit_limit:?}, explicit={}",
            case["explicit"]
        );
    }
}

#[test]
fn database_sort_eligibility_matches_each_reference_system_sort_and_location() {
    let fixture = hydrus_testkit::fixture_json("file_search_limits.json");
    for case in fixture["sort_eligibility"].as_array().unwrap() {
        let location = LocationContext::single(ServiceKey::new(
            if case["local"].as_bool().unwrap() {
                builtin_keys::MY_FILES
            } else {
                builtin_keys::COMBINED_FILE
            }
            .to_vec(),
        ));
        let sort = SortBy::from_code(case["code"].as_i64().unwrap()).unwrap();
        assert_eq!(
            sort.can_sort_at_database_level(&location),
            case["can_sort_at_db"].as_bool().unwrap(),
            "{sort:?}, {location:?}"
        );
    }
}
