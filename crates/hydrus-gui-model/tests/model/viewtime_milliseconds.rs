//! Actual Qt viewtime fields and DB search results, including fractional edges.
use std::sync::Arc;

use hydrus_core::{Sha256, content::CanvasType, search::predicate::ViewingStat};
use hydrus_gui_model::predicate_editors::{Blank, Context, Editor};
use hydrus_search::{
    CivilDateTime, Clock, FileSearchContext, FileSort, LocationContext, Predicate, SortBy,
    SortOrder, TextContext, predicate_text,
};
use hydrus_store::Store;
use serde_json::Value;

fn seeded(fixture: &Value) -> (tempfile::TempDir, Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    let seeds = fixture["seeds"].as_array().unwrap().clone();
    store
        .write_content(move |writer| {
            for seed in seeds {
                let hash: Sha256 = seed["hash"].as_str().unwrap().parse().unwrap();
                let id = hydrus_store::master::hash_id(writer.conn(), &hash)?.unwrap();
                let canvas = match seed["canvas"].as_str().unwrap() {
                    "media" => CanvasType::MediaViewer,
                    "preview" => CanvasType::Preview,
                    "client api" => CanvasType::ClientApi,
                    other => panic!("unknown canvas {other}"),
                };
                writer.set_views(
                    id,
                    canvas,
                    Some(1_700_000_000_000),
                    1,
                    seed["viewtime_ms"].as_i64().unwrap(),
                )?;
            }
            Ok(())
        })
        .unwrap();
    (dir, store)
}

// leaf: audit-options-predicate-file-viewing-statistics-fileviewingstatsviewtime-test
#[test]
fn editor_and_imported_predicates_match_reference_millisecond_queries() {
    let fixture = hydrus_testkit::fixture_json("viewtime_milliseconds.json");
    let (_dir, store) = seeded(&fixture);
    let context = Context::new(
        &store.snapshot().services,
        Vec::new(),
        CivilDateTime::new(2026, 10, 4, 0, 0).unwrap(),
    );
    let text = TextContext::from_store(
        &store.snapshot().services,
        &hydrus_store::settings::FileViewingStatistics::default(),
    );
    let mut checked = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let editor = Editor::new(Blank::FileViewingStats, &context);
        let mut panel = editor.pages[0].panels[1].clone();
        let locations = case["locations"].as_array().unwrap();
        for (index, name) in ["media", "preview", "client api"].iter().enumerate() {
            panel.tick(
                1,
                index,
                locations.iter().any(|value| value.as_str() == Some(*name)),
            );
        }
        let operator = ["<", "\u{2248}", "=", ">"]
            .iter()
            .position(|value| Some(*value) == case["operator"].as_str())
            .unwrap();
        panel.choose(2, operator);
        let milliseconds = case["milliseconds"].as_i64().unwrap();
        let mut remaining = milliseconds;
        for (index, scale) in [86_400_000, 3_600_000, 60_000, 1000, 1].iter().enumerate() {
            panel.set_number(3 + index, remaining / scale);
            remaining %= scale;
        }
        let made = panel.predicates(&context).unwrap();
        assert_eq!(made.len(), 1);
        assert_eq!(
            predicate_text(&made[0], &text),
            case["text"].as_str().unwrap(),
            "{case}"
        );
        let stored = hydrus_legacy::serialisable::SerialisableObject::from_tuple_str(
            &case["serialised"].to_string(),
        )
        .unwrap();
        let imported = hydrus_legacy::objects::predicates::predicate(&stored).unwrap();
        assert_eq!(made[0], imported, "{case}");
        let saved = serde_json::to_string(&made[0]).unwrap();
        assert_eq!(serde_json::from_str::<Predicate>(&saved).unwrap(), made[0]);
        let search = FileSearchContext {
            location: LocationContext::single(hydrus_core::ServiceKey::new(
                hydrus_core::service::builtin_keys::MY_FILES.to_vec(),
            )),
            predicates: made,
            ..FileSearchContext::default()
        };
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
                let mut hashes = hydrus_store::master::hashes(conn, &ids)?
                    .values()
                    .map(Sha256::to_hex)
                    .collect::<Vec<_>>();
                hashes.sort();
                Ok(hashes)
            })
            .unwrap();
        let expected: Vec<_> = case["hashes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap().to_owned())
            .collect();
        assert_eq!(found, expected, "{case}");
        checked += 1;
    }
    assert_eq!(checked, 96);
    // Existing stored whole-second values keep their original enum and unit.
    assert_eq!(
        ViewingStat::from_viewtime_milliseconds(1000),
        (ViewingStat::ViewTime, 1)
    );
}
