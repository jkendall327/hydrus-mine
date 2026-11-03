//! The "review files to import" window's import options button (the
//! reference's `SpecificImportOptionsContainerButton` on a local import,
//! `oracle/record_flow_manual_import.py` recording its labels): clicked as a
//! user clicks it, it opens the import options editor on a local import's
//! defaults; "cancel" keeps what the import had; "apply" sets it, the
//! button saying so; opened again, the editor shows what was applied; and
//! the import page made from the window gets it.

use std::sync::Arc;

use slint::{ComponentHandle as _, Model as _};

use hydrus_gui::{ImportOptionsWindow, MainWindow, Pages, ReviewImportsWindow, bind, headless};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use hydrus_store::queues;

use crate::common::widgets;

fn store() -> ([tempfile::TempDir; 2], Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    ([legacy, native], store)
}

/// The recording's labels of the button: before, and with locations set.
fn recorded_labels() -> (String, String) {
    let recording = hydrus_testkit::fixture_json("flow_manual_import.json");
    let steps = recording["steps"].as_array().unwrap();
    (
        steps[0]["dialog"]["import options"]
            .as_str()
            .unwrap()
            .to_owned(),
        steps[1]["dialog"]["button"].as_str().unwrap().to_owned(),
    )
}

/// The editor's page for `kind`, shown.
fn show_kind(editor: &ImportOptionsWindow, kind: &str) {
    for i in 0..editor.get_labels().row_count() {
        editor.invoke_kind_clicked(i32::try_from(i).unwrap());
        if editor.get_kind() == kind {
            return;
        }
    }
    panic!("no {kind} page");
}

#[test]
fn the_review_windows_import_options_button_edits_what_the_import_gets() {
    let (default_label, locations_label) = recorded_labels();
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let work = tempfile::tempdir().unwrap();
    std::fs::copy(
        hydrus_testkit::fixture_path("media/jpeg_420.jpg"),
        work.path().join("a.jpg"),
    )
    .unwrap();
    crate::files_io_file_menu::file_menu(&ui, &["import files\u{2026}"]);
    let review = || -> ReviewImportsWindow {
        bound
            .review_imports
            .borrow()
            .as_ref()
            .map(|(w, _)| w.clone_strong())
            .expect("the window is open")
    };
    review().invoke_path_entered(work.path().to_string_lossy().into_owned().into());
    for _ in 0..500 {
        slint::platform::update_timers_and_animations();
        if !review().get_working() && review().get_rows().row_count() > 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let window = review();
    widgets::lay_out(window.window(), 820.0, 560.0);
    let editor = || -> Option<ImportOptionsWindow> {
        bound
            .review_import_options
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
    };

    // the button, as labelled; clicked, the editor opens on a local import
    assert_eq!(window.get_import_options_label().as_str(), default_label);
    widgets::click(window.window(), &default_label);
    let opened = editor().expect("the editor opens");
    widgets::lay_out(opened.window(), 900.0, 620.0);
    // a second click while it is open opens no other
    widgets::click(window.window(), &default_label);
    assert!(std::ptr::eq(editor().unwrap().window(), opened.window()));

    // cancel: nothing changes
    show_kind(&opened, "locations");
    opened.set_custom_index(1);
    opened.invoke_changed();
    opened.set_archive(true);
    opened.invoke_changed();
    widgets::click(opened.window(), "cancel");
    assert!(editor().is_none(), "the editor closed");
    assert_eq!(window.get_import_options_label().as_str(), default_label);

    // apply: the button says what is set
    widgets::click(window.window(), &default_label);
    let opened = editor().expect("the editor opens again");
    widgets::lay_out(opened.window(), 900.0, 620.0);
    show_kind(&opened, "locations");
    assert_eq!(opened.get_custom_index(), 0, "the cancelled edit is gone");
    opened.set_custom_index(1);
    opened.invoke_changed();
    opened.set_archive(true);
    opened.invoke_changed();
    widgets::click(opened.window(), "apply");
    assert!(editor().is_none(), "the editor closed");
    assert_eq!(window.get_import_options_label().as_str(), locations_label);

    // opened again: as applied
    widgets::click(window.window(), &locations_label);
    let opened = editor().expect("the editor opens a third time");
    widgets::lay_out(opened.window(), 900.0, 620.0);
    show_kind(&opened, "locations");
    assert_eq!(opened.get_custom_index(), 1);
    assert!(opened.get_archive());
    widgets::click(opened.window(), "cancel");

    // and the import page gets it
    widgets::click(window.window(), "import now");
    assert!(bound.review_imports.borrow().is_none());
    let queue = bound
        .current
        .borrow()
        .borrow()
        .importer()
        .map(|i| i.queue)
        .expect("the import page");
    let options = store
        .read(move |c| queues::queue(c, queue))
        .unwrap()
        .unwrap()
        .options;
    let locations = options.locations.expect("custom locations");
    assert!(locations.automatically_archive, "{locations:?}");
    assert!(options.file_filtering.is_none() && options.tags.is_none());
}
