//! The duplicates page's filtering tab against the reference's, replaying
//! `oracle/fixtures/duplicates_quick_processing.json`
//! (`oracle/record_duplicates_quick_processing.py`, on the database the
//! reference's auto-resolution run left): the pair sort and direction
//! choosers, the mixed pairs/group mode chooser, the quick processing
//! buttons' labels, and "show some random potential duplicates" followed by
//! the "set current media as ..." buttons with their "Are you sure?" question
//! answered as recorded: what is asked, which files the page then shows, and
//! the relationships the answers leave.

use std::collections::BTreeSet;

use hydrus_core::duplicates::PairOrder;
use hydrus_gui::duplicates_filtering_sidebar::question_opened;
use slint::Model as _;

use crate::auto_resolution_review::{Opened, opened};

fn order_of(sort_type: i64) -> PairOrder {
    match sort_type {
        0 => PairOrder::MaxFilesize,
        1 => PairOrder::Similarity,
        2 => PairOrder::MinFilesize,
        3 => PairOrder::Random,
        other => panic!("sort type {other}"),
    }
}

fn page_state(o: &Opened) -> (PairOrder, bool, bool) {
    let current = o.bound.current.borrow();
    let page = current.borrow();
    let d = page.duplicates().expect("a duplicates page");
    (d.order, d.ascending, d.group_mode)
}

// leaf: audit-media-filter-sidebar-sort-group
#[test]
fn the_filtering_tab_chooses_pair_sort_direction_and_group_mode_as_the_reference_does() {
    let recorded = hydrus_testkit::fixture_json("duplicates_quick_processing.json");
    let _windows = hydrus_gui::headless::init();
    let o = opened();
    let f = o.ui.get_duplicates_filtering();
    let sorts = recorded["sort"].as_array().unwrap();
    assert_eq!(
        f.sorts.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        sorts
            .iter()
            .map(|s| s["label"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>(),
        "the sort choices, in order"
    );
    let groups = recorded["group_mode"].as_array().unwrap();
    assert_eq!(
        f.groups.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        groups
            .iter()
            .map(|g| g["label"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    );
    for (i, sort) in sorts.iter().enumerate() {
        o.ui.invoke_duplicates_filtering_action("sort".into(), i as i32);
        let f = o.ui.get_duplicates_filtering();
        assert_eq!(f.sort, i as i32);
        let order = order_of(sort["type"].as_i64().unwrap());
        assert_eq!(page_state(&o).0, order, "{}", sort["label"]);
        // the direction chooser: shown (not for random) with its choices
        let directions = sort["directions"].as_array().unwrap();
        if sort["direction_shown"] == true {
            assert_eq!(
                f.directions
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>(),
                directions
                    .iter()
                    .map(|d| d["label"].as_str().unwrap().to_owned())
                    .collect::<Vec<_>>(),
                "{}",
                sort["label"]
            );
            for (d, value) in sort["values"].as_array().unwrap().iter().enumerate() {
                o.ui.invoke_duplicates_filtering_action("direction".into(), d as i32);
                let (now, ascending, _) = page_state(&o);
                assert_eq!(now, order);
                assert_eq!(
                    ascending,
                    value[1].as_bool().unwrap(),
                    "{} {}",
                    sort["label"],
                    directions[d]["label"]
                );
                assert_eq!(directions[d]["asc"], value[1]);
            }
        } else {
            assert_eq!(f.directions.row_count(), 0, "random has no direction");
        }
    }
    for (i, group) in groups.iter().enumerate() {
        o.ui.invoke_duplicates_filtering_action("group".into(), i as i32);
        assert_eq!(page_state(&o).2, group["page"].as_bool().unwrap());
        assert_eq!(o.ui.get_duplicates_filtering().group, i as i32);
    }
}

// leaf: audit-media-filter-sidebar-random
#[test]
fn show_random_group_and_the_set_buttons_ask_and_act_as_the_reference_does() {
    let recorded = hydrus_testkit::fixture_json("duplicates_quick_processing.json");
    hydrus_store::duplicates::pin_random_group_choice();
    let _windows = hydrus_gui::headless::init();
    let o = opened();
    let f = o.ui.get_duplicates_filtering();
    // the buttons' labels: the random one is the sidebar's, the others the set buttons
    let labels: Vec<String> = recorded["buttons"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        f.set_buttons
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        labels[1..],
        "same quality, alternates, false positive"
    );
    let shown = |o: &Opened| -> BTreeSet<String> {
        o.bound
            .current
            .borrow()
            .borrow()
            .results()
            .iter()
            .map(|id| o.hex[id].clone())
            .collect()
    };
    for step in recorded["steps"].as_array().unwrap() {
        let button = step["button"].as_str().unwrap();
        let before = shown(&o);
        assert_eq!(
            before,
            step["before"]
                .as_array()
                .unwrap()
                .iter()
                .map(|h| h.as_str().unwrap().to_owned())
                .collect::<BTreeSet<_>>(),
            "{button}: shown before"
        );
        match button {
            "show" => o.ui.invoke_duplicates_filtering_action("random".into(), 0),
            other => o.ui.invoke_duplicates_filtering_action(
                "set".into(),
                match other {
                    "same_quality" => 0,
                    "alternates" => 1,
                    "false_positive" => 2,
                    other => panic!("{other}"),
                },
            ),
        }
        let asked = step["asked"].as_array().unwrap();
        match asked.as_slice() {
            [] => assert!(question_opened().is_none(), "{button}: asks nothing"),
            [question] => {
                let window = question_opened().unwrap_or_else(|| panic!("{button}: asks"));
                assert_eq!(
                    window.get_window_title(),
                    question["title"].as_str().unwrap()
                );
                assert_eq!(
                    window.get_message(),
                    question["yes_no"].as_str().unwrap(),
                    "{button}"
                );
                if question["pressed"] == "yes" {
                    window.invoke_answered(true);
                } else {
                    window.invoke_answered(false);
                }
            }
            _ => panic!("one question at most"),
        }
        let recorded_shown: BTreeSet<String> = step["shown"]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| h.as_str().unwrap().to_owned())
            .collect();
        assert_eq!(
            shown(&o),
            recorded_shown,
            "{button} {}: shown after",
            step["answer"]
        );
        // the relationships of the files concerned
        for (hash, theirs) in step["relationships"].as_object().unwrap() {
            let hash_id = o.ids[hash];
            let snapshot = o.store.snapshot();
            let scope = hydrus_store::duplicates::FileScope::Domains {
                current: vec![
                    snapshot
                        .services
                        .builtin(hydrus_core::service::builtin_keys::MY_FILES)
                        .unwrap()
                        .id,
                ],
                deleted: Vec::new(),
            };
            let local = snapshot
                .services
                .builtin(hydrus_core::service::builtin_keys::HYDRUS_LOCAL_FILE_STORAGE)
                .unwrap()
                .id;
            let relationships = o
                .store
                .read(move |c| {
                    hydrus_store::duplicates::file_relationships(c, &scope, local, hash_id)
                })
                .unwrap();
            let set = |v: &[hydrus_core::HashId]| -> BTreeSet<String> {
                v.iter().map(|h| o.hex[h].clone()).collect()
            };
            let theirs_set = |key: &str| -> BTreeSet<String> {
                theirs[key]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|h| h.as_str().unwrap().to_owned())
                    .collect()
            };
            assert_eq!(
                o.hex[&relationships.king],
                theirs["king"].as_str().unwrap(),
                "{button}: king of {hash}"
            );
            assert_eq!(
                set(&relationships.alternates),
                theirs_set("3"),
                "{button}: alternates of {hash}"
            );
            assert_eq!(
                set(&relationships.false_positives),
                theirs_set("1"),
                "{button}: false positives of {hash}"
            );
            assert_eq!(
                set(&relationships.potentials),
                theirs_set("0"),
                "{button}: potentials of {hash}"
            );
            assert_eq!(
                set(&relationships.duplicates),
                theirs_set("8"),
                "{button}: members of {hash}"
            );
        }
    }
}
