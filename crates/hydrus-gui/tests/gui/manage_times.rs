//! Files' times, managed from the thumbnails' "manage > times" (the
//! reference's `EditFileTimestampsPanel`): a time edited in the date-time
//! editor and a web domain time added are written when applied, the file
//! modified time to the file on disk too. The dialog's and editor's steps
//! are tested against the reference's in hydrus-gui-model
//! (`oracle/record_manage_times.py`, `oracle/record_datetime_editor.py`).

use std::sync::Arc;

use slint::{ComponentHandle, Model as _};

use hydrus_core::HashId;
use hydrus_gui::{
    Bound, DateTimeEditorWindow, MainWindow, ManageTimesWindow, Pages, SearchPage, bind, headless,
};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use hydrus_store::media::MediaResult;

fn load(store: &Store, file: HashId) -> MediaResult {
    let services = store.snapshot().services.clone();
    store
        .read(|c| hydrus_store::media::load(c, &services, None, &[file]))
        .unwrap()
        .results
        .remove(0)
}

/// "manage > times" on the thumbnails selected.
fn open(ui: &MainWindow, bound: &Bound, index: i32) -> ManageTimesWindow {
    ui.invoke_thumbnail_menu_requested(index);
    let manage = ui.get_thumbnail_menu().manage;
    let id = (0..manage.row_count())
        .map(|i| manage.row_data(i).unwrap())
        .find(|r| r.label == "times")
        .expect("manage > times")
        .id;
    ui.invoke_menu_chosen(id);
    bound
        .manage_times
        .borrow()
        .as_ref()
        .map(ComponentHandle::clone_strong)
        .expect("the dialog opens")
}

fn editor(bound: &Bound) -> DateTimeEditorWindow {
    bound
        .datetime_editor
        .borrow()
        .as_ref()
        .map(ComponentHandle::clone_strong)
        .expect("the date-time editor opens")
}

fn rows(dialog: &ManageTimesWindow) -> Vec<(String, String, bool)> {
    let times = dialog.get_times();
    (0..times.row_count())
        .map(|i| times.row_data(i).unwrap())
        .map(|r| (r.label.to_string(), r.text.to_string(), r.enabled))
        .collect()
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

// leaf: audit-media-times-main, audit-media-times-domain, audit-media-times-disk
#[test]
fn times_are_edited_added_and_applied_as_the_reference_does() {
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
    let results = bound.current.borrow().borrow().results().to_vec();
    // a file with a modified time
    let index = results
        .iter()
        .position(|&f| load(&store, f).info.and_then(|i| i.file_modified).is_some())
        .unwrap();
    let file = results[index];
    let index = i32::try_from(index).unwrap();
    ui.invoke_thumbnail_clicked(index, false, false);
    let initial_modified = load(&store, file).info.unwrap().file_modified;
    let retired = open(&ui, &bound, index);
    retired.invoke_time_clicked(0);
    let retired_child = editor(&bound);
    retired_child.set_date("2020-01-02".into());
    retired_child.invoke_edited();
    retired.invoke_apply(); // parent acceptance cannot bypass its child
    assert!(bound.manage_times.borrow().is_some());
    assert_eq!(bound.metadata_jobs.running(), 0);
    retired.invoke_cancel();
    assert!(bound.datetime_editor.borrow().is_none());
    let dialog = open(&ui, &bound, index);
    dialog.invoke_time_clicked(0);
    let successor_child = editor(&bound);
    retired_child.invoke_apply();
    retired_child.invoke_cancel();
    retired.invoke_apply();
    retired.invoke_cancel();
    assert!(bound.manage_times.borrow().is_some());
    assert!(bound.datetime_editor.borrow().is_some());
    assert_eq!(
        load(&store, file).info.unwrap().file_modified,
        initial_modified
    );
    successor_child.invoke_cancel();
    dialog.invoke_cancel();
    let dialog = open(&ui, &bound, index);
    assert_eq!(dialog.get_window_title(), "manage times");
    let shown = rows(&dialog);
    assert_eq!(shown[0].0, "file modified time: ");
    assert!(shown[0].2);
    assert!(dialog.get_can_copy());
    assert_eq!(dialog.get_warning(), "");
    // (the newest window, drawn)
    let last = (0..100)
        .take_while(|&n| windows.get(n).is_some())
        .last()
        .unwrap();
    let pixels = headless::render(&windows.get(last).unwrap(), 720, 620);
    let shot = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("manage_times.png");
    headless::save_png(&shot, &pixels, 720, 620).unwrap();

    // the modified time, edited: the editor shows it, and gives 2020-01-02
    dialog.invoke_time_clicked(0);
    let edit = editor(&bound);
    assert_eq!(edit.get_label(), "");
    assert!(!edit.get_step_shown());
    edit.set_date("2020-01-02".into());
    edit.set_time("03:04:05.678".into());
    edit.invoke_edited();
    assert!(
        edit.get_value().contains(":04:05.678"),
        "{}",
        edit.get_value()
    );
    edit.invoke_apply();
    assert!(bound.datetime_editor.borrow().is_none());
    assert!(rows(&dialog)[0].1.contains(":04:05.678"));
    assert_eq!(
        dialog.get_warning(),
        "This will also change the modified time of the file on disk!"
    );

    // a web domain time added, at now unless edited
    dialog.invoke_add_domain();
    assert_eq!(dialog.get_asking_message(), "Enter domain.");
    dialog.set_asking_text("example.org".into());
    dialog.invoke_chosen(0);
    let edit = editor(&bound);
    edit.set_date("2021-06-07".into());
    edit.set_time("08:09:10.000".into());
    edit.invoke_edited();
    edit.invoke_apply();
    let domains = dialog.get_domains();
    let cells: Vec<String> = (0..domains.row_count())
        .flat_map(|i| {
            let cells = domains.row_data(i).unwrap().cells;
            (0..cells.row_count()).map(move |c| cells.row_data(c).unwrap().to_string())
        })
        .collect();
    assert!(cells.contains(&"example.org".to_owned()), "{cells:?}");
    // the same domain again is refused
    dialog.invoke_add_domain();
    dialog.set_asking_text("example.org".into());
    dialog.invoke_chosen(0);
    assert_eq!(
        dialog.get_asking_message(),
        "Sorry, that domain already exists!"
    );
    dialog.invoke_chosen(0);
    assert!(!dialog.get_asking());

    // applied: both written, and the file on disk changed
    dialog.invoke_apply();
    assert!(bound.manage_times.borrow().is_none());
    let result = load(&store, file);
    let modified = result.info.as_ref().unwrap().file_modified.unwrap().0;
    let expected_ms = jiff::civil::date(2020, 1, 2)
        .at(3, 4, 5, 678_000_000)
        .to_zoned(jiff::tz::TimeZone::system())
        .unwrap()
        .timestamp()
        .as_millisecond();
    assert_eq!(modified, expected_ms);
    let domain = result
        .domain_modified
        .iter()
        .find(|(d, _)| d == "example.org")
        .map(|(_, t)| t.0);
    let expected_domain = jiff::civil::date(2021, 6, 7)
        .at(8, 9, 10, 0)
        .to_zoned(jiff::tz::TimeZone::system())
        .unwrap()
        .timestamp()
        .as_millisecond();
    assert_eq!(domain, Some(expected_domain));
    let snapshot = store.snapshot();
    let path = snapshot
        .storage
        .file_path(&result.hash, result.info.as_ref().unwrap().mime)
        .unwrap();
    wait_for_file_work(&bound);
    let on_disk = std::fs::metadata(&path).unwrap().modified().unwrap();
    let on_disk_ms = on_disk
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    assert_eq!(i64::try_from(on_disk_ms).unwrap(), expected_ms);
}
