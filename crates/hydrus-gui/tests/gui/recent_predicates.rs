//! Recent system predicates in the editor window: what an editor adds is
//! kept, shown the next time it opens, added again from there, and
//! forgotten from there. (Kept, shown and migrated as the reference's are:
//! hydrus-gui-model's tests.)

use std::sync::Arc;

use slint::{ComponentHandle as _, Model as _};

use hydrus_core::search::recent::RecentPredicates;
use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;

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

/// In the window: what an editor adds is kept, shown the next time it
/// opens, added again from there, and forgotten from there.
#[test]
fn the_editor_window_keeps_and_shows_recent_predicates() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let editor = || {
        bound
            .predicate_editor
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
            .expect("an editor")
    };
    let open = |text: &str| {
        ui.invoke_search_edited("".into());
        let i = ui
            .get_suggestions()
            .iter()
            .position(|s| s.text == text)
            .unwrap();
        ui.invoke_suggestion_chosen(i32::try_from(i).unwrap());
        editor()
    };
    let recent = |window: &hydrus_gui::PredicateEditorWindow| -> Vec<String> {
        window.get_recent().iter().map(|s| s.to_string()).collect()
    };
    let kept = || -> RecentPredicates { store.read(hydrus_store::settings::get).unwrap() };
    // none at first
    let window = open("system:filesize");
    assert!(recent(&window).is_empty());
    // "ok" adds and keeps the panel's
    window.invoke_ok(0);
    assert_eq!(kept().by_type[&10].len(), 1);
    let window = open("system:filesize");
    assert_eq!(recent(&window), ["system:filesize < 200KB"]);
    // a ready-made button's are kept too, but not shown (it has a button)
    let window = open("system:limit");
    window.invoke_button_clicked(0);
    assert_eq!(kept().by_type[&9].len(), 1);
    assert!(recent(&open("system:limit")).is_empty());
    // the recent one is entered again (taking it out of the search, which
    // has it, as entering it again does), and stays; once more, it is back
    let searched = || -> Vec<String> {
        ui.get_predicates()
            .iter()
            .map(|p| p.text.to_string())
            .collect()
    };
    assert!(searched().contains(&"system:filesize < 200KB".to_owned()));
    let window = open("system:filesize");
    window.invoke_recent_clicked(0);
    assert!(bound.predicate_editor.borrow().is_none());
    assert_eq!(searched(), ["system:limit is 64"]);
    open("system:filesize").invoke_recent_clicked(0);
    assert!(searched().contains(&"system:filesize < 200KB".to_owned()));
    assert_eq!(kept().by_type[&10].len(), 1);
    // forgotten, it goes from the editor and the store
    let window = open("system:filesize");
    window.invoke_recent_forgotten(0);
    assert!(recent(&window).is_empty());
    assert!(kept().by_type[&10].is_empty());
}
