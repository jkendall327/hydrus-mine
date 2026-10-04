//! Exact Qt child summaries/pairs, live text memory and per-file staged Apply.
#[path = "../support/manage_tag_counts.rs"]
mod fixture;
use hydrus_core::ContentStatus;
use hydrus_gui_model::{incremental_tagging::IncrementalTagging, manage_tags::ManageTags};
use serde_json::{Value, json};
fn state(editor: &IncrementalTagging, files: &[hydrus_core::HashId]) -> Value {
    let updates:Vec<_>=editor.pairs().into_iter().map(|(file,tag)|json!({"service":"my tags","action":0,"tag":tag,"files":[files.iter().position(|f|*f==file).unwrap()]})).collect();
    json!({"namespace":editor.namespace,"prefix":editor.prefix,"suffix":editor.suffix,"start":editor.start,"step":editor.step,"reverse":editor.reverse,"summary":editor.summary(),"updates":updates})
}
#[test]
fn actual_child_defaults_conflicts_reverse_pairs_and_nested_cancellation_replay() {
    let recorded = hydrus_testkit::fixture_json("manage_tag_counts_incremental.json");
    let (_directory, store, files) = fixture::seed(&recorded);
    let mut prior = ManageTags::new(store.clone(), files.clone()).unwrap();
    prior.enter("checkpoint:old").unwrap();
    prior.enter("checkpoint:live").unwrap();
    prior.apply().unwrap();
    for case in recorded["incremental"].as_array().unwrap() {
        let mut parent = ManageTags::new(store.clone(), files.clone()).unwrap();
        fixture::mine(&mut parent);
        let before = fixture::tags(&store, &files, "my tags", ContentStatus::Current);
        let mut editor = parent.incremental().unwrap();
        assert_eq!(state(&editor, &files), case["child"][0]["state"]);
        let input = &case["input"];
        for (field, key) in ["namespace", "prefix", "suffix"].into_iter().enumerate() {
            editor
                .set_text(field, input[key].as_str().unwrap())
                .unwrap();
        }
        editor
            .set_numbers(
                i32::try_from(input["start"].as_i64().unwrap()).unwrap(),
                i32::try_from(input["step"].as_i64().unwrap()).unwrap(),
                input["reverse"].as_bool().unwrap(),
            )
            .unwrap();
        assert_eq!(state(&editor, &files), case["child"][1]["state"]);
        let valid = state(&editor, &files);
        assert!(editor.set_numbers(10_000_001, 1, false).is_err());
        assert!(editor.set_numbers(1, -10_001, false).is_err());
        assert!(editor.set_text(99, "invalid field").is_err());
        assert_eq!(
            state(&editor, &files),
            valid,
            "invalid controls preserve the accepted preview"
        );
        if input["accept"].as_bool().unwrap() {
            parent
                .apply_incremental(parent.service(), editor.cleaned_pairs().unwrap())
                .unwrap();
            assert_eq!(
                fixture::tags(&store, &files, "my tags", ContentStatus::Current),
                before,
                "child Apply is staged"
            );
            if input["name"].as_str().unwrap().ends_with("parent_apply") {
                parent.apply().unwrap();
            }
        } else {
            assert!(!parent.has_changes());
        }
        let memory: hydrus_store::tag_editing::ManageTagsSettings =
            store.read(hydrus_store::settings::get).unwrap();
        assert_eq!(
            json!({"namespace":memory.incremental_namespace,"prefix":memory.incremental_prefix,"suffix":memory.incremental_suffix}),
            case["preferences"]
        );
        assert_eq!(
            json!(fixture::tags(
                &store,
                &files,
                "my tags",
                ContentStatus::Current
            )),
            case["reopened_tags"]
        );
        drop(parent);
    }
    let mut parent = ManageTags::new(store.clone(), files.clone()).unwrap();
    let editor = parent.incremental().unwrap();
    let service = parent.service();
    let other = parent
        .service_names()
        .iter()
        .position(|name| name == "second tags")
        .unwrap();
    parent.choose_service(other).unwrap();
    assert!(
        parent
            .apply_incremental(service, editor.cleaned_pairs().unwrap())
            .is_err()
    );
    assert!(!parent.has_changes());
    let single = ManageTags::new(store, vec![files[0]]).unwrap();
    assert!(single.incremental().is_none());
    let _ = fixture::snapshot(&mut parent);
}
#[test]
fn add_then_remove_retains_the_reference_deleted_mapping_in_private_preview() {
    let recorded = hydrus_testkit::fixture_json("manage_tag_counts_incremental.json");
    let (_directory, store, files) = fixture::seed(&recorded);
    let mut parent = ManageTags::new(store.clone(), files.clone()).unwrap();
    parent.enter("checkpoint:old").unwrap();
    parent.enter("checkpoint:live").unwrap();
    parent.apply().unwrap();
    let mut parent = ManageTags::new(store.clone(), files.clone()).unwrap();
    parent.flip_show_deleted().unwrap();
    parent.enter("checkpoint:cycle").unwrap();
    parent.enter("checkpoint:cycle").unwrap();
    assert_eq!(
        parent.has_changes(),
        recorded["fresh_cycle"]["has_changes"].as_bool().unwrap()
    );
    assert_eq!(
        fixture::snapshot(&mut parent),
        recorded["fresh_cycle"]["state"]
    );
    assert!(
        !fixture::tags(&store, &files, "my tags", ContentStatus::Deleted)
            .iter()
            .any(|tags| tags.iter().any(|tag| tag == "checkpoint:cycle"))
    );
    parent.apply().unwrap();
    assert!(
        fixture::tags(&store, &files, "my tags", ContentStatus::Deleted)
            .iter()
            .all(|tags| tags.iter().any(|tag| tag == "checkpoint:cycle"))
    );
}

#[test]
fn initial_start_uses_actual_unicode_decimal_sorting_and_ignores_negative_subtags() {
    let recorded = hydrus_testkit::fixture_json("manage_tag_counts_incremental.json");
    let (_directory, store, files) = fixture::seed(&recorded);
    let current =
        recorded["incremental"].as_array().unwrap().last().unwrap()["reopened_tags"].clone();
    store
        .write(|ctx| {
            hydrus_store::settings::set(
                ctx.conn(),
                &hydrus_store::tag_editing::ManageTagsSettings {
                    incremental_namespace: "page".into(),
                    incremental_prefix: "v".into(),
                    incremental_suffix: "x".into(),
                    ..hydrus_store::tag_editing::ManageTagsSettings::default()
                },
            )
        })
        .unwrap();
    for case in recorded["initial_cases"].as_array().unwrap() {
        let mut tags: std::collections::BTreeMap<_, std::collections::BTreeSet<String>> = files
            .iter()
            .enumerate()
            .map(|(i, file)| {
                (
                    *file,
                    current[i]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|tag| tag.as_str().unwrap().to_owned())
                        .collect(),
                )
            })
            .collect();
        tags.insert(
            files[0],
            case["subtags"]
                .as_array()
                .unwrap()
                .iter()
                .map(|subtag| format!("page:{}", subtag.as_str().unwrap()))
                .collect(),
        );
        let mut editor = IncrementalTagging::new(store.clone(), files.clone(), tags);
        assert_eq!(
            i64::from(editor.start),
            case["initial_start"].as_i64().unwrap(),
            "{:?}",
            case["subtags"]
        );
        assert_eq!(editor.summary(), case["summary"].as_str().unwrap());
        editor.set_numbers(editor.start, 0, false).unwrap();
        assert_eq!(
            editor.summary(),
            case["zero_step_summary"].as_str().unwrap()
        );
    }
    for zero in recorded["decimal_zeros"].as_array().unwrap() {
        let zero = u32::try_from(zero.as_u64().unwrap()).unwrap();
        for value in 0..10 {
            assert_eq!(
                hydrus_core::tag_presentation::decimal_digit(char::from_u32(zero + value).unwrap()),
                Some(value)
            );
        }
    }
    assert_eq!(hydrus_core::tag_presentation::decimal_digit('²'), None);
    assert_eq!(hydrus_core::tag_presentation::decimal_digit('-'), None);
}

#[test]
fn actual_qt_initial_clamps_and_long_decimal_previews_preserve_safe_native_inference() {
    let corpus = hydrus_testkit::fixture_json("manage_tag_counts_incremental.json");
    let recorded = hydrus_testkit::fixture_json("incremental_number_boundaries.json");
    let (_directory, store, files) = fixture::seed(&corpus);
    let files = files[..2].to_vec();
    for case in recorded["cases"].as_array().unwrap() {
        let input = &case["input"];
        let digits = input["digits"].as_str().map_or_else(
            || {
                format!(
                    "{}{}",
                    input["repeat"]
                        .as_str()
                        .unwrap()
                        .repeat(usize::try_from(input["count"].as_u64().unwrap()).unwrap()),
                    input["tail"].as_str().unwrap()
                )
            },
            str::to_owned,
        );
        let current = [(files[0], [format!("page:{digits}")].into_iter().collect())]
            .into_iter()
            .collect();
        let mut editor = IncrementalTagging::new(store.clone(), files.clone(), current);
        if let Some(start) = case["outcome"]["start"].as_i64() {
            assert_eq!(i64::from(editor.start), start, "{input}");
            assert_eq!(
                editor.summary(),
                case["outcome"]["summary"].as_str().unwrap(),
                "{input}"
            );
            for edit in case["clamp_edits"].as_array().unwrap() {
                let start = i32::try_from(edit["actual"][0].as_i64().unwrap()).unwrap();
                let step = i32::try_from(edit["actual"][1].as_i64().unwrap()).unwrap();
                editor.set_numbers(start, step, false).unwrap();
                assert_eq!((editor.start, editor.step), (start, step));
            }
        } else {
            // Qt cannot construct its panel for these literal previews. Native
            // keeps the usable value rather than copying that reference defect.
            let expected = match case["outcome"]["error_type"].as_str().unwrap() {
                "OverflowError" => 10_000_000,
                "ValueError" => 7,
                other => panic!("unexpected actual reference failure {other}"),
            };
            assert_eq!(editor.start, expected, "{input}");
            assert_eq!(editor.pairs().len(), files.len());
            assert_eq!(editor.step, 1);
        }
    }
}
