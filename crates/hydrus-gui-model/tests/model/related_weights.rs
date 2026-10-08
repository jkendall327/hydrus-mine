use hydrus_gui_model::related_weights::{DUPLICATE, Editor, RESERVED};
use hydrus_store::related_tags::{Query, Weights, rank};
use std::collections::{BTreeMap, BTreeSet};

// leaf: audit-options-nested-tag-suggestions-weights
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
    assert_eq!(editor.rows()[editor.selection()[0]].0, "probe:");
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
    assert_eq!(editor.rows()[editor.selection()[0]].0, "probe:");
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

// leaf: audit-options-nested-tag-suggestions-weights
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
        tags.add_side_suggestions(&[tag.into()]).unwrap();
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

#[test]
fn options_acceptance_merges_only_weights_and_preserves_live_unrelated_preferences() {
    use hydrus_gui_model::options::{Editor as Options, Settings};
    use hydrus_store::{related_tags::Settings as Related, settings};
    let (_dirs, store) =
        super::options_dialog::fixture_store(&hydrus_testkit::fixture_json("options_dialog.json"));
    let before = store.read(Settings::load).unwrap();
    let mut options = Options::new(before.clone());
    let mut weights = options.edited_related_weights();
    weights.search.push(("parity:".into(), 10_000));
    options.set_related_weights(weights.clone());
    assert_eq!(
        store.read(settings::get::<Related>).unwrap().weights,
        before.related_tags.weights,
        "discarded parent draft does not persist"
    );
    store
        .write(|ctx| {
            let mut current: Related = settings::get(ctx.conn())?;
            current.enabled = false;
            current.concurrence_percent = 11;
            settings::set(ctx.conn(), &current)
        })
        .unwrap();
    let (after, before, problems) = options.applied();
    assert!(problems.is_empty());
    let before = before.clone();
    store
        .write(move |ctx| after.save(ctx.conn(), &before))
        .unwrap();
    let reopened: Related = hydrus_store::Store::open(store.dir())
        .unwrap()
        .read(settings::get)
        .unwrap();
    assert_eq!(reopened.weights, weights);
    assert!(!reopened.enabled);
    assert_eq!(reopened.concurrence_percent, 11);
}

#[test]
fn binary64_result_rounding_and_sibling_contexts_replay_the_actual_db() {
    use hydrus_core::{Sha256, Tag};
    use hydrus_store::{
        content::{
            MappingAction,
            tag_relations::{self, RelationAction, RelationUpdate},
        },
        display::RelationKind,
    };
    let (_dirs, store) =
        super::options_dialog::fixture_store(&hydrus_testkit::fixture_json("options_dialog.json"));
    let (service, key) = {
        let snapshot = store.snapshot();
        let service = snapshot.services.by_name("second tags").unwrap();
        (service.id, service.key.clone())
    };
    store
        .write_content(move |writer| {
            let files: Vec<_> = (0..100u8)
                .map(|i| hydrus_store::master::intern_hash(writer.conn(), &Sha256([i; 32])))
                .collect::<hydrus_store::Result<_>>()?;
            for (tag, files) in [
                ("round:search", &files[..1]),
                ("round:alias", &files[..1]),
                ("round:result", files.as_slice()),
            ] {
                let tag = hydrus_store::master::intern_tag(writer.conn(), &Tag::new(tag).unwrap())?;
                writer.update_mappings(service, &MappingAction::Add, tag, files)?;
            }
            let local: Vec<hydrus_core::HashId> = writer
                .conn()
                .prepare("SELECT hash_id FROM files ORDER BY hash_id LIMIT 2")?
                .query_map([], |row| row.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            assert_eq!(local.len(), 2);
            for (tag, file) in [
                ("scope:ideal", local[0]),
                ("scope:alias", local[1]),
                ("scope:direct", local[0]),
                ("scope:aliasresult", local[1]),
            ] {
                let tag = hydrus_store::master::intern_tag(writer.conn(), &Tag::new(tag).unwrap())?;
                writer.update_mappings(service, &MappingAction::Add, tag, &[file])?;
            }
            Ok(())
        })
        .unwrap();
    tag_relations::apply(
        &store,
        RelationKind::Siblings,
        ["round", "scope"]
            .into_iter()
            .map(|namespace| RelationUpdate {
                service,
                left: Tag::new(&format!("{namespace}:alias")).unwrap(),
                right: Tag::new(&format!(
                    "{namespace}:{}",
                    if namespace == "round" {
                        "search"
                    } else {
                        "ideal"
                    }
                ))
                .unwrap(),
                action: RelationAction::Add,
            })
            .collect(),
    )
    .unwrap();
    let f = hydrus_testkit::fixture_json("related_tag_weights.json");
    for case in f["rounding"].as_array().unwrap() {
        let request = Query {
            service: key.clone(),
            searches: serde_json::from_value(case["searches"].clone()).unwrap(),
            local: false,
            display: case["display"].as_bool().unwrap(),
            weights: Weights {
                search: vec![(String::new(), 0), (":".into(), 0), ("round:".into(), 100)],
                result: vec![(String::new(), 0), (":".into(), 0), ("round:".into(), 29)],
            },
            concurrence_percent: 6,
        };
        let result = hydrus_store::related_tags::query(&store, &request, &|| false).unwrap();
        assert_eq!(serde_json::json!(result), case["rows"]);
        let rounding = result
            .iter()
            .find(|row| row.tag == "round:result")
            .unwrap()
            .score;
        assert!(
            matches!(rounding, 28 | 57),
            "binary64 multiplication truncates below the exact rational integer"
        );
    }
    for case in f["scope"].as_array().unwrap() {
        let request = Query {
            service: key.clone(),
            searches: vec!["scope:alias".into(), "scope:ideal".into()],
            local: case["local"].as_bool().unwrap(),
            display: case["display"].as_bool().unwrap(),
            weights: Weights {
                search: vec![(String::new(), 0), (":".into(), 0), ("scope:".into(), 100)],
                result: vec![(String::new(), 0), (":".into(), 0), ("scope:".into(), 100)],
            },
            concurrence_percent: 6,
        };
        let result = hydrus_store::related_tags::query(&store, &request, &|| false).unwrap();
        assert_eq!(
            serde_json::json!(result),
            case["rows"],
            "local={} display={}",
            request.local,
            request.display
        );
    }
}

fn selected_weight_rows(editor: &Editor) -> Vec<(String, u16)> {
    editor
        .selection()
        .into_iter()
        .map(|index| editor.rows()[index].clone())
        .collect()
}

#[test]
fn folded_header_sorts_add_selection_and_independent_tables_match_actual_qt() {
    let f = hydrus_testkit::fixture_json("related_weight_table.json");
    let mut editor = Editor::new(Weights {
        search: serde_json::from_value(f["initial"].clone()).unwrap(),
        result: serde_json::from_value(f["initial_result"].clone()).unwrap(),
    });
    for event in f["events"].as_array().unwrap() {
        match event["action"].as_str().unwrap() {
            "initial" => {}
            "select both" => {
                let index = editor
                    .rows()
                    .iter()
                    .position(|(slice, _)| slice == "alpha:")
                    .unwrap();
                editor.click(index, false, false);
                editor.choose(true);
                let index = editor
                    .rows()
                    .iter()
                    .position(|(slice, _)| slice == "Zulu:")
                    .unwrap();
                editor.click(index, false, false);
                editor.choose(false);
            }
            "search weight ascending" => editor.sort_by(1, true),
            "search weight descending" => editor.sort_by(1, false),
            "add search replaces selection" => editor.add("bravo:".into(), 200),
            "result slice descending" => {
                editor.choose(true);
                editor.sort_by(0, false);
                editor.choose(false);
            }
            "search slice ascending" => editor.sort_by(0, true),
            "search slice descending" => editor.sort_by(0, false),
            action => panic!("unhandled recorded action: {action}"),
        }
        assert_eq!(
            serde_json::json!(editor.weights.search),
            event["search"],
            "{}",
            event["action"]
        );
        assert_eq!(
            serde_json::json!(editor.weights.result),
            event["result"],
            "{}",
            event["action"]
        );
        assert_eq!(
            serde_json::json!(selected_weight_rows(&editor)),
            event["search_selected"]
        );
        let search_sort = (editor.sort_column(), editor.ascending());
        editor.choose(true);
        assert_eq!(
            serde_json::json!(selected_weight_rows(&editor)),
            event["result_selected"]
        );
        editor.choose(false);
        assert_eq!((editor.sort_column(), editor.ascending()), search_sort);
    }
    assert_eq!(
        serde_json::json!([editor.weights.search, editor.weights.result]),
        f["saved"]
    );
}
