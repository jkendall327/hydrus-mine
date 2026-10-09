//! The import options editor's per-service "N additional tags" button and its
//! overwrite check, driven from the editor as a user does and read back from
//! the importer's saved options, against the reference's real panel with its
//! tag dialog scripted (`oracle/record_additional_tags_button.py`).

use hydrus_gui::{ImportOptionsWindow, MainWindow, Pages, bind, headless};
use hydrus_store::queues;
use serde_json::json;
use slint::{ComponentHandle as _, Model as _};

fn fixture() -> serde_json::Value {
    hydrus_testkit::fixture_json("additional_tags_button.json")
}

fn my_tags_row(editor: &ImportOptionsWindow) -> i32 {
    i32::try_from(
        editor
            .get_tag_services()
            .iter()
            .position(|s| s.name == "my tags")
            .unwrap(),
    )
    .unwrap()
}

fn button(editor: &ImportOptionsWindow, row: i32) -> String {
    editor
        .get_tag_services()
        .row_data(usize::try_from(row).unwrap())
        .unwrap()
        .additional
        .to_string()
}

fn open_tags_page(ui: &MainWindow, bound: &hydrus_gui::Bound) -> ImportOptionsWindow {
    ui.invoke_importer_import_options();
    let editor = bound
        .folders
        .import_options
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    for i in 0..editor.get_labels().row_count() {
        editor.invoke_kind_clicked(i32::try_from(i).unwrap());
        if editor.get_kind() == "tags" {
            editor.set_custom_index(1);
            editor.invoke_changed();
            return editor;
        }
    }
    panic!("no tags page");
}

// leaf: audit-network-options-additional-tags
#[test]
fn the_additional_tags_button_counts_opens_the_dialog_and_keeps_or_discards_what_it_returns() {
    let _windows = headless::init();
    let recorded = fixture();
    let (_dirs, store) = crate::subscriptions::store();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    ui.invoke_new_page();
    ui.invoke_chooser_pressed(4);
    ui.invoke_chooser_pressed(8);
    let queue = bound.current.borrow().borrow().importer().unwrap().queue;
    let editor = open_tags_page(&ui, &bound);
    let mine = my_tags_row(&editor);
    // the label counts the tags, thousands separated, "1 additional tags"
    let labels = &recorded["labels"];
    assert_eq!(button(&editor, mine), labels[0]["label"].as_str().unwrap());

    // the dialog opens on the service's tags with the reference's message
    let dialogs = &recorded["dialogs"];
    let message = dialogs[0]["dialog"]["message"].as_str().unwrap();
    editor.invoke_edit_additional_tags(mine);
    let child = hydrus_gui::write_tag_window::last_opened().expect("the dialog opens");
    assert_eq!(child.get_message(), message);
    assert!(editor.get_tag_child_open());
    // (the editor can't be applied while it is open)
    editor.invoke_apply();
    assert!(bound.folders.import_options.borrow().is_some());
    // entering and accepting
    for tag in ["zebra", "apple", "series:thing"] {
        child.invoke_edited(tag.into());
        child.invoke_entered();
    }
    child.invoke_apply();
    assert!(!editor.get_tag_child_open());
    assert_eq!(button(&editor, mine), dialogs[0]["label"].as_str().unwrap());

    // it starts with what is there, and cancelling keeps it (the reference's
    // "cancel" case)
    editor.invoke_edit_additional_tags(mine);
    let child = hydrus_gui::write_tag_window::last_opened().unwrap();
    assert_eq!(
        child
            .get_tags()
            .iter()
            .map(|t| t.text.to_string())
            .collect::<std::collections::BTreeSet<_>>(),
        ["zebra", "apple", "series:thing"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect()
    );
    child.invoke_edited("cancelled tag".into());
    child.invoke_entered();
    child.invoke_cancel();
    assert_eq!(button(&editor, mine), dialogs[0]["label"].as_str().unwrap());

    // the overwrite check, then the options reach the importer
    editor.invoke_tag_service_toggled(mine, "additional-overwrite".into(), true);
    editor.invoke_apply();
    let saved = store
        .read(move |c| queues::queue(c, queue))
        .unwrap()
        .unwrap()
        .options;
    let tags = saved.tags.expect("custom tags");
    let key = editor_key(&store);
    let mine_options = tags.service(&key).expect("my tags");
    let mut got = mine_options.additional_tags.clone();
    got.sort();
    let mut want: Vec<String> = dialogs[0]["additional_tags"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t.as_str().unwrap().to_owned())
        .collect();
    want.sort();
    assert_eq!(got, want);
    assert!(mine_options.additional_tags_overwrite_deleted);
    assert_eq!(recorded["menu"]["flipped_overwrite"], json!(true));

    // accepting with none empties it; the label says 0
    let editor = open_tags_page(&ui, &bound);
    let mine = my_tags_row(&editor);
    assert_eq!(button(&editor, mine), dialogs[0]["label"].as_str().unwrap());
    editor.invoke_edit_additional_tags(mine);
    let child = hydrus_gui::write_tag_window::last_opened().unwrap();
    for i in (0..child.get_tags().row_count()).rev() {
        child.invoke_remove(i32::try_from(i).unwrap());
    }
    child.invoke_apply();
    assert_eq!(button(&editor, mine), dialogs[1]["label"].as_str().unwrap());
    // the label for more tags (1, 2 and 1,234)
    let mut have = 0;
    for entry in labels.as_array().unwrap().iter().skip(1) {
        let target = entry["count"].as_u64().unwrap();
        editor.invoke_edit_additional_tags(mine);
        let child = hydrus_gui::write_tag_window::last_opened().unwrap();
        while have < target {
            child.invoke_edited(format!("tag {have}").into());
            child.invoke_entered();
            have += 1;
        }
        child.invoke_apply();
        assert_eq!(button(&editor, mine), entry["label"].as_str().unwrap());
    }
    editor.invoke_cancel();
}

fn editor_key(store: &hydrus_store::Store) -> String {
    store
        .snapshot()
        .services
        .by_name("my tags")
        .unwrap()
        .key
        .to_hex()
}
