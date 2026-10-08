//! A file's notes, managed from its thumbnail's "manage > notes" (the
//! reference's `EditFileNotesPanel`): notes added, renamed, edited and
//! deleted are written when applied, and cancelling with changes asks
//! first. The dialog's steps are tested against the reference's in
//! hydrus-gui-model.

use std::collections::BTreeMap;
use std::sync::Arc;

use slint::{ComponentHandle, Model as _};

use hydrus_core::HashId;
use hydrus_gui::{Bound, MainWindow, ManageNotesWindow, Pages, SearchPage, bind, headless};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;

fn notes(store: &Store, file: HashId) -> BTreeMap<String, String> {
    store.read(|c| hydrus_store::media::notes(c, file)).unwrap()
}

fn find(rows: &slint::ModelRc<hydrus_gui::MenuRow>, label: &str) -> Option<i32> {
    (0..rows.row_count())
        .map(|i| rows.row_data(i).unwrap())
        .find(|row| row.label == label)
        .map(|row| row.id)
}

fn names(dialog: &ManageNotesWindow) -> Vec<String> {
    dialog.get_names().iter().map(|n| n.to_string()).collect()
}

/// "manage > `label`" from the first thumbnail's menu.
fn open(ui: &MainWindow, bound: &Bound, label: &str) -> ManageNotesWindow {
    ui.invoke_thumbnail_clicked(0, false, false);
    ui.invoke_thumbnail_menu_requested(0);
    let manage = ui.get_thumbnail_menu().manage;
    let labels: Vec<String> = manage.iter().map(|r| r.label.to_string()).collect();
    let id = find(&manage, label).unwrap_or_else(|| panic!("{label} in {labels:?}"));
    ui.invoke_menu_chosen(id);
    bound
        .manage_notes
        .borrow()
        .as_ref()
        .map(ComponentHandle::clone_strong)
        .expect("the dialog opens")
}

/// The name asked for, given.
fn answer(dialog: &ManageNotesWindow, name: &str) {
    assert!(dialog.get_asking());
    assert_eq!(dialog.get_asking_message(), "Enter the name for the note.");
    dialog.set_asking_text(name.into());
    dialog.invoke_chosen(0);
}

fn type_note(dialog: &ManageNotesWindow, text: &str) {
    dialog.set_text(text.into());
    dialog.invoke_text_edited();
}

// leaf: audit-media-notes-tabs, audit-media-notes-cancel
#[test]
fn notes_are_added_edited_and_deleted_as_the_reference_does() {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store: Arc<Store> = Store::open(native.path()).unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let file = bound.current.borrow().borrow().results()[0];
    assert!(notes(&store, file).is_empty());

    // a file with no notes: one empty "notes" tab
    let dialog = open(&ui, &bound, "notes");
    assert_eq!(dialog.get_window_title(), "manage notes");
    assert_eq!(names(&dialog), ["notes"]);
    type_note(&dialog, "  first line\r\nsecond  \n\n");
    // "add", named as the first: numbered
    dialog.invoke_add();
    answer(&dialog, "notes");
    assert_eq!(names(&dialog), ["notes", "notes (1)"]);
    assert_eq!(dialog.get_current(), 1);
    assert_eq!(dialog.get_text(), "");
    type_note(&dialog, "the source");
    // renamed
    dialog.invoke_rename();
    assert_eq!(dialog.get_asking_text(), "notes (1)");
    answer(&dialog, "source");
    // an empty third is not written
    dialog.invoke_add();
    answer(&dialog, "empty");
    // back to the first: its text as typed
    dialog.invoke_tab_chosen(0);
    assert_eq!(dialog.get_text(), "  first line\r\nsecond  \n\n");
    dialog.invoke_apply();
    assert!(bound.manage_notes.borrow().is_none());
    assert_eq!(
        notes(&store, file),
        BTreeMap::from([
            ("notes".to_owned(), "first line\nsecond".to_owned()),
            ("source".to_owned(), "the source".to_owned()),
        ])
    );

    // the menu counts them; the dialog opens on them, by name
    let dialog = open(&ui, &bound, "notes (2)");
    assert_eq!(names(&dialog), ["notes", "source"]);
    // (the newest window)
    let last = (0..100)
        .take_while(|&n| windows.get(n).is_some())
        .last()
        .unwrap();
    let pixels = headless::render(&windows.get(last).unwrap(), 640, 420);
    let shot = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("manage_notes.png");
    headless::save_png(&shot, &pixels, 640, 420).unwrap();
    // cancelling with changes asks; "no" keeps the dialog
    dialog.invoke_tab_chosen(1);
    dialog.invoke_delete();
    assert_eq!(dialog.get_asking_message(), "Delete this note?");
    dialog.invoke_chosen(0);
    assert_eq!(names(&dialog), ["notes"]);
    dialog.invoke_cancel();
    assert_eq!(
        dialog.get_asking_message(),
        "It looks like you have made changes--are you sure you want to cancel?"
    );
    dialog.invoke_chosen(1);
    assert!(!dialog.get_asking());
    assert!(bound.manage_notes.borrow().is_some());
    // applied: the deleted note is gone
    dialog.invoke_apply();
    assert_eq!(
        notes(&store, file),
        BTreeMap::from([("notes".to_owned(), "first line\nsecond".to_owned())])
    );

    // cancelling with no changes closes at once
    let dialog = open(&ui, &bound, "notes (1)");
    dialog.invoke_cancel();
    assert!(!dialog.get_asking());
    assert!(bound.manage_notes.borrow().is_none());
}
