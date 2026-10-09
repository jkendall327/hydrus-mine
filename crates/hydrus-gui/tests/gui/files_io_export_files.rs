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

/// The recording of the reference's panel naming, removing and browsing
/// (oracle/record_export_names.py), its scratch folder as `work`.
fn recorded(work: &std::path::Path) -> serde_json::Value {
    let text = std::fs::read_to_string(hydrus_testkit::fixture_path("export_names.json")).unwrap();
    serde_json::from_str(&text.replace("{work}", work.to_str().unwrap())).unwrap()
}

fn strings(v: &serde_json::Value) -> Vec<Vec<String>> {
    serde_json::from_value(v.clone()).unwrap()
}

// leaf: audit-network-export-remove
#[test]
fn selected_rows_are_removed_after_asking_and_renumbered_as_the_reference_does() {
    let (_dirs, store) = crate::subscriptions::store();
    let _windows = headless::init();
    let work = tempfile::tempdir().unwrap();
    let recorded = recorded(work.path());
    let all: Vec<HashId> = recorded["remove_files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| HashId(u32::try_from(f["file_id"].as_u64().unwrap()).unwrap()))
        .collect();
    for removal in recorded["removals"].as_array().unwrap() {
        let slots = Slots::default();
        let window = open(&store, &all, &slots);
        let phrase = removal["phrase"].as_str().unwrap();
        window.set_destination("/tmp/hx".into());
        window.set_phrase(phrase.into());
        window.invoke_update();
        let steps = removal["steps"].as_array().unwrap();
        assert_eq!(rows(&window), strings(&steps[0]["rows"]), "{phrase}");
        let mut kept = all.clone();
        for step in &steps[1..] {
            // the recorded files selected: a click, then ctrl-clicks
            let selected: Vec<HashId> = step["selected"]
                .as_array()
                .unwrap()
                .iter()
                .map(|i| all[i.as_u64().unwrap() as usize])
                .collect();
            for (n, file) in selected.iter().enumerate() {
                let at = kept.iter().position(|f| f == file).unwrap();
                window.invoke_row_clicked(at as i32, n > 0, false);
            }
            window.invoke_remove();
            let asked: Vec<String> = serde_json::from_value(step["asked"].clone()).unwrap();
            assert_eq!(window.get_asking(), !asked.is_empty(), "{step}");
            if window.get_asking() {
                assert_eq!(window.get_question(), asked[0]);
                let yes = step["yes"].as_bool().unwrap();
                window.invoke_answer(i32::from(!yes));
                assert!(!window.get_asking());
                if yes {
                    kept.retain(|f| !selected.contains(f));
                }
            }
            // the rows renumbered at once; the names as the reference makes
            // them once the phrase is entered again (it shows the old names
            // until then)
            let shown = rows(&window);
            let theirs = strings(&step["rows"]);
            assert_eq!(shown.len(), theirs.len(), "{step}");
            for (ours, theirs) in shown.iter().zip(&theirs) {
                assert_eq!(ours[..2], theirs[..2], "{step}");
            }
            assert_eq!(shown, strings(&step["refreshed"]), "{step}");
        }
        window.invoke_dismissed();
        assert!(slots.window.borrow().is_none());
    }
}

// leaf: audit-network-export-paths
#[cfg(target_os = "linux")]
#[test]
fn the_destination_is_browsed_for_and_its_location_opened_as_the_reference_does() {
    use std::os::unix::fs::PermissionsExt;

    let (_dirs, store) = crate::subscriptions::store();
    let _windows = headless::init();
    let slots = Slots::default();
    let work = tempfile::tempdir().unwrap();
    let recorded = recorded(work.path());
    std::fs::create_dir_all(work.path().join("picked").join("inner")).unwrap();
    // the OS opener, a stub that writes down how it was run (as the
    // recorder's did)
    let bin = work.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    let stub = bin.join("xdg-open");
    let launched = work.path().join("launched.txt");
    std::fs::write(
        &stub,
        format!(
            "#!/bin/sh\nprintf \"%s\" \"$0\" >> \"{0}\"\nfor a in \"$@\"; do printf \"\\0%s\" \"$a\" >> \"{0}\"; done\necho >> \"{0}\"\n",
            launched.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
    hydrus_gui::set_launch_dir(&bin);

    let answers: Rc<RefCell<Vec<Option<std::path::PathBuf>>>> = Rc::default();
    let asked: Rc<RefCell<Vec<String>>> = Rc::default();
    hydrus_gui::set_picker({
        let answers = answers.clone();
        let asked = asked.clone();
        move |kind, title| {
            assert_eq!(kind, hydrus_gui::Pick::Folder);
            asked.borrow_mut().push(title.to_owned());
            answers.borrow_mut().remove(0).into_iter().collect()
        }
    });
    let files = [HashId(1), HashId(3)];
    let window = open(&store, &files, &slots);
    window.set_destination("/tmp/hx".into());
    window.set_phrase("{hash}".into());
    window.invoke_update();

    // browse: a folder (tidied), a cancel, and a folder that isn't there
    for browse in recorded["browses"].as_array().unwrap() {
        answers
            .borrow_mut()
            .push(browse["answer"].as_str().map(std::path::PathBuf::from));
        window.invoke_browse();
        assert_eq!(
            window.get_destination(),
            browse["field"].as_str().unwrap(),
            "{browse}"
        );
        assert_eq!(rows(&window), strings(&browse["rows"]), "{browse}");
    }
    let titles: Vec<&str> = recorded["browse_asked"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a[0].as_str().unwrap())
        .collect();
    assert_eq!(*asked.borrow(), titles);

    // open location: nothing for an empty one, the opener for one that
    // exists, and a missing one said so
    for opened in recorded["opened"].as_array().unwrap() {
        window.set_status("".into());
        window.set_destination(opened["destination"].as_str().unwrap().into());
        window.invoke_open_location();
        let critical = opened["criticals"].as_array().unwrap();
        if critical.is_empty() {
            assert_eq!(window.get_status(), "");
        } else {
            // (a status line here, where the reference shows a dialog
            // titled "Does not exist!")
            assert_eq!(window.get_status(), critical[0][1].as_str().unwrap());
        }
    }
    // (the stub runs on its own thread: wait for its whole line)
    let started = Instant::now();
    while !std::fs::read_to_string(&launched).is_ok_and(|text| text.ends_with('\n'))
        && started.elapsed() < Duration::from_secs(10)
    {
        std::thread::sleep(Duration::from_millis(20));
    }
    hydrus_gui::clear_launch_dir();
    let ours: Vec<Vec<String>> = std::fs::read_to_string(&launched)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.split('\0').map(str::to_owned).collect())
        .collect();
    assert_eq!(ours, strings(&recorded["launched"]));
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
