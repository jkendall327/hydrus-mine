//! Flow (`oracle/record_flow_manual_import.py`): a folder given to file >
//! import files (two images, a copy of the first, a file hydrus does not
//! import, and a subfolder with an image); "delete original files after
//! successful import" ticked; the import options set to archive every
//! import; then "add tags/urls with the import >>" with filename tagging on
//! "my tags" (a tag for all, the filename as a `title` tag, a tag for the
//! second file alone), applied. The import page the daemon then works
//! through must leave what the reference left: the file log, the files'
//! tags and inbox, and the folder's contents.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Value as Json, json};
use slint::{ComponentHandle as _, Model as _};

use hydrus_gui::{MainWindow, Pages, ReviewImportsWindow, bind, headless};
use hydrus_store::queues;

use super::{Daemon, Known, Normaliser, State, compare, hashes_in_log};

/// What this flow shows that the reference does not, each explained in
/// `docs/rust/DIFFERENCES.md`.
const KNOWN: &[Known] = &[];

/// The recording's files, placed under `work` as old files.
fn place(work: &Path, files: &Json) {
    for file in files.as_array().unwrap() {
        let target = work.join(file[0].as_str().unwrap());
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        match file[1].as_str() {
            Some(media) => {
                std::fs::copy(
                    hydrus_testkit::fixture_path(format!("media/{media}")),
                    &target,
                )
                .unwrap();
            }
            None => std::fs::write(&target, "# not a picture\n").unwrap(),
        }
        let settled = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_600_000_000);
        std::fs::File::options()
            .write(true)
            .open(&target)
            .unwrap()
            .set_modified(settled)
            .unwrap();
    }
}

/// Every file under `dir`: its path and size.
fn listing(dir: &Path) -> Json {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(here) = stack.pop() {
        for entry in std::fs::read_dir(&here).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let size = std::fs::metadata(&path).unwrap().len();
                out.insert(
                    path.strip_prefix(dir)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    size,
                );
            }
        }
    }
    json!(out)
}

fn table(rows: &slint::ModelRc<hydrus_gui::TableRow>, n: &Normaliser) -> Json {
    Json::Array(
        (0..rows.row_count())
            .map(|r| {
                let cells = rows.row_data(r).unwrap().cells;
                Json::Array(
                    (0..cells.row_count())
                        .map(|c| json!(n.text(&cells.row_data(c).unwrap())))
                        .collect(),
                )
            })
            .collect(),
    )
}

fn strings(model: &slint::ModelRc<slint::SharedString>) -> Vec<String> {
    (0..model.row_count())
        .map(|i| model.row_data(i).unwrap().to_string())
        .collect()
}

/// A title without the application's name, which Qt adds to the window,
/// not to its title.
fn bare(title: &str) -> String {
    title
        .strip_suffix(hydrus_gui::app_title::current().as_str())
        .unwrap_or(title)
        .to_owned()
}

#[test]
fn files_imported_by_hand_with_tags_options_and_deletion_as_the_reference_does() {
    let recording = hydrus_testkit::fixture_json("flow_manual_import.json");
    let (_dirs, store) = super::basic_store();
    let work_dir = tempfile::tempdir().unwrap();
    let work = work_dir.path().canonicalize().unwrap();
    place(&work, &recording["files"]);
    let n = Normaliser::local().place(&work, "WORK").check_delta();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    // (popups and the pages' thumbnails need a window that is shown)
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let daemon = Daemon::start(&store);
    let state = State {
        store: &store,
        ui: &ui,
        site: None,
        n: &n,
    };
    let review = || -> ReviewImportsWindow {
        bound
            .review_imports
            .borrow()
            .as_ref()
            .map(|(w, _)| w.clone_strong())
            .expect("the review files to import window")
    };
    let mut steps = Vec::new();

    // 1. review: the folder given (as dropped on the window), parsed;
    // "delete original files after successful import" ticked
    crate::files_io_file_menu::file_menu(&ui, &["import files\u{2026}"]);
    review().invoke_path_entered(work.to_string_lossy().into_owned().into());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while review().get_working() || review().get_rows().row_count() == 0 {
        assert!(std::time::Instant::now() < deadline, "the folder parsed");
        slint::platform::update_timers_and_animations();
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    review().set_delete_after_success(true);
    review().invoke_delete_toggled(true);
    let window = review();
    steps.push(json!({
        "step": "review",
        "dialog": {
            "title": bare(&window.get_composed_title()),
            "rows": table(&window.get_rows(), &n),
            "progress": n.text(&window.get_progress()),
            "delete": window.get_delete_after_success(),
            "delete warning": window.get_delete_warning().as_str(),
            "import options": window.get_import_options_label().as_str(),
        },
    }));

    // 2. options: locations made custom, "archive all imports", apply
    window.invoke_import_options();
    let editor = bound
        .review_import_options
        .borrow()
        .as_ref()
        .expect("the import options editor")
        .clone_strong();
    let before = strings(&editor.get_labels());
    let mut found = false;
    for i in 0..editor.get_labels().row_count() {
        editor.invoke_kind_clicked(i32::try_from(i).unwrap());
        if editor.get_kind() == "locations" {
            editor.set_custom_index(1);
            editor.invoke_changed();
            editor.set_archive(true);
            editor.invoke_changed();
            found = true;
        }
    }
    assert!(found, "a locations page");
    let after = strings(&editor.get_labels());
    let title = bare(&editor.get_composed_title());
    editor.invoke_apply();
    assert!(bound.review_import_options.borrow().is_none());
    steps.push(json!({
        "step": "options",
        "dialog": {
            "title": title,
            "before": before,
            "after": after,
            "button": window.get_import_options_label().as_str(),
        },
    }));

    // 3. import: filename tagging on my tags, apply; the import page made
    // and worked through
    window.invoke_add_tags();
    let tagging = bound
        .filename_tagging
        .borrow()
        .as_ref()
        .expect("the filename tagging dialog")
        .clone_strong();
    let services = strings(&tagging.get_services());
    let mine = services.iter().position(|s| s == "my tags").unwrap();
    tagging.invoke_service_chosen(i32::try_from(mine).unwrap());
    tagging.set_tags_all("imported".into());
    tagging.invoke_changed();
    tagging.invoke_misc_toggled(0, true);
    tagging.invoke_misc_namespace(0, "title".into());
    let second = tagging
        .get_rows()
        .iter()
        .position(|r| r.cells.row_data(1).unwrap().ends_with("/b_second.png"))
        .unwrap();
    tagging.invoke_row_clicked(i32::try_from(second).unwrap(), false, false);
    tagging.set_tags_selected("only second".into());
    tagging.invoke_changed();
    let seen = json!({
        "title": bare(&tagging.get_composed_title()),
        "tabs": std::iter::once(tagging.get_sidecars_tab_label().to_string())
            .chain(services)
            .collect::<Vec<_>>(),
        "rows": table(&tagging.get_rows(), &n),
    });
    tagging.invoke_apply();
    assert!(bound.review_imports.borrow().is_none());
    let queue = bound
        .current
        .borrow()
        .borrow()
        .importer()
        .map(|i| i.queue)
        .expect("the import page");
    let page_done = || {
        (bound.sync)();
        let files = store.read(|c| queues::file_seeds(c, queue)).unwrap();
        !files.is_empty()
            && files
                .iter()
                .all(|f| f.status != queues::SeedStatus::Unknown)
            && bound.current.borrow().borrow().files().len()
                == hashes_in_log(&state.file_log(queue)).len()
    };
    super::wait_until_idle(&daemon, page_done);
    let files = state.file_log(queue);
    steps.push(json!({
        "step": "import",
        "dialog": seen,
        "page": {
            "name": bound.pages.borrow().shown().name.clone(),
            "import options": ui.get_import_options_label().as_str(),
            "files": files,
            "thumbnails": bound.current.borrow().borrow().files().len(),
        },
        "files": state.files(hashes_in_log(&files)),
        "left": listing(&work),
        "review open": bound.review_imports.borrow().is_some(),
        "popups": state.popups(),
    }));

    compare(&steps, &recording, KNOWN);
}
