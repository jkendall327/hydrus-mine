use hydrus_gui_model::related_weights::{DUPLICATE, Editor, RESERVED};
use hydrus_store::related_tags::{Query, Weights, rank};
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn namespace_questions_and_detached_tables_match_actual_qt() {
    let f = hydrus_testkit::fixture_json("related_tag_weights.json");
    let mut editor = Editor::new(Weights::default());
    assert_eq!(
        serde_json::json!(editor.weights.search),
        f["events"][0]["search"]
    );
    assert_eq!(
        serde_json::json!(editor.weights.result),
        f["events"][0]["result"]
    );
    assert_eq!(editor.namespace(""), Err(RESERVED.into()));
    assert_eq!(editor.namespace(":"), Err(RESERVED.into()));
    assert_eq!(editor.namespace("creator"), Err(DUPLICATE.into()));
    let slice = editor.namespace("probe").unwrap();
    editor.add(slice, 10_000);
    assert_eq!(
        serde_json::json!(editor.weights.search),
        f["events"][5]["search"]
    );
    let probe = editor
        .rows()
        .iter()
        .position(|(s, _)| s == "probe:")
        .unwrap();
    editor.edit(probe, 0);
    assert_eq!(
        serde_json::json!(editor.weights.search),
        f["events"][6]["search"]
    );
    let protected = editor
        .rows()
        .iter()
        .position(|(s, _)| s.is_empty())
        .unwrap();
    editor.click(protected, false, false);
    assert!(!editor.can_delete());
    editor.delete();
    let probe = editor
        .rows()
        .iter()
        .position(|(s, _)| s == "probe:")
        .unwrap();
    editor.click(probe, false, false);
    assert!(editor.can_delete());
    editor.delete();
    editor.choose(true);
    editor.add("probe:".into(), 333);
    assert_eq!(
        serde_json::json!([editor.weights.search, editor.weights.result]),
        f["saved"]
    );
    assert_eq!(f["saved"], f["reopened"]);
    for prompt in f["events"].as_array().unwrap().last().unwrap()["prompts"]
        .as_array()
        .unwrap()
    {
        if prompt.get("minimum").is_some() {
            assert_eq!(prompt["minimum"], 0);
            assert_eq!(prompt["maximum"], 10_000);
        }
    }
}

#[test]
fn search_and_result_weights_change_actual_qt_cosine_ranking_separately() {
    let f = hydrus_testkit::fixture_json("related_tag_weights.json");
    let files: BTreeMap<String, BTreeSet<hydrus_core::HashId>> = [
        ("source:seed", vec![0, 1, 2, 3]),
        ("context:seed", vec![2, 3, 4, 5]),
        ("alpha:first", vec![0, 1, 2]),
        ("beta:second", vec![3, 4, 5]),
        ("alpha:third", vec![4, 5]),
        ("zero:hidden", vec![0, 1]),
    ]
    .into_iter()
    .map(|(tag, files)| {
        (
            tag.into(),
            files.into_iter().map(hydrus_core::HashId).collect(),
        )
    })
    .collect();
    for case in f["ranking"].as_array().unwrap() {
        let request = Query {
            service: hydrus_core::ServiceKey::new(Vec::new()),
            searches: serde_json::from_value(case["searches"].clone()).unwrap(),
            local: true,
            display: false,
            concurrence_percent: 6,
            weights: Weights {
                search: serde_json::from_value(case["search_weights"].clone()).unwrap(),
                result: serde_json::from_value(case["result_weights"].clone()).unwrap(),
            },
        };
        let result = rank(&files, &request, &|| false);
        if case["name"] == "zero all" {
            assert!(result.is_empty());
        } else {
            assert_eq!(serde_json::json!(result), case["rows"], "{}", case["name"]);
        }
        assert!(rank(&files, &request, &|| true).is_empty());
    }
}

#[test]
fn real_service_mappings_replay_recorded_ranks_and_never_cross_service() {
    use hydrus_gui_model::manage_tags::ManageTags;
    let (_dirs, store) =
        super::options_dialog::fixture_store(&hydrus_testkit::fixture_json("options_dialog.json"));
    let files: Vec<hydrus_core::HashId> = store
        .read(|conn| {
            Ok(conn
                .prepare("SELECT hash_id FROM files ORDER BY hash_id LIMIT 6")?
                .query_map([], |row| row.get(0))?
                .collect::<rusqlite::Result<_>>()?)
        })
        .unwrap();
    assert_eq!(files.len(), 6);
    let mut service = None;
    for (tag, indices) in [
        ("source:seed", vec![0, 1, 2, 3]),
        ("context:seed", vec![2, 3, 4, 5]),
        ("alpha:first", vec![0, 1, 2]),
        ("beta:second", vec![3, 4, 5]),
        ("alpha:third", vec![4, 5]),
        ("zero:hidden", vec![0, 1]),
    ] {
        let mut tags = ManageTags::new(
            store.clone(),
            indices.into_iter().map(|i| files[i]).collect(),
        )
        .unwrap();
        let index = tags
            .service_names()
            .iter()
            .position(|name| name == "second tags")
            .unwrap();
        tags.choose_service(index).unwrap();
        service = tags.migration_service_key();
        tags.add_side_suggestions(&[tag.into()]);
        tags.apply().unwrap();
    }
    let f = hydrus_testkit::fixture_json("related_tag_weights.json");
    for case in f["ranking"].as_array().unwrap() {
        let request = Query {
            service: service.clone().unwrap(),
            searches: serde_json::from_value(case["searches"].clone()).unwrap(),
            local: true,
            display: false,
            weights: Weights {
                search: serde_json::from_value(case["search_weights"].clone()).unwrap(),
                result: serde_json::from_value(case["result_weights"].clone()).unwrap(),
            },
            concurrence_percent: 6,
        };
        let rows = hydrus_store::related_tags::query(&store, &request, &|| false).unwrap();
        if case["name"] == "zero all" {
            assert!(rows.is_empty());
        } else {
            assert_eq!(serde_json::json!(rows), case["rows"]);
        }
        let mut other = request.clone();
        other.service =
            hydrus_core::ServiceKey::new(hydrus_core::service::builtin_keys::MY_TAGS.to_vec());
        assert!(
            hydrus_store::related_tags::query(&store, &other, &|| false)
                .unwrap()
                .is_empty()
        );
    }
}
