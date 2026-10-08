//! Manual export's window against the reference's panel
//! (`ClientGUIExport.ExportPanel`): removing selected rows after asking
//! and renumbering, the destination's browse and open-location buttons, and
//! copies, symlinks and trashing (the preview and the questions against the
//! recording are in hydrus-gui-model's tests).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use slint::Model as _;

use hydrus_core::HashId;
use hydrus_gui::export_files_window::{self, Slots};
use hydrus_gui::headless;

fn open(
    store: &Arc<hydrus_store::Store>,
    files: &[HashId],
    slots: &Slots,
) -> hydrus_gui::ExportFilesWindow {
    export_files_window::open(store, files.to_vec(), slots, Rc::new(|| {})).unwrap()
}

/// (number, filetype, destination) per row.
fn rows(window: &hydrus_gui::ExportFilesWindow) -> Vec<Vec<String>> {
    let rows = window.get_rows();
    (0..rows.row_count())
        .map(|r| {
            let cells = rows.row_data(r).unwrap().cells;
            (0..cells.row_count())
                .map(|c| cells.row_data(c).unwrap().to_string())
                .collect()
        })
        .collect()
}

fn finish(window: &hydrus_gui::ExportFilesWindow) {
    let started = Instant::now();
    while window.get_working() && started.elapsed() < Duration::from_secs(15) {
        std::thread::sleep(Duration::from_millis(20));
        slint::platform::update_timers_and_animations();
    }
    assert!(!window.get_working());
}

// leaf: audit-network-export-remove
#[test]
fn selected_rows_are_removed_after_asking_and_the_names_are_made_again() {
    let (_dirs, store) = crate::subscriptions::store();
    let _windows = headless::init();
    let slots = Slots::default();
    let work = tempfile::tempdir().unwrap();
    let window = open(&store, &[HashId(1), HashId(8), HashId(3)], &slots);
    window.set_destination(work.path().to_string_lossy().into_owned().into());
    window.set_phrase("{#}".into());
    window.invoke_update();
    let names = |w: &hydrus_gui::ExportFilesWindow| -> Vec<String> {
        rows(w)
            .iter()
            .map(|r| {
                std::path::Path::new(&r[2])
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect()
    };
    assert_eq!(names(&window), ["1.png", "2.png", "3.flac"]);

    // nothing selected: nothing to ask
    window.invoke_remove();
    assert!(!window.get_asking());

    // the first two rows (click, then shift-click); asked, and declined
    window.invoke_row_clicked(0, false, false);
    window.invoke_row_clicked(1, false, true);
    window.invoke_remove();
    assert!(window.get_asking());
    assert_eq!(window.get_question(), "Remove all selected?");
    window.invoke_answer(1);
    assert!(!window.get_asking());
    assert_eq!(names(&window), ["1.png", "2.png", "3.flac"]);
    // asked again, and accepted: the last file is now the first
    window.invoke_remove();
    window.invoke_answer(0);
    assert_eq!(names(&window), ["1.flac"]);
    let left = rows(&window);
    assert_eq!(left[0][0], "1");
    window.invoke_dismissed();
    assert!(slots.window.borrow().is_none());
}

// leaf: audit-network-export-paths
#[test]
fn the_destination_is_browsed_for_and_its_location_opened() {
    let (_dirs, store) = crate::subscriptions::store();
    let _windows = headless::init();
    let slots = Slots::default();
    let picked = tempfile::tempdir().unwrap();
    let asked: Rc<RefCell<Vec<(hydrus_gui::Pick, String)>>> = Rc::default();
    hydrus_gui::set_picker({
        let asked = asked.clone();
        let picked = picked.path().to_path_buf();
        move |kind, title| {
            asked.borrow_mut().push((kind, title.to_owned()));
            vec![picked.clone()]
        }
    });
    let launched: Rc<RefCell<Vec<String>>> = Rc::default();
    hydrus_gui::set_launcher({
        let launched = launched.clone();
        move |target| launched.borrow_mut().push(target.to_owned())
    });
    let window = open(&store, &[HashId(1), HashId(3)], &slots);
    window.set_phrase("{hash}".into());
    window.set_destination("".into());
    window.invoke_open_location();
    assert!(
        launched.borrow().is_empty(),
        "an empty location does nothing"
    );

    // browse: the picked folder is the destination, and the preview moves
    window.invoke_browse();
    assert_eq!(
        *asked.borrow(),
        [(hydrus_gui::Pick::Folder, "Select directory".to_owned())]
    );
    assert_eq!(
        window.get_destination(),
        picked.path().to_string_lossy().as_ref()
    );
    let shown = rows(&window);
    assert_eq!(shown.len(), 2);
    assert!(
        shown
            .iter()
            .all(|r| r[2].starts_with(picked.path().to_str().unwrap()))
    );

    // open location: the folder is launched; one that is gone says so
    window.invoke_open_location();
    assert_eq!(
        *launched.borrow(),
        [picked.path().to_string_lossy().into_owned()]
    );
    let gone = picked.path().join("gone");
    window.set_destination(gone.to_string_lossy().into_owned().into());
    window.invoke_open_location();
    assert_eq!(launched.borrow().len(), 1);
    assert_eq!(window.get_status(), "That location does not seem to exist!");
    window.invoke_dismissed();
}

// leaf: audit-network-export-policy
#[cfg(unix)]
#[test]
fn files_are_copied_or_symlinked_and_trashing_asks_and_wins_over_symlinks() {
    let (_dirs, store) = crate::subscriptions::store();
    let _windows = headless::init();
    let slots = Slots::default();
    let work = tempfile::tempdir().unwrap();
    let changes = Rc::new(Cell::new(0));

    // a plain export: no question, regular files
    let window = open(&store, &[HashId(1), HashId(3)], &slots);
    let copies = work.path().join("copies");
    std::fs::create_dir(&copies).unwrap();
    window.set_destination(copies.to_string_lossy().into_owned().into());
    window.set_phrase("{#}".into());
    window.invoke_update();
    window.invoke_export(false);
    assert!(!window.get_asking(), "a plain copy asks nothing");
    finish(&window);
    assert_eq!(window.get_status(), "done!");
    for name in ["1.png", "2.flac"] {
        let meta = std::fs::symlink_metadata(copies.join(name)).unwrap();
        assert!(meta.is_file() && !meta.file_type().is_symlink(), "{name}");
    }
    window.invoke_dismissed();

    // symlinks: links to the client's files, again no question
    let window = open(&store, &[HashId(1), HashId(3)], &slots);
    let links = work.path().join("links");
    std::fs::create_dir(&links).unwrap();
    window.set_destination(links.to_string_lossy().into_owned().into());
    window.set_phrase("{#}".into());
    window.set_symlinks(true);
    window.invoke_update();
    window.invoke_export(false);
    assert!(!window.get_asking());
    finish(&window);
    for name in ["1.png", "2.flac"] {
        let link = links.join(name);
        assert!(
            std::fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink(),
            "{name}"
        );
        assert_eq!(
            std::fs::read(&link).unwrap(),
            std::fs::read(copies.join(name)).unwrap()
        );
    }
    window.invoke_dismissed();

    // trashing too: the warning comes first, and trashing wins, so the
    // files are copied; export-and-close is asked about on its own
    let window = open(&store, &[HashId(1), HashId(3)], &slots);
    let trashed = work.path().join("trashed");
    std::fs::create_dir(&trashed).unwrap();
    window.set_destination(trashed.to_string_lossy().into_owned().into());
    window.set_phrase("{#}".into());
    window.set_symlinks(true);
    window.set_trash(true);
    window.invoke_update();
    window.invoke_export(false);
    assert!(window.get_asking());
    assert_eq!(
        window.get_question(),
        hydrus_gui_model::export_files::TRASH_WARNING
    );
    window.invoke_answer(1);
    assert!(!window.get_working());
    assert_eq!(std::fs::read_dir(&trashed).unwrap().count(), 0, "declined");
    window.invoke_export(false);
    window.invoke_answer(0);
    finish(&window);
    let meta = std::fs::symlink_metadata(trashed.join("1.png")).unwrap();
    assert!(meta.is_file() && !meta.file_type().is_symlink());
    let _ = changes;
    window.invoke_dismissed();
}
