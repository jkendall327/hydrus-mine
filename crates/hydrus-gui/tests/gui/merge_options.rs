//! The duplicate metadata merge options editor: the duplicates page's
//! defaults edited (a tag service taken out, one added through its action
//! and the tag filter editor, a sync changed) and written, and a rule's
//! custom merge options edited in it, and the duplicate filter's custom
//! action merging by options of its own. What the editor says and does at each
//! step is tested against the reference's in hydrus-gui-model's tests.

use std::sync::Arc;

use slint::{ComponentHandle as _, Model as _};

use hydrus_core::duplicates::{DuplicatesSearch, PairSearchKind, PixelDuplicates};
use hydrus_core::pages::{DuplicatesPage, Page, PageContent, PageKey, Session};
use hydrus_gui::merge_options_window::last_opened;
use hydrus_gui::{MainWindow, MergeOptionsWindow, Pages, bind, headless};
use hydrus_search::{FileSearchContext, LocationContext};
use hydrus_store::Store;
use hydrus_store::duplicates::auto;
use hydrus_store::duplicates::merge::{DuplicateMergeSettings, MergeAction};
use hydrus_store::sessions::{self, LAST_SESSION};

use crate::duplicate_filter::{my_files, store_with_pairs};

fn duplicates_page(store: &Store) {
    let (_, key) = my_files(store);
    let search = FileSearchContext {
        location: LocationContext::single(key),
        ..FileSearchContext::default()
    };
    let session = Session {
        name: LAST_SESSION.into(),
        pages: vec![Page {
            key: PageKey::random(),
            name: "duplicates".into(),
            content: PageContent::Duplicates {
                duplicates: DuplicatesPage::new(DuplicatesSearch {
                    search_1: search.clone(),
                    search_2: search,
                    kind: PairSearchKind::OneFileMatchesOneSearch,
                    pixel_duplicates: PixelDuplicates::Allowed,
                    max_hamming_distance: 4,
                }),
                sort: None,
            },
        }],
    };
    store
        .write(move |ctx| sessions::save(ctx.conn(), &session, 0))
        .unwrap();
}

fn cells(rows: &slint::ModelRc<hydrus_gui::TableRow>) -> Vec<Vec<String>> {
    (0..rows.row_count())
        .map(|r| {
            let cells = rows.row_data(r).unwrap().cells;
            (0..cells.row_count())
                .map(|c| cells.row_data(c).unwrap().to_string())
                .collect()
        })
        .collect()
}

fn choices(editor: &MergeOptionsWindow) -> Vec<String> {
    let choices = editor.get_asking_choices();
    (0..choices.row_count())
        .map(|i| choices.row_data(i).unwrap().to_string())
        .collect()
}

// leaf: audit-media-merge-tags, audit-media-merge-note-settings, audit-media-merge-sync
#[test]
fn the_default_merge_options_are_edited_and_written() {
    let windows = headless::init();
    let (_dir, store) = store_with_pairs();
    duplicates_page(&store);
    let ui = MainWindow::new().unwrap();
    let _bound = bind(&ui, Pages::open(Arc::clone(&store)).unwrap());

    // "this is better"'s
    ui.invoke_duplicates_action("merge options".into(), 0, false, false);
    let editor = last_opened().expect("it opens");
    assert_eq!(editor.get_window_title(), "edit duplicate merge options");
    assert_eq!(
        editor.get_note(),
        "Editing for \"this is a better duplicate\"."
    );
    assert!(editor.get_edit_action_shown());
    assert_eq!(
        cells(&editor.get_tag_rows()),
        [
            ["downloader tags", "move from worse to better", "all tags"],
            ["my tags", "move from worse to better", "all tags"],
        ]
    );
    // downloader tags taken out, asking first
    editor.invoke_row_clicked(0, 0, false, false);
    assert!(editor.get_tag_one_selected());
    editor.invoke_list_button(0, "delete".into());
    assert_eq!(editor.get_asking_message(), "Remove all selected?");
    editor.invoke_chosen(0);
    assert_eq!(cells(&editor.get_tag_rows()).len(), 1);
    // and back: the only one left is taken without asking; its action is
    // asked, then its tags in the tag filter editor
    editor.invoke_list_button(0, "add".into());
    assert_eq!(editor.get_asking_title(), "select action");
    assert_eq!(
        choices(&editor),
        [
            "copy from worse to better",
            "copy in both directions",
            "move from worse to better"
        ]
    );
    editor.invoke_chosen(0);
    let filter = hydrus_gui::tag_filter_window::last_opened().expect("the filter editor opens");
    assert_eq!(filter.get_window_title(), "edit which tags will be merged");
    filter.invoke_typed(1, "creator:".into());
    filter.invoke_apply();
    assert_eq!(
        cells(&editor.get_tag_rows())[0],
        [
            "downloader tags",
            "copy from worse to better",
            "all tags except 'creator' tags"
        ]
    );
    // urls no longer copied
    editor.set_urls_index(0);
    editor.invoke_choice_changed();
    let last = (0..100)
        .take_while(|&n| windows.get(n).is_some())
        .last()
        .unwrap();
    let pixels = headless::render(&windows.get(last - 1).unwrap(), 720, 720);
    let shots = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    headless::save_png(&shots.join("merge_options.png"), &pixels, 720, 720).unwrap();
    editor.invoke_apply();
    assert!(last_opened().is_none());
    let written: DuplicateMergeSettings = store.read(hydrus_store::settings::get).unwrap();
    assert_eq!(written.better.urls, None);
    let downloader = written
        .better
        .tags
        .iter()
        .find(|t| t.action == MergeAction::Copy)
        .expect("downloader tags, copied");
    assert_eq!(
        downloader.filter.to_filter_string(),
        "all tags except 'creator' tags"
    );
    // (the others' are as they were)
    assert_eq!(
        written.same_quality,
        DuplicateMergeSettings::default().same_quality
    );

    // "same quality"'s: one action only, so none asked
    ui.invoke_duplicates_action("merge options".into(), 1, false, false);
    let editor = last_opened().expect("it opens");
    assert!(!editor.get_edit_action_shown());
    assert_eq!(
        (0..editor.get_urls_options().row_count())
            .map(|i| editor.get_urls_options().row_data(i).unwrap().to_string())
            .collect::<Vec<_>>(),
        ["make no change", "copy in both directions"]
    );
    // the note merge settings
    editor.invoke_note_settings();
    assert!(editor.get_note_settings_open());
    editor.set_note_extend(false);
    editor.set_note_conflict(2);
    editor.invoke_note_settings_done(true);
    editor.invoke_apply();
    let written: DuplicateMergeSettings = store.read(hydrus_store::settings::get).unwrap();
    let merge = written.same_quality.note_merge.unwrap();
    assert!(!merge.extend_existing);
    assert_eq!(merge.conflict, hydrus_core::notes::NoteConflict::Append);
    assert_eq!(written.better.urls, None, "still");
}

#[test]
fn a_rules_custom_merge_options_are_edited() {
    let _windows = headless::init();
    let (_dir, store) = store_with_pairs();
    duplicates_page(&store);
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(Arc::clone(&store)).unwrap());
    ui.invoke_duplicates_action("edit rules".into(), 0, false, false);
    let list = bound
        .auto_resolution
        .list
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    list.invoke_add_suggested();
    list.invoke_suggested_chosen(0);
    list.invoke_row_clicked(0, false, false);
    list.invoke_edit();
    let rule = bound
        .auto_resolution
        .rule
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    // custom options: the client's for the action, to start from
    rule.set_default_merge(false);
    rule.invoke_changed();
    rule.invoke_edit_merge();
    let editor = last_opened().expect("it opens");
    assert_eq!(
        editor.get_note(),
        "Editing for \"this is a better duplicate\"."
    );
    editor.set_archive_index(1);
    editor.invoke_choice_changed();
    editor.invoke_apply();
    rule.invoke_apply();
    list.invoke_apply();
    let written = store.read(auto::rules).unwrap();
    let custom = written[0].1.custom_merge.as_ref().expect("custom options");
    assert_eq!(
        custom.archive,
        hydrus_store::duplicates::merge::ArchiveSync::IfEither
    );
}

#[test]
fn the_duplicate_filters_custom_action_merges_by_its_own_options() {
    let _windows = headless::init();
    let (_dir, store) = store_with_pairs();
    duplicates_page(&store);
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(Arc::clone(&store)).unwrap());
    ui.invoke_launch_filter();
    let filter = bound
        .filter
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong)
        .expect("the filter opened");
    let answers = |filter: &hydrus_gui::DuplicateFilterWindow| {
        let answers = filter.get_answers();
        (0..answers.row_count())
            .map(|i| answers.row_data(i).unwrap().to_string())
            .collect::<Vec<_>>()
    };
    // which decision
    filter.invoke_decide("custom".into());
    assert_eq!(filter.get_question(), "select duplicate type");
    assert_eq!(
        answers(&filter),
        [
            "alternates",
            "not related/false positive",
            "same quality",
            "this is a better duplicate",
            "cancel"
        ]
    );
    filter.invoke_answer(3);
    // its merge options, for this decision alone: archive neither
    let editor = last_opened().expect("the merge options open");
    assert!(editor.get_syncs_enabled());
    editor.set_archive_index(0);
    editor.invoke_choice_changed();
    editor.invoke_apply();
    // which to delete
    assert_eq!(filter.get_question(), "Delete any of the files?");
    assert_eq!(
        answers(&filter),
        [
            "delete neither",
            "delete this one",
            "delete the other",
            "delete both",
            "forget it"
        ]
    );
    filter.invoke_answer(0);
    assert!(
        filter.get_index_text().contains("1 decision"),
        "{}",
        filter.get_index_text()
    );
    // committed on closing: the files kept in the inbox, where the
    // client's options for "this is better" archive both
    let inbox_before: usize = store
        .read(|c| {
            let ids: Vec<hydrus_core::HashId> = c
                .prepare("SELECT hash_id FROM hashes")?
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            Ok(hydrus_store::media::inboxed(c, &ids)?.len())
        })
        .unwrap();
    assert!(inbox_before >= 2);
    filter.invoke_close_requested();
    filter.invoke_answer(0);
    assert!(bound.filter.borrow().is_none());
    let inbox_after: usize = store
        .read(|c| {
            let ids: Vec<hydrus_core::HashId> = c
                .prepare("SELECT hash_id FROM hashes")?
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            Ok(hydrus_store::media::inboxed(c, &ids)?.len())
        })
        .unwrap();
    assert_eq!(inbox_after, inbox_before);
    let groups: i64 = store
        .read(|c| Ok(c.query_row("SELECT COUNT(*) FROM dup_group_members", [], |r| r.get(0))?))
        .unwrap();
    assert!(groups > 0, "the pair is set as duplicates");
}

fn texts(model: &slint::ModelRc<slint::SharedString>) -> Vec<String> {
    model.iter().map(|s| s.to_string()).collect()
}

// leaf: audit-media-merge-ratings
#[test]
fn the_ratings_list_adds_edits_and_deletes_as_the_references_does() {
    // the editor's steps on ratings, as the reference's recording has them
    // (`oracle/record_merge_options_editor.py`), done in the window on the
    // same services: the service and action questions asked, the rows
    // listed after each step, and the options written on Apply
    let recorded = hydrus_testkit::fixture_json("merge_options_editor.json");
    let _windows = headless::init();
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    duplicates_page(&store);
    let ui = MainWindow::new().unwrap();
    let _bound = bind(&ui, Pages::open(Arc::clone(&store)).unwrap());
    let mut played = 0;
    for case in recorded["cases"].as_array().unwrap() {
        // (the page's menu: better, same quality, alternates, false positive;
        // the ones started from the client's options, not a rule's or empty)
        let index = match case["decision"].as_str().unwrap() {
            "better" => 0,
            "same quality" => 1,
            "alternates" => 2,
            _ => 3,
        };
        if case["start"] != "client" || case["custom"].as_bool().unwrap() {
            continue;
        }
        ui.invoke_duplicates_action("merge options".into(), index, false, false);
        let editor = last_opened().expect("it opens");
        let row_of = |editor: &MergeOptionsWindow, name: &str| -> i32 {
            cells(&editor.get_rating_rows())
                .iter()
                .position(|r| r[0] == name)
                .unwrap_or_else(|| panic!("no rating row {name}")) as i32
        };
        let first = &case["states"][0]["state"]["rating_rows"];
        let rows = |value: &serde_json::Value| -> Vec<Vec<String>> {
            value
                .as_array()
                .unwrap()
                .iter()
                .map(|r| {
                    r.as_array()
                        .unwrap()
                        .iter()
                        .map(|c| c.as_str().unwrap().to_owned())
                        .collect()
                })
                .collect()
        };
        assert_eq!(
            cells(&editor.get_rating_rows()),
            rows(first),
            "{}",
            case["decision"]
        );
        for state in case["states"].as_array().unwrap() {
            let Some(step) = state["step"].as_array() else {
                continue;
            };
            let kind = step[0].as_str().unwrap();
            if !kind.ends_with("rating") && kind != "delete_ratings" {
                continue;
            }
            let at = format!("{} {step:?}", case["decision"]);
            let said = state["said"].as_array().unwrap();
            let ask = |editor: &MergeOptionsWindow, title: &str, choices: &serde_json::Value| {
                assert_eq!(editor.get_asking_title(), title, "{at}");
                let mut shown = texts(&editor.get_asking_choices());
                let mut theirs: Vec<String> = choices
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|c| c.as_str().unwrap().to_owned())
                    .collect();
                shown.sort();
                theirs.sort();
                assert_eq!(shown, theirs, "{at}");
            };
            let choose = |editor: &MergeOptionsWindow, label: &str| {
                let at = texts(&editor.get_asking_choices())
                    .iter()
                    .position(|c| c == label)
                    .unwrap_or_else(|| panic!("no choice {label}"));
                editor.invoke_chosen(at as i32);
            };
            match kind {
                "add_rating" => {
                    editor.invoke_list_button(1, "add".into());
                    // (a lone service is taken without asking)
                    let first = &said[0];
                    if let Some(title) = first.get("select") {
                        ask(&editor, title.as_str().unwrap(), &first["choices"]);
                    } else {
                        assert!(!editor.get_asking(), "{at}");
                    }
                    if let Some(service) = step[1].as_str() {
                        choose(&editor, service);
                        if let Some(second) = said.get(1) {
                            ask(
                                &editor,
                                second["select"].as_str().unwrap(),
                                &second["choices"],
                            );
                            choose(&editor, step[2].as_str().unwrap());
                        }
                    } else {
                        editor.invoke_cancelled();
                    }
                }
                "edit_rating" => {
                    let row = row_of(&editor, step[1].as_str().unwrap());
                    editor.invoke_row_activated(1, row);
                    ask(
                        &editor,
                        said[0]["select"].as_str().unwrap(),
                        &said[0]["choices"],
                    );
                    choose(&editor, step[2].as_str().unwrap());
                }
                _ => {
                    for name in step[1].as_array().unwrap() {
                        let row = row_of(&editor, name.as_str().unwrap());
                        editor.invoke_row_clicked(1, row, true, false);
                    }
                    editor.invoke_list_button(1, "delete".into());
                    assert_eq!(
                        editor.get_asking_message(),
                        said[0]["asked"].as_str().unwrap(),
                        "{at}"
                    );
                    editor.invoke_chosen(0);
                }
            }
            assert!(!editor.get_asking(), "{at}");
            assert_eq!(
                cells(&editor.get_rating_rows()),
                rows(&state["state"]["rating_rows"]),
                "{at}"
            );
            played += 1;
        }
        editor.invoke_apply();
    }
    assert!(played >= 5, "{played}");
}
