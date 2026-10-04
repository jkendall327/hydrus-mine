//! Manual export opens from both media menus, previews and removes files,
//! confirms trash/close, edits sidecars, and completes exports asynchronously.
use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
use hydrus_store::{Store, import::import_legacy};
use slint::{ComponentHandle, Model};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[test]
fn export_files_menu_window_previews_confirmation_and_worker() {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store: Arc<Store> = Store::open(native.path()).unwrap();
    let work = tempfile::tempdir().unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    ui.invoke_thumbnail_clicked(0, false, false);
    ui.invoke_thumbnail_menu_requested(0);
    let share = ui.get_thumbnail_menu().share_export;
    let export = (0..share.row_count())
        .map(|i| share.row_data(i).unwrap())
        .find(|r| r.label == "export files")
        .expect("share > export files");
    ui.invoke_menu_chosen(export.id);
    let dialog = bound
        .export_files
        .window
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    dialog.set_destination(work.path().to_string_lossy().into_owned().into());
    dialog.set_phrase("{#} {file_id}".into());
    dialog.invoke_update();
    assert_eq!(dialog.get_rows().row_count(), 1);
    assert!(
        dialog
            .get_rows()
            .row_data(0)
            .unwrap()
            .cells
            .row_data(2)
            .unwrap()
            .starts_with(work.path().to_str().unwrap())
    );
    let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 880, 610);
    let shot = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("export_files.png");
    headless::save_png(&shot, &pixels, 880, 610).unwrap();
    dialog.set_trash(true);
    dialog.invoke_export(false);
    assert!(dialog.get_asking());
    assert_eq!(
        dialog.get_question(),
        hydrus_gui_model::export_files::TRASH_WARNING
    );
    dialog.invoke_answer(1);
    assert!(!dialog.get_working());
    dialog.set_trash(false);
    dialog.invoke_export(false);
    assert!(dialog.get_working());
    let started = Instant::now();
    while dialog.get_working() && started.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(20));
        slint::platform::update_timers_and_animations();
    }
    assert!(!dialog.get_working());
    assert_eq!(dialog.get_status(), "done!");
    assert_eq!(std::fs::read_dir(work.path()).unwrap().count(), 1);
    dialog.invoke_edit_sidecars();
    let routers = bound
        .export_files
        .sidecars
        .routers
        .borrow()
        .as_ref()
        .expect("sidecar editor stays open")
        .clone_strong();
    routers.invoke_template(0);
    routers.invoke_apply();
    assert!(bound.export_files.sidecars.routers.borrow().is_none());
    assert_ne!(dialog.get_sidecars(), "no sidecars");
    dialog.invoke_row_clicked(0, false, false);
    dialog.invoke_remove();
    assert_eq!(dialog.get_question(), "Remove all selected?");
    dialog.invoke_answer(0);
    assert_eq!(dialog.get_rows().row_count(), 0);
    dialog.invoke_dismissed();
    assert!(bound.export_files.window.borrow().is_none());
    // The viewer opens the same dialog for just its displayed file.
    let index = {
        let page = bound.current.borrow();
        let page = page.borrow();
        page.results()
            .iter()
            .position(|&f| {
                hydrus_gui::viewer_menu::player(&store, f)
                    == hydrus_gui::viewer_menu::Player::StaticImage
            })
            .unwrap()
    };
    ui.invoke_thumbnail_activated(i32::try_from(index).unwrap());
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    viewer.invoke_context_menu_requested();
    let share = viewer.get_context_menu().share_export;
    let export = (0..share.row_count())
        .map(|i| share.row_data(i).unwrap())
        .find(|r| r.label == "export files")
        .unwrap();
    viewer.invoke_menu_chosen(export.id);
    let dialog = bound
        .export_files
        .window
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(dialog.get_rows().row_count(), 1);
    dialog.invoke_export(true);
    assert!(dialog.get_asking());
    assert_eq!(dialog.get_question(), "Export as shown?");
    dialog.invoke_answer(1);
    assert!(bound.export_files.window.borrow().is_none());
    viewer.invoke_close_requested();
}

#[test]
fn shared_pattern_shortcuts_copy_without_editing_and_guard_closed_owners() {
    use hydrus_gui::export_files_window::{self, Slots};
    use std::{cell::RefCell, rc::Rc};
    let (_dirs, store) = crate::subscriptions::store();
    let rendered = headless::init();
    let copied = Rc::new(RefCell::new(String::new()));
    hydrus_gui::set_clipper({
        let copied = copied.clone();
        move |clip| {
            if let hydrus_gui::Clip::Text(text) = clip {
                copied.borrow_mut().clone_from(text);
            }
        }
    });
    let reference = hydrus_testkit::fixture_json("export_pattern_shortcuts.json");
    let slots = Slots::default();
    let window =
        export_files_window::open(&store, vec![hydrus_core::HashId(1)], &slots, Rc::new(|| {}))
            .unwrap();
    window.set_phrase("keep {hash}".into());
    window.invoke_update();
    let initial_rows = window
        .get_rows()
        .row_data(0)
        .unwrap()
        .cells
        .iter()
        .collect::<Vec<_>>();
    assert_eq!(
        window.get_pattern_heading(),
        reference["menu"][0]["label"].as_str().unwrap()
    );
    let labels = window
        .get_pattern_shortcuts()
        .iter()
        .map(|label| label.to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        labels,
        reference["menu"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| !row["separator"].as_bool().unwrap())
            .skip(1)
            .map(|row| row["label"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    );
    for (index, expected) in std::iter::once(7)
        .chain(0..7)
        .zip(reference["copied"].as_array().unwrap())
    {
        window.invoke_pattern_chosen(index);
        assert_eq!(*copied.borrow(), expected.as_str().unwrap());
        assert_eq!(
            window.get_phrase(),
            reference["phrase_after"].as_str().unwrap()
        );
        assert_eq!(
            window
                .get_rows()
                .row_data(0)
                .unwrap()
                .cells
                .iter()
                .collect::<Vec<_>>(),
            initial_rows
        );
    }
    let previous = copied.borrow().clone();
    window.invoke_pattern_chosen(-1);
    window.invoke_pattern_chosen(8);
    assert_eq!(*copied.borrow(), previous);
    window.set_working(true);
    window.invoke_pattern_chosen(0);
    assert_eq!(*copied.borrow(), previous);
    window.set_working(false);
    window.set_asking(true);
    window.invoke_pattern_chosen(0);
    assert_eq!(*copied.borrow(), previous);
    window.set_asking(false);
    window.invoke_edit_sidecars();
    window.invoke_pattern_chosen(0);
    assert_eq!(*copied.borrow(), previous);
    slots.sidecars.cancel();
    // The copied phrase reaches the existing real filename consumer only after
    // the user pastes it and accepts the filename edit.
    let destination = tempfile::tempdir().unwrap();
    window.set_destination(destination.path().to_string_lossy().into_owned().into());
    window.invoke_pattern_chosen(0);
    window.set_phrase(copied.borrow().clone().into());
    window.invoke_update();
    assert_eq!(
        window
            .get_rows()
            .row_data(0)
            .unwrap()
            .cells
            .row_data(2)
            .unwrap()
            .to_string(),
        destination.path().join("1.png").to_string_lossy()
    );
    let pixels = headless::render(&rendered.get(0).unwrap(), 880, 610);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("export-pattern-shortcuts.png"),
        &pixels,
        880,
        610,
    )
    .unwrap();
    window.invoke_dismissed();
    let before = copied.borrow().clone();
    window.invoke_pattern_chosen(1);
    assert_eq!(*copied.borrow(), before);
    assert_eq!(std::fs::read_dir(destination.path()).unwrap().count(), 0);
    // The concrete folder owner uses the same menu and closes its callbacks.
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    crate::folders::open(&ui, "manage export folders\u{2026}");
    let list = bound
        .folders
        .export_list
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    list.invoke_add();
    let folder = bound
        .folders
        .export_edit
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    folder.set_phrase("keep {hash}".into());
    for (index, expected) in std::iter::once(7)
        .chain(0..7)
        .zip(reference["copied"].as_array().unwrap())
    {
        folder.invoke_pattern_chosen(index);
        assert_eq!(*copied.borrow(), expected.as_str().unwrap());
        assert_eq!(folder.get_phrase(), "keep {hash}");
    }
    folder.invoke_edit_sidecars();
    let before = copied.borrow().clone();
    folder.invoke_pattern_chosen(0);
    assert_eq!(*copied.borrow(), before);
    folder.invoke_cancel();
    folder.invoke_pattern_chosen(0);
    assert_eq!(*copied.borrow(), before);
    assert!(bound.folders.sidecars.routers.borrow().is_none());
    list.invoke_cancel();
    let stored = store
        .read(hydrus_store::settings::get::<hydrus_store::settings::ExportFolders>)
        .unwrap();
    assert!(stored.0.is_empty());
}
