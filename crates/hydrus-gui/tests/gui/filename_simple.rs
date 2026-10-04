//! Simple filename tags: recorded additive actions, real seed and folder consumers.
use hydrus_gui::{FilenameTaggingWindow, MainWindow, Pages, bind, headless, write_tag_window};
use serde_json::{Value, json};
use slint::{ComponentHandle as _, Model as _};

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}
fn tags(text: &str) -> Vec<String> {
    text.lines().map(str::to_owned).collect()
}
fn cell(window: &FilenameTaggingWindow, index: usize, column: usize) -> String {
    window
        .get_rows()
        .row_data(index)
        .unwrap()
        .cells
        .row_data(column)
        .unwrap()
        .to_string()
}
fn parsed(window: &hydrus_gui::ReviewImportsWindow) {
    for _ in 0..500 {
        std::thread::sleep(std::time::Duration::from_millis(10));
        slint::platform::update_timers_and_animations();
        if !window.get_working() && window.get_can_import() {
            return;
        }
    }
    panic!("review did not finish parsing");
}
fn child(window: &FilenameTaggingWindow, single: bool) -> hydrus_gui::WriteTagsWindow {
    window.invoke_edit_tags(single);
    assert!(window.get_tags_child_open());
    write_tag_window::last_opened().unwrap()
}

#[test]
fn real_simple_actions_replay_selection_paste_and_removal_then_reach_manual_import_seeds() {
    let windows = headless::init();
    let (_dirs, store) = crate::subscriptions::store();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let fixture = hydrus_testkit::fixture_json("filename_simple.json");
    let paths = strings(&fixture["paths"]);
    let work = tempfile::tempdir().unwrap();
    let files: Vec<_> = paths
        .iter()
        .map(|path| {
            let tail = path.strip_prefix("/srv/").unwrap();
            let file = work.path().join(tail);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::copy(hydrus_testkit::fixture_path("media/bmp_24.bmp"), &file).unwrap();
            file.to_string_lossy().into_owned()
        })
        .collect();
    (bound.drop_files)(files.clone());
    let review = bound
        .review_imports
        .borrow()
        .as_ref()
        .unwrap()
        .0
        .clone_strong();
    parsed(&review);
    review.invoke_add_tags();
    let dialog = bound
        .filename_tagging
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    let mine = dialog
        .get_services()
        .iter()
        .position(|s| s == "my tags")
        .unwrap();
    dialog.invoke_service_chosen(i32::try_from(mine).unwrap());
    let order: Vec<_> = (0..dialog.get_rows().row_count())
        .map(|i| cell(&dialog, i, 1))
        .collect();
    let positions: Vec<_> = files
        .iter()
        .map(|path| order.iter().position(|p| p == path).unwrap())
        .collect();
    assert_eq!(order.len(), paths.len());
    dialog.invoke_edit_tags(true);
    dialog.invoke_paste_tags(true);
    assert!(
        !dialog.get_tags_child_open(),
        "selected actions are unavailable with no selection"
    );
    for step in fixture["steps"].as_array().unwrap() {
        let action = step["action"].as_str().unwrap();
        match action {
            "select" => {
                // Clear the actual selection, then replay recorded path identities.
                for i in 0..dialog.get_rows().row_count() {
                    if dialog.get_rows().row_data(i).unwrap().selected {
                        dialog.invoke_row_clicked(i32::try_from(i).unwrap(), true, false);
                    }
                }
                for (n, index) in step["value"].as_array().unwrap().iter().enumerate() {
                    let at = positions[usize::try_from(index.as_u64().unwrap()).unwrap()];
                    dialog.invoke_row_clicked(i32::try_from(at).unwrap(), n > 0, false);
                }
            }
            "enter_all"
            | "enter_single"
            | "autocomplete_paste_single"
            | "remove_all"
            | "remove_single" => {
                let single = !action.ends_with("all");
                let before: Vec<_> = (0..order.len()).map(|i| cell(&dialog, i, 2)).collect();
                let child = child(&dialog, single);
                let old_service = dialog.get_service_index();
                dialog.invoke_row_clicked(0, false, false);
                dialog.invoke_service_chosen(99);
                dialog.invoke_sidecars_chosen();
                dialog.invoke_paste_tags(false);
                dialog.invoke_apply();
                assert_eq!(dialog.get_service_index(), old_service);
                assert_eq!(
                    (0..order.len())
                        .map(|i| cell(&dialog, i, 2))
                        .collect::<Vec<_>>(),
                    before
                );
                assert!(bound.filename_tagging.borrow().is_some());
                let input = strings(&step["value"]);
                if action.starts_with("enter") {
                    for tag in &input {
                        child.invoke_edited(tag.as_str().into());
                        child.invoke_entered();
                    }
                } else if action.starts_with("autocomplete") {
                    let text = input.join("\n");
                    hydrus_gui::set_clipboard_reader(move || Ok(Some(text.clone())));
                    assert!(child.invoke_paste(true));
                } else {
                    for tag in &input {
                        let index = child
                            .get_tags()
                            .iter()
                            .position(|t| t.text == tag.as_str())
                            .unwrap();
                        child.invoke_remove(i32::try_from(index).unwrap());
                    }
                }
                child.invoke_apply();
                assert!(!dialog.get_tags_child_open());
                let accepted = dialog.get_tags_all();
                child.invoke_edited("closed child".into());
                child.invoke_entered();
                child.invoke_apply();
                assert_eq!(dialog.get_tags_all(), accepted);
            }
            "paste_all" | "paste_single" => {
                let raw = step["value"].as_str().unwrap().to_owned();
                hydrus_gui::set_clipboard_reader(move || Ok(Some(raw.clone())));
                dialog.invoke_paste_tags(action == "paste_single");
            }
            "filename_text" => {
                dialog.invoke_misc_namespace(0, step["value"].as_str().unwrap().into())
            }
            "filename_check" => dialog.invoke_misc_toggled(0, step["value"].as_bool().unwrap()),
            "directory_text" | "directory_check" => {
                let index = step["value"][0].as_i64().unwrap();
                let row = hydrus_gui_model::filename_tagging::DIRECTORIES
                    .iter()
                    .position(|(_, i)| *i == index)
                    .unwrap()
                    + 1;
                if action == "directory_text" {
                    dialog.invoke_misc_namespace(
                        i32::try_from(row).unwrap(),
                        step["value"][1].as_str().unwrap().into(),
                    );
                } else {
                    dialog.invoke_misc_toggled(
                        i32::try_from(row).unwrap(),
                        step["value"][1].as_bool().unwrap(),
                    );
                }
            }
            "clipboard_missing" => {
                hydrus_gui::set_clipboard_reader(|| Err("clipboard has no text".into()));
                dialog.invoke_paste_tags(false);
                assert_eq!(
                    dialog.get_errors(),
                    "Problem pasting!\nclipboard has no text"
                );
            }
            _ => panic!("unknown recorded action {action}"),
        }
        assert_eq!(
            json!(tags(&dialog.get_tags_all())),
            step["state"]["all"],
            "{action}"
        );
        assert_eq!(
            json!(tags(&dialog.get_tags_selected())),
            step["state"]["single"],
            "{action}"
        );
        assert_eq!(
            dialog.get_has_selection(),
            step["state"]["enabled"].as_bool().unwrap(),
            "{action}"
        );
        for (i, position) in positions.iter().enumerate() {
            assert_eq!(
                cell(&dialog, *position, 2),
                strings(&step["state"]["tags"][i]).join(", "),
                "{action} {i}"
            );
        }
        let misc = dialog.get_misc();
        assert_eq!(
            misc.row_data(0).unwrap().on,
            step["state"]["filename"][0].as_bool().unwrap()
        );
        assert_eq!(
            misc.row_data(0).unwrap().namespace.as_str(),
            step["state"]["filename"][1].as_str().unwrap()
        );
        for (i, (_, directory)) in hydrus_gui_model::filename_tagging::DIRECTORIES
            .iter()
            .enumerate()
        {
            let expected = &step["state"]["directories"][directory.to_string()];
            let row = misc.row_data(i + 1).unwrap();
            assert_eq!(row.on, expected[0].as_bool().unwrap());
            assert_eq!(row.namespace.as_str(), expected[1].as_str().unwrap());
        }
    }
    // No-op Apply on a multi-file union retains the recorded per-path tags.
    dialog.invoke_row_clicked(i32::try_from(positions[0]).unwrap(), false, false);
    dialog.invoke_row_clicked(i32::try_from(positions[1]).unwrap(), true, false);
    let before: Vec<_> = (0..order.len()).map(|i| cell(&dialog, i, 2)).collect();
    child(&dialog, true).invoke_apply();
    assert_eq!(
        (0..order.len())
            .map(|i| cell(&dialog, i, 2))
            .collect::<Vec<_>>(),
        before
    );
    let child = child(&dialog, false);
    child.invoke_edited("cancelled only".into());
    child.invoke_entered();
    child.invoke_cancel();
    assert!(!dialog.get_tags_all().contains("cancelled only"));
    let pixels = headless::render(&windows.get(2).unwrap(), 1000, 760);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("filename_simple.png"),
        &pixels,
        1000,
        760,
    )
    .unwrap();
    dialog.invoke_apply();
    assert!(bound.filename_tagging.borrow().is_none());
    assert!(bound.review_imports.borrow().is_none());
    let queue = bound.current.borrow().borrow().importer().unwrap().queue;
    let seeds = store
        .read(move |conn| hydrus_store::queues::file_seeds(conn, queue))
        .unwrap();
    assert_eq!(seeds.len(), files.len());
    let service = hex::encode(hydrus_core::service::builtin_keys::MY_TAGS);
    for (i, file) in files.iter().enumerate() {
        let seed = seeds.iter().find(|seed| seed.data == *file).unwrap();
        let tags: Vec<_> = seed
            .meta
            .external_additional_tags
            .iter()
            .find(|(key, _)| *key == service)
            .unwrap()
            .1
            .iter()
            .cloned()
            .collect();
        assert_eq!(json!(tags), fixture["applied"]["tags"][i]);
    }
}

#[test]
fn folder_tag_child_is_staged_reopens_and_is_cancelled_with_its_owner() {
    let _windows = headless::init();
    let (_dirs, store) = crate::subscriptions::store();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let work = tempfile::tempdir().unwrap();
    crate::folders::open(&ui, "manage import folders\u{2026}");
    let list = bound
        .folders
        .import_list
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    list.invoke_add();
    let folder = bound
        .folders
        .import_edit
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    folder.set_path(work.path().to_string_lossy().into_owned().into());
    let mine = folder
        .get_tagging_choices()
        .iter()
        .position(|s| s == "my tags")
        .unwrap();
    folder.set_tagging_choice(i32::try_from(mine).unwrap());
    folder.invoke_tagging_add();
    let dialog = bound
        .folders
        .filename_tagging
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    let entry = child(&dialog, false);
    entry.invoke_edited(" Folder:Saved ".into());
    entry.invoke_entered();
    entry.invoke_entered();
    entry.invoke_apply();
    assert_eq!(dialog.get_tags_all(), "folder:saved");
    assert!(
        store
            .read(hydrus_store::import_folders::import_folders)
            .unwrap()
            .is_empty()
    );
    dialog.invoke_apply();
    folder.invoke_apply();
    list.invoke_apply();
    let saved = store
        .read(hydrus_store::import_folders::import_folders)
        .unwrap();
    let options = saved[0].settings.filename_tagging[0].1.clone();
    assert_eq!(
        options.tags(&work.path().join("one.jpg").to_string_lossy()),
        std::collections::BTreeSet::from(["folder:saved".to_owned()])
    );
    crate::folders::open(&ui, "manage import folders\u{2026}");
    let list = bound
        .folders
        .import_list
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    list.invoke_row_clicked(0, false, false);
    list.invoke_edit();
    let folder = bound
        .folders
        .import_edit
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    folder.invoke_tagging_edit(0);
    let dialog = bound
        .folders
        .filename_tagging
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(dialog.get_tags_all(), "folder:saved");
    let entry = child(&dialog, false);
    entry.invoke_edited("never saved".into());
    entry.invoke_entered();
    hydrus_gui::set_clipboard_reader(|| Ok(Some("one\ntwo".into())));
    assert!(entry.invoke_paste(false));
    assert!(!entry.get_question().is_empty());
    dialog.invoke_cancel();
    assert!(!entry.window().is_visible());
    assert!(!dialog.get_tags_child_open());
    entry.invoke_answered(true);
    entry.invoke_apply();
    dialog.invoke_apply();
    folder.invoke_apply();
    list.invoke_cancel();
    assert_eq!(
        store
            .read(hydrus_store::import_folders::import_folders)
            .unwrap(),
        saved
    );
}
