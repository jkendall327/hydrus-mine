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

fn wait_for_file_work(bound: &Bound) {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while bound.metadata_jobs.running() != 0 {
        assert!(
            std::time::Instant::now() < until,
            "metadata worker did not finish"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
        slint::platform::update_timers_and_animations();
    }
}

// leaf: audit-media-force-choose
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
    // This copied fixture's files belong to the test. Imported shared media
    // remains copy-only until the user has transferred ownership.
    store
        .write(|ctx| {
            hydrus_store::settings::set(
                ctx.conn(),
                &hydrus_store::transfer::MediaOwnership::default(),
            )
        })
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
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

    // Reference rejection schedules no worker; retired callbacks cannot affect
    // a successor or rewrite its captured target.
    let rejected = open(&ui, &bound, index);
    rejected.invoke_cancel();
    assert_eq!(bound.metadata_jobs.running(), 0);
    assert_eq!(info(&store, file).0.original_mime, None);
    let dialog = open(&ui, &bound, index);
    rejected.invoke_apply();
    rejected.invoke_cancel();
    assert!(bound.force_filetype.borrow().is_some());
    assert_eq!(bound.metadata_jobs.running(), 0);
    // one jpeg: not offered jpeg, nothing to remove
    assert!(dialog.get_text().ends_with(
        "Of the 1 files, there are 1 jpeg. None are currently forced to be anything else."
    ));
    let offered = choices(&dialog);
    assert_eq!(offered[0], "image - png");
    assert!(!offered.contains(&"image - jpeg".to_owned()));
    let another = results.iter().copied().find(|&id| id != file).unwrap();
    let another_index =
        i32::try_from(results.iter().position(|&id| id == another).unwrap()).unwrap();
    let another_before = info(&store, another).0;
    ui.invoke_thumbnail_clicked(another_index, false, false);
    dialog.set_chosen(0);
    dialog.invoke_apply();
    assert!(bound.force_filetype.borrow().is_none());
    wait_for_file_work(&bound);
    let after = info(&store, another).0;
    assert_eq!(
        (after.mime, after.original_mime),
        (another_before.mime, another_before.original_mime)
    );
    ui.invoke_thumbnail_clicked(index, false, false);
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
    wait_for_file_work(&bound);
    let (back, _) = info(&store, file);
    assert_eq!((back.mime, back.original_mime), (Mime::ImageJpeg, None));
    assert!(path(Mime::ImageJpeg).is_file());
    assert!(!path(Mime::ImagePng).is_file());
}

// leaf: audit-media-force-rename
#[test]
fn a_forced_file_is_renamed_or_when_the_media_is_shared_copied_with_its_old_copy_queued_for_cleanup()
 {
    use hydrus_store::file_maintenance::{self, JobType};
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store: Arc<Store> = Store::open(native.path()).unwrap();
    // (media shared with another install, as an in-place import leaves it,
    // is copied and never moved)
    let shared = legacy.path().to_path_buf();
    store
        .write(move |ctx| {
            hydrus_store::settings::set(
                ctx.conn(),
                &hydrus_store::transfer::MediaOwnership {
                    shared_with: Some(shared),
                },
            )
        })
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
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
    let jobs = |store: &Store| {
        store
            .read(move |c| file_maintenance::jobs_for(c, file))
            .unwrap()
    };
    assert!(!jobs(&store).contains(&JobType::DeleteNeighbourDupes));
    let index = i32::try_from(index).unwrap();
    ui.invoke_thumbnail_clicked(index, false, false);
    let dialog = open(&ui, &bound, index);
    dialog.set_chosen(0);
    dialog.invoke_apply();
    wait_for_file_work(&bound);
    let (forced, _) = info(&store, file);
    assert_eq!(
        (forced.mime, forced.original_mime),
        (Mime::ImagePng, Some(Mime::ImageJpeg))
    );
    // the new extension exists and the shared original was left in place
    assert!(path(Mime::ImagePng).is_file());
    assert!(path(Mime::ImageJpeg).is_file());
    // and the extra copy is queued for the "delete neighbour dupes" job
    assert!(jobs(&store).contains(&JobType::DeleteNeighbourDupes));
}

/// Force the fixture's jpeg to png through the thumbnail menu and report the
/// jpeg's and the png's paths, bytes and modification times as the file
/// system holds them. `shared` marks the media as shared with another
/// install, which makes the job copy instead of move.
fn force_and_read_the_disk(
    shared: bool,
) -> (std::path::PathBuf, std::path::PathBuf, impl Sized) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store: Arc<Store> = Store::open(native.path()).unwrap();
    let ownership = hydrus_store::transfer::MediaOwnership {
        shared_with: shared.then(|| legacy.path().to_path_buf()),
    };
    store
        .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &ownership))
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let results = bound.current.borrow().borrow().results().to_vec();
    let index = results
        .iter()
        .position(|&f| info(&store, f).0.mime == Mime::ImageJpeg)
        .unwrap();
    let (_, hash) = info(&store, results[index]);
    let path = |mime: Mime| store.snapshot().storage.file_path(&hash, mime).unwrap();
    let (jpeg, png) = (path(Mime::ImageJpeg), path(Mime::ImagePng));
    // Give the file a modification time no copy would get by accident.
    let before = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_500_000_000);
    std::fs::File::options()
        .write(true)
        .open(&jpeg)
        .unwrap()
        .set_modified(before)
        .unwrap();
    let bytes = std::fs::read(&jpeg).unwrap();
    let index = i32::try_from(index).unwrap();
    ui.invoke_thumbnail_clicked(index, false, false);
    let dialog = open(&ui, &bound, index);
    dialog.set_chosen(0);
    dialog.invoke_apply();
    wait_for_file_work(&bound);
    assert_eq!(std::fs::read(&png).unwrap(), bytes);
    assert_eq!(std::fs::metadata(&png).unwrap().modified().unwrap(), before);
    // the directories live as long as the caller checks the paths
    (jpeg, png, (native, legacy))
}

// leaf: audit-media-force-rename
// Native-only (the reference's forced filetypes are not stored under a new
// extension): the file system is real, and so are the window, the job and
// the store.
#[test]
fn forcing_a_filetype_moves_the_file_to_its_new_extension_or_copies_shared_media_keeping_its_times()
 {
    let (jpeg, png, _dirs) = force_and_read_the_disk(false);
    assert!(png.is_file());
    assert!(!jpeg.exists(), "owned media is renamed, not left behind");
    let (jpeg, png, _dirs) = force_and_read_the_disk(true);
    assert!(png.is_file());
    assert!(jpeg.is_file(), "shared media is copied, never moved");
}
