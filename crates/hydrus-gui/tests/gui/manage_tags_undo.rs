//! What hydrus-rs's undo menu does with a manage tags edit, which the
//! reference's cannot undo (its dialog commits past its undo manager; see
//! DIFFERENCES.md, "Content undo", and the manage tags flow): a tag added
//! on "second tags" and one removed there, applied; undo takes both back on
//! that service alone, and redo puts them back; "my tags" is untouched
//! throughout.

use std::collections::BTreeSet;
use std::sync::Arc;

use slint::{ComponentHandle as _, Model as _};

use hydrus_core::{HashId, ServiceId};
use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;

use crate::common::{main_menu, widgets};

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

/// A file's current tags on `service`.
fn tags(store: &Store, file: HashId, service: ServiceId) -> BTreeSet<String> {
    let snapshot = store.snapshot();
    store
        .read(|conn| {
            let batch = hydrus_store::media::load(conn, &snapshot.services, None, &[file])?;
            Ok(batch.results[0]
                .tags
                .get(&service)
                .and_then(|t| t.by_status.get(&hydrus_core::ContentStatus::Current))
                .map(|t| {
                    t.iter()
                        .map(|t| batch.tags[t].as_str().to_owned())
                        .collect()
                })
                .unwrap_or_default())
        })
        .unwrap()
}

/// The undo menu's undo and redo items.
fn undo_items(ui: &MainWindow) -> (Option<String>, Option<String>) {
    let lines = main_menu::open(ui, &["undo"]);
    ui.invoke_menu_dismissed();
    let find = |prefix: &str| {
        lines
            .iter()
            .find(|l| l.0.starts_with(prefix))
            .map(|l| l.0.clone())
    };
    (find("undo "), find("redo "))
}

#[test]
fn a_manage_tags_edit_is_undone_and_redone_on_its_own_service() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let services = store.snapshot().services.clone();
    let second = services.by_name("second tags").unwrap().id;
    let mine = services.by_name("my tags").unwrap().id;
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    widgets::settle();
    // a file with something on second tags to take away, if any has
    let results = bound.current.borrow().borrow().results().to_vec();
    let (at, file) = results
        .iter()
        .enumerate()
        .find(|(_, f)| !tags(&store, **f, second).is_empty())
        .map_or((0, results[0]), |(i, f)| (i, *f));
    let second_before = tags(&store, file, second);
    let mine_before = tags(&store, file, mine);
    let removed = second_before.iter().next().cloned();

    ui.invoke_select_none();
    ui.invoke_thumbnail_clicked(i32::try_from(at).unwrap(), false, false);
    ui.invoke_manage_tags_selected();
    let manage = bound
        .manage_tags
        .borrow()
        .as_ref()
        .expect("the manage tags dialog")
        .clone_strong();
    let tab = manage
        .get_service_names()
        .iter()
        .position(|s| s == "second tags")
        .unwrap();
    manage.invoke_service_chosen(i32::try_from(tab).unwrap());
    widgets::settle();
    manage.invoke_text_edited("undo me".into());
    manage.invoke_entered();
    widgets::settle();
    if let Some(tag) = &removed {
        let row = manage
            .get_tags()
            .iter()
            .position(|r| r.text.starts_with(&format!("{tag} (")))
            .unwrap();
        manage.invoke_tag_clicked(i32::try_from(row).unwrap(), false, false);
        manage.invoke_remove_pressed();
        widgets::settle();
        if !manage.get_tag_menu_question().is_empty() {
            manage.invoke_tag_menu_answered(true);
            widgets::settle();
        }
    }
    manage.invoke_apply();
    widgets::settle();

    let mut edited = second_before.clone();
    edited.insert("undo me".to_owned());
    if let Some(tag) = &removed {
        edited.remove(tag);
    }
    assert_eq!(tags(&store, file, second), edited, "applied");
    assert_eq!(tags(&store, file, mine), mine_before);

    // undo: back as it was, on second tags only
    let (undo, redo) = undo_items(&ui);
    let undo = undo.expect("an undo item");
    assert!(undo.contains("second tags"), "{undo}");
    assert!(redo.is_none());
    main_menu::choose(&ui, &["undo"], &undo);
    widgets::settle();
    assert_eq!(tags(&store, file, second), second_before, "undone");
    assert_eq!(tags(&store, file, mine), mine_before);

    // redo: the edit again
    let (undo, redo) = undo_items(&ui);
    assert!(undo.is_none(), "{undo:?}");
    let redo = redo.expect("a redo item");
    main_menu::choose(&ui, &["undo"], &redo);
    widgets::settle();
    assert_eq!(tags(&store, file, second), edited, "redone");
    assert_eq!(tags(&store, file, mine), mine_before);
    let (undo, redo) = undo_items(&ui);
    assert!(undo.is_some() && redo.is_none());
}
