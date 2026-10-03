//! Files' filetypes forced from the thumbnails' "manage > force filetype"
//! (the reference's `EditFilesForcedFiletypePanel`): the file is taken as
//! the type chosen, renamed on disk to its extension, and forcing is
//! removed again. The dialog's text and choices are tested against the
//! reference's in hydrus-gui-model (`oracle/record_force_filetype.py`).

use std::sync::Arc;

use slint::{ComponentHandle, Model as _};

use hydrus_core::{HashId, Mime};
use hydrus_gui::{Bound, ForceFiletypeWindow, MainWindow, Pages, SearchPage, bind, headless};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use hydrus_store::media::FileInfo;

fn info(store: &Store, file: HashId) -> (FileInfo, hydrus_core::Sha256) {
    let services = store.snapshot().services.clone();
    let result = store
        .read(|c| hydrus_store::media::load(c, &services, None, &[file]))
        .unwrap()
        .results
        .remove(0);
    (result.info.unwrap(), result.hash)
}

/// "manage > force filetype" on the thumbnail selected.
fn open(ui: &MainWindow, bound: &Bound, index: i32) -> ForceFiletypeWindow {
    ui.invoke_thumbnail_menu_requested(index);
    let manage = ui.get_thumbnail_menu().manage;
    let id = (0..manage.row_count())
        .map(|i| manage.row_data(i).unwrap())
        .find(|r| r.label == "force filetype")
        .expect("manage > force filetype")
        .id;
    ui.invoke_menu_chosen(id);
    bound
        .force_filetype
        .borrow()
        .as_ref()
        .map(ComponentHandle::clone_strong)
        .expect("the dialog opens")
}

fn choices(dialog: &ForceFiletypeWindow) -> Vec<String> {
    dialog.get_choices().iter().map(|c| c.to_string()).collect()
}

#[test]
fn a_file_is_forced_to_another_filetype_and_back() {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store: Arc<Store> = Store::open(native.path()).unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let results = bound.current.borrow().borrow().results().to_vec();
    let index = results
        .iter()
        .position(|&f| info(&store, f).0.mime == Mime::ImageJpeg)
        .unwrap();
    let file = results[index];
    let (_, hash) = info(&store, file);
    let path = |mime: Mime| store.snapshot().storage.file_path(&hash, mime).unwrap();
    assert!(path(Mime::ImageJpeg).is_file());
    let index = i32::try_from(index).unwrap();
    ui.invoke_thumbnail_clicked(index, false, false);

    // one jpeg: not offered jpeg, nothing to remove
    let dialog = open(&ui, &bound, index);
    assert!(dialog.get_text().ends_with(
        "Of the 1 files, there are 1 jpeg. None are currently forced to be anything else."
    ));
    let offered = choices(&dialog);
    assert_eq!(offered[0], "image - png");
    assert!(!offered.contains(&"image - jpeg".to_owned()));
    dialog.set_chosen(0);
    dialog.invoke_apply();
    assert!(bound.force_filetype.borrow().is_none());
    let (forced, _) = info(&store, file);
    assert_eq!(
        (forced.mime, forced.original_mime),
        (Mime::ImagePng, Some(Mime::ImageJpeg))
    );
    assert!(path(Mime::ImagePng).is_file());
    assert!(!path(Mime::ImageJpeg).is_file());

    // forced: removing it is offered first, and undoes it
    let dialog = open(&ui, &bound, index);
    assert!(
        dialog
            .get_text()
            .ends_with("All are currently being forced: 1 png.")
    );
    assert_eq!(choices(&dialog)[0], "remove all forced filetypes");
    dialog.set_chosen(0);
    dialog.invoke_apply();
    let (back, _) = info(&store, file);
    assert_eq!((back.mime, back.original_mime), (Mime::ImageJpeg, None));
    assert!(path(Mime::ImageJpeg).is_file());
    assert!(!path(Mime::ImagePng).is_file());
}
