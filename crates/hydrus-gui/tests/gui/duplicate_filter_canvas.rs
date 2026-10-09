//! The duplicate filter's window against the reference's canvas, recorded by
//! `oracle/record_duplicate_filter_canvas.py`: the real
//! `CanvasFilterDuplicates` on the database the reference's auto-resolution
//! run left, driven with its application commands through short batches of
//! pairs. Each scenario is replayed on the native window opened on the same
//! database (the same files and potential pairs), with the same commands and
//! the same buttons pressed on the same questions; after every step the
//! pair shown, the top bar's text, the questions asked and their buttons,
//! whether the window is open and what a commit wrote are the reference's.

use std::collections::{BTreeSet, HashMap};

use slint::{ComponentHandle as _, Model as _};

use hydrus_core::HashId;
use hydrus_core::service::builtin_keys;
use hydrus_gui::headless;
use hydrus_store::duplicates::{DuplicateFilterSettings, FileScope, file_relationships};
use serde_json::Value;

use crate::auto_resolution_review::{Opened, opened};

fn hex_of(o: &Opened, id: HashId) -> String {
    o.hex[&id].clone()
}

fn pair_now(o: &Opened) -> Option<[String; 2]> {
    hydrus_gui::shown_pair().map(|(a, b)| [hex_of(o, a), hex_of(o, b)])
}

fn answers_of(window: &hydrus_gui::DuplicateFilterWindow) -> Vec<String> {
    window.get_answers().iter().map(|a| a.to_string()).collect()
}

/// File > options, on `page`.
fn options_page(o: &Opened, page: &str) -> hydrus_gui::OptionsWindow {
    o.ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = o.ui.get_menu_panes().row_data(0).unwrap().lines;
    let at = (0..lines.row_count())
        .position(|i| lines.row_data(i).unwrap().label == "options\u{2026}")
        .expect("file > options");
    o.ui.invoke_menu_line_clicked(0, at as i32, 0.0, 0.0, 0.0);
    let options = o.bound.options.borrow().as_ref().unwrap().clone_strong();
    let at = (0..options.get_pages().row_count())
        .position(|i| options.get_pages().row_data(i).unwrap().text == page)
        .unwrap_or_else(|| panic!("page {page}"));
    options.invoke_page_chosen(at as i32);
    options
}

fn option_row(options: &hydrus_gui::OptionsWindow, label: &str) -> (i32, hydrus_gui::OptionRow) {
    let rows = options.get_rows();
    (0..rows.row_count())
        .map(|i| (i as i32, rows.row_data(i).unwrap()))
        .find(|(_, r)| r.label == label)
        .unwrap_or_else(|| panic!("{label:?}"))
}

/// The scenario's options, set in the Options window and applied: the
/// duplicate filter's batch size and auto-commit size (duplicates), and the
/// pairs to prefetch (speed and memory).
fn set_options(o: &Opened, options: &Value) {
    let window = options_page(o, "duplicates");
    let (row, shown) =
        option_row(&window, "Max size of duplicate filter pair batches (in mixed mode):");
    assert_eq!((shown.kind, shown.minimum, shown.maximum), (2, 5, 1024));
    let size = options["duplicate_filter_max_batch_size"]
        .as_i64()
        .unwrap_or(100);
    window.invoke_number_edited(row, size as i32);
    let (row, shown) = option_row(
        &window,
        "Auto-commit completed batches of this size or smaller:",
    );
    assert_eq!((shown.kind, shown.minimum, shown.maximum), (3, 1, 50));
    let auto = options
        .get("duplicate_filter_auto_commit_batch_size")
        .and_then(Value::as_i64);
    if let Some(auto) = auto {
        window.invoke_number_edited(row, auto as i32);
    }
    window.invoke_none_toggled(row, auto.is_none());
    window.invoke_apply();
    let saved: DuplicateFilterSettings = o.store.read(hydrus_store::settings::get).unwrap();
    assert_eq!(
        (saved.max_batch_size, saved.auto_commit_batch_size),
        (size as u32, auto.map(|n| n as u32))
    );
    if let Some(pairs) = options
        .get("duplicate_filter_prefetch_num_pairs")
        .and_then(Value::as_i64)
    {
        let window = options_page(o, "speed and memory");
        let (row, shown) = option_row(&window, "Num pairs to prefetch in Duplicate Filter:");
        assert_eq!((shown.minimum, shown.maximum), (0, 25));
        window.invoke_number_edited(row, pairs as i32);
        // (and room in the cache for all of it: what the reference asked to
        // prefetch is what is compared)
        let (row, _) = option_row(
            &window,
            "Maximum % of cache that will be prefetched per media viewer:",
        );
        window.invoke_number_edited(row, 50);
        let (row, _) = option_row(&window, "Memory reserved for image cache:");
        // 4 GiB
        window.invoke_number_edited(row, 4);
        window.invoke_choice_chosen(row, 3);
        window.invoke_apply();
        assert_eq!(
            o.store
                .read(hydrus_store::image_cache::load)
                .unwrap()
                .bytes,
            4 << 30
        );
    }
}

/// The images among `files` (what the image cache warms).
fn images(o: &Opened, files: &[HashId]) -> BTreeSet<HashId> {
    let loaded = o
        .store
        .read(|conn| hydrus_store::media::load_basic(conn, files))
        .unwrap();
    loaded
        .iter()
        .filter(|f| {
            f.info
                .as_ref()
                .is_some_and(|i| i.mime.general_class() == Some(hydrus_core::Mime::GeneralImage))
        })
        .map(|f| f.hash_id)
        .collect()
}

/// The image cache warms what the reference's canvas prefetched (its
/// `_GetPrefetchNeighboursInPreferenceOrder`: the other file of the pair
/// shown and both files of the next pairs); at the start, nothing more than
/// that and the file shown.
fn prefetched(o: &Opened, prefetch: &Value, shown: &str, start: bool, what: &str) {
    let wanted: Vec<HashId> = prefetch
        .as_array()
        .unwrap()
        .iter()
        .map(|h| o.ids[h.as_str().unwrap()])
        .collect();
    let wanted = images(o, &wanted);
    let cache = o.bound.image_cache.clone();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        slint::platform::update_timers_and_animations();
        let warmed: BTreeSet<HashId> = cache.keys().into_iter().collect();
        if warmed.is_superset(&wanted) {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "{what}: warmed {warmed:?}, wanted {wanted:?}"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    if start {
        for _ in 0..10 {
            slint::platform::update_timers_and_animations();
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        let mut allowed = wanted.clone();
        allowed.extend(images(o, &[o.ids[shown]]));
        let warmed: BTreeSet<HashId> = cache.keys().into_iter().collect();
        assert!(
            warmed.is_subset(&allowed),
            "{what}: warmed more than the reference prefetched: {:?}",
            warmed.difference(&allowed).collect::<Vec<_>>()
        );
    }
}

/// Replay one recorded scenario; the questions the native window asked.
fn replay(scenario: &Value) {
    let o = opened();
    let name = scenario["name"].as_str().unwrap();
    let options = &scenario["options"];
    // the options through File > options, as a user sets them
    set_options(&o, options);
    if options.get("group_mode") == Some(&Value::Bool(true)) {
        // the filtering tab's "mixed pairs"/"group mode" chooser
        o.ui.invoke_duplicates_filtering_action("group".into(), 1);
    }
    o.ui.invoke_launch_filter();
    let window = o
        .bound
        .filter
        .borrow()
        .as_ref()
        .expect("the filter opens")
        .clone_strong();

    let start = &scenario["start"];
    assert_eq!(
        window.get_index_text(),
        start["index"].as_str().unwrap(),
        "{name}"
    );
    assert_eq!(
        pair_now(&o).unwrap().to_vec(),
        [
            start["shown"].as_str().unwrap().to_owned(),
            start["other"].as_str().unwrap().to_owned()
        ],
        "{name}: first pair"
    );
    let batch: Vec<[String; 2]> = scenario["batch"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            [
                p[0].as_str().unwrap().to_owned(),
                p[1].as_str().unwrap().to_owned(),
            ]
        })
        .collect();
    assert_eq!(batch.len() as u64, start["num_pairs"].as_u64().unwrap());
    if let Some(prefetch) = start.get("prefetch") {
        prefetched(&o, prefetch, start["shown"].as_str().unwrap(), true, name);
    }

    let my_files = o
        .store
        .snapshot()
        .services
        .builtin(builtin_keys::MY_FILES)
        .unwrap()
        .id;
    let batch_files: Vec<HashId> = batch.iter().flatten().map(|h| o.ids[h]).collect();
    let in_my_files = |o: &Opened| -> BTreeSet<String> {
        o.store
            .read(|c| hydrus_store::media::current_in(c, my_files, &batch_files))
            .unwrap()
            .into_iter()
            .map(|h| hex_of(o, h))
            .collect()
    };
    let before_files = in_my_files(&o);

    for step in scenario["steps"].as_array().unwrap() {
        let what = format!("{name}: {} {}", step["do"], step["answers"]);
        let mut pressed: Vec<String> = Vec::new();
        let mut asked_now: Vec<(String, Vec<String>, Option<String>)> = Vec::new();
        match step["do"].as_str().unwrap() {
            "close" => window.invoke_close_requested(),
            "switch" => window.invoke_switch_media(),
            command => window.invoke_decide(
                match command {
                    "better_delete_other" => "better-delete",
                    "better_keep_both" => "better-keep",
                    "same" => "same",
                    "alternates" => "alternates",
                    "false_positive" => "false-positive",
                    "skip" => "skip",
                    "back" => "back",
                    other => panic!("{other}"),
                }
                .into(),
            ),
        }
        // press the recorded buttons on the questions the window asks (and
        // "yes" where the reference asked a yes/no question, which it was
        // scripted to accept)
        let mut to_press: Vec<String> = step["asked"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| match a.get("pressed") {
                Some(button) => button.as_str().unwrap().to_owned(),
                None => "yes".to_owned(),
            })
            .collect();
        while !window.get_question().is_empty() && window.window().is_visible() {
            let question = window.get_question().to_string();
            let buttons = answers_of(&window);
            let Some(button) = (!to_press.is_empty()).then(|| to_press.remove(0)) else {
                // a question nobody answered
                asked_now.push((question, buttons, None));
                break;
            };
            let at = buttons
                .iter()
                .position(|b| *b == button)
                .unwrap_or_else(|| panic!("{what}: no button {button} among {buttons:?}"));
            asked_now.push((question, buttons, Some(button)));
            window.invoke_answer(i32::try_from(at).unwrap());
        }
        pressed.clear();
        // the questions the reference asked: the dialogs with their buttons,
        // then its yes/no questions (which have no buttons of ours)
        let recorded_asked: Vec<(String, BTreeSet<String>, Option<String>)> = step["asked"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| {
                if let Some(texts) = a.get("texts") {
                    (
                        texts[0].as_str().unwrap().to_owned(),
                        a["buttons"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|b| b.as_str().unwrap().to_owned())
                            .collect(),
                        Some(a["pressed"].as_str().unwrap().to_owned()),
                    )
                } else {
                    (
                        a.get("yes_no")
                            .or_else(|| a.get("information"))
                            .or_else(|| a.get("critical"))
                            .unwrap()
                            .as_str()
                            .unwrap()
                            .to_owned(),
                        BTreeSet::new(),
                        None,
                    )
                }
            })
            .collect();
        let ours: Vec<(String, BTreeSet<String>, Option<String>)> = asked_now
            .into_iter()
            .map(|(q, b, p)| (q, b.into_iter().collect(), p))
            .collect();
        assert_eq!(
            ours.len(),
            recorded_asked.len(),
            "{what}: {ours:?} vs {recorded_asked:?}"
        );
        for (ours, theirs) in ours.iter().zip(&recorded_asked) {
            assert_eq!(ours.0, theirs.0, "{what}: question");
            if theirs.1.is_empty() {
                // a yes/no question, answered yes
                assert_eq!(
                    ours.1,
                    BTreeSet::from(["yes".to_owned(), "no".to_owned()]),
                    "{what}"
                );
            } else {
                assert_eq!(ours.1, theirs.1, "{what}: buttons");
                assert_eq!(ours.2, theirs.2, "{what}: pressed");
            }
        }

        let after = &step["after"];
        let open = window.window().is_visible();
        // (the reference's harness left its window open when closing was
        // allowed; the window closes)
        let closed = step.get("ok_to_close").and_then(Value::as_bool) == Some(true);
        if closed {
            assert!(!open, "{what}: still open");
        } else {
            assert_eq!(open, after["open"].as_bool().unwrap(), "{what}: open");
        }
        if open {
            assert_eq!(
                window.get_index_text(),
                after["index"].as_str().unwrap(),
                "{what}"
            );
            let shown = after["shown"].as_str().map(|shown| {
                [
                    shown.to_owned(),
                    after["other"].as_str().unwrap().to_owned(),
                ]
            });
            assert_eq!(pair_now(&o), shown, "{what}: pair");
            if let Some(prefetch) = after.get("prefetch") {
                prefetched(&o, prefetch, after["shown"].as_str().unwrap(), false, &what);
            }
        }
    }

    // the files the commits deleted left my files, the others stayed
    let deleted: BTreeSet<String> = scenario["steps"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|st| st["writes"].as_array().unwrap())
        .flat_map(|w| w["deleted"].as_array().unwrap())
        .map(|h| h.as_str().unwrap().to_owned())
        .collect();
    let expected: BTreeSet<String> = before_files.difference(&deleted).cloned().collect();
    assert_eq!(in_my_files(&o), expected, "{name}: files left in my files");

    // what the commits left in the database
    let Some(relationships) = scenario.get("relationships") else {
        return;
    };
    // (the reference asked about the combined local file domains: my files
    // here, which has no trashed file; "local" is the local file storage)
    let local = o
        .store
        .snapshot()
        .services
        .builtin(builtin_keys::HYDRUS_LOCAL_FILE_STORAGE)
        .unwrap()
        .id;
    let id = o
        .store
        .snapshot()
        .services
        .builtin(builtin_keys::MY_FILES)
        .unwrap()
        .id;
    let scope = FileScope::Domains {
        current: vec![id],
        deleted: Vec::new(),
    };
    let ids: &HashMap<String, HashId> = &o.ids;
    let set = |v: &[HashId]| -> BTreeSet<String> { v.iter().map(|h| hex_of(&o, *h)).collect() };
    for (hash, theirs) in relationships.as_object().unwrap() {
        let ours = o
            .store
            .read(|c| file_relationships(c, &scope, local, ids[hash]))
            .unwrap();
        let hashes = |key: &str| -> BTreeSet<String> {
            theirs[key]
                .as_array()
                .unwrap()
                .iter()
                .map(|h| h.as_str().unwrap().to_owned())
                .collect()
        };
        assert_eq!(
            hex_of(&o, ours.king),
            theirs["king"].as_str().unwrap(),
            "{name}: king of {hash}"
        );
        assert_eq!(
            set(&ours.potentials),
            hashes("0"),
            "{name}: potentials of {hash}"
        );
        assert_eq!(
            set(&ours.false_positives),
            hashes("1"),
            "{name}: false positives of {hash}"
        );
        assert_eq!(
            set(&ours.alternates),
            hashes("3"),
            "{name}: alternates of {hash}"
        );
        let members: BTreeSet<String> = set(&ours.duplicates);
        assert_eq!(members, hashes("8"), "{name}: members of {hash}");
    }
}

// leaf: audit-media-filter-decisions, audit-media-filter-commit
// leaf: audit-media-filter-back
// leaf: audit-options-duplicates-duplicate-filter-batches-auto-commit-completed-batches-of-this-size-or-smaller
// leaf: audit-options-duplicates-duplicate-filter-batches-max-size-of-duplicate-filter-pair-batches-in-mixed-mode
// leaf: audit-options-speed-and-memory-image-prefetch-num-pairs-to-prefetch-in-duplicate-filter
#[test]
fn the_filter_window_goes_through_the_references_batches_as_it_does() {
    let _windows = headless::init();
    // (the reference's random group pick was pinned to the lowest pair)
    hydrus_store::duplicates::pin_random_group_choice();
    let recorded = hydrus_testkit::fixture_json("duplicate_filter_canvas.json");
    let scenarios = recorded["scenarios"].as_array().unwrap();
    assert_eq!(scenarios.len(), 17);
    for scenario in scenarios {
        replay(scenario);
    }
}
