//! The import and export folder editors against their consumers: what is
//! entered in the edit dialogs, written with "apply" in the list, is what
//! the folders' workers then do (`ClientGUIImportFolders.EditImportFolderPanel`,
//! `ClientGUIExport.EditExportFolderPanel`), on temporary directories.

use std::sync::Arc;
use std::time::Duration;

use slint::{ComponentHandle as _, Model as _};

use hydrus_gui::{Bound, FoldersWindow, ImportFolderWindow, MainWindow, Pages, bind, headless};
use hydrus_parse::folders::FolderAction;
use hydrus_store::import::import_legacy;
use hydrus_store::{Store, import_folders};

fn fixture_store(name: &str) -> ([tempfile::TempDir; 2], Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture(name);
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    ([legacy, native], store)
}

fn downloader(store: &Arc<Store>) -> hydrus_download::Downloader {
    let net = Arc::new(
        hydrus_net::NetEngine::new(store.clone(), hydrus_net::NetOptions::default()).unwrap(),
    );
    let importer = hydrus_import::FileImporter::new(store.clone(), hydrus_media::MediaTools::new());
    hydrus_download::Downloader::new(store.clone(), net, importer).unwrap()
}

/// A copy of an `import_folder` fixture file, last modified `age` seconds ago.
fn place(dir: &std::path::Path, name: &str, age: u64) {
    let to = dir.join(name);
    std::fs::copy(
        hydrus_testkit::fixture_path(format!("import_folder/{name}")),
        &to,
    )
    .unwrap();
    let now = std::time::SystemTime::now();
    std::fs::File::options()
        .write(true)
        .open(&to)
        .unwrap()
        .set_modified(now - Duration::from_secs(age))
        .unwrap();
}

fn fields(model: &slint::ModelRc<hydrus_gui::DurationField>) -> Vec<i32> {
    (0..model.row_count())
        .map(|i| model.row_data(i).unwrap().value)
        .collect()
}

/// file > manage import folders…, the first folder's edit dialog.
fn edit_first(ui: &MainWindow, bound: &Bound) -> (FoldersWindow, ImportFolderWindow) {
    crate::folders::open(ui, "manage import folders\u{2026}");
    let list = crate::folders::import_list(bound);
    list.invoke_row_clicked(0, false, false);
    list.invoke_edit();
    let edit = bound
        .folders
        .import_edit
        .borrow()
        .as_ref()
        .expect("the edit dialog opens")
        .clone_strong();
    (list, edit)
}

fn saved(store: &Store) -> hydrus_store::import_folders::ImportFolder {
    store
        .read(|c| import_folders::find_import_folder(c, "drop box"))
        .unwrap()
        .unwrap()
}

// leaf: audit-network-import-folder-outcomes
#[test]
fn an_import_folders_schedule_skip_period_and_outcomes_are_what_its_worker_does() {
    let (_dirs, store) = fixture_store("import_folder");
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let watched = tempfile::tempdir().unwrap();
    let moved = tempfile::tempdir().unwrap();
    // one file an hour old; one long old
    place(watched.path(), "recent.png", 3600);
    place(watched.path(), "a.png", 1_000_000);

    let (list, edit) = edit_first(&ui, &bound);
    edit.set_path(watched.path().to_string_lossy().into_owned().into());
    // the period is at least three minutes (the reference's minimum)
    assert_eq!(fields(&edit.get_period()), [0, 1, 0]);
    edit.invoke_period_edited(1, 0);
    assert_eq!(fields(&edit.get_period()), [0, 0, 3]);
    edit.invoke_period_edited(1, 5);
    assert_eq!(fields(&edit.get_period()), [0, 5, 3]);
    // the recent-modified skip: a day
    assert_eq!(fields(&edit.get_skip()), [0, 0, 1, 0]);
    edit.invoke_skip_edited(0, 1);
    edit.invoke_skip_edited(2, 0);
    assert_eq!(fields(&edit.get_skip()), [1, 0, 0, 0]);
    edit.set_check_regularly(false);
    edit.set_paused(false);
    edit.set_check_now(true);
    edit.set_show_popup(true);
    edit.set_publish_popup_button(true);
    edit.set_publish_page(true);
    // success: moved (needs a place); the rest left alone
    edit.invoke_action_chosen(0, 2);
    edit.invoke_location_edited(0, moved.path().to_string_lossy().into_owned().into());
    for row in 1..4 {
        edit.invoke_action_chosen(row, 1);
    }
    edit.invoke_apply();
    assert!(
        bound.folders.import_edit.borrow().is_none(),
        "{}",
        edit.get_asking_message()
    );
    list.invoke_apply();
    let folder = saved(&store);
    let s = &folder.settings;
    assert_eq!(s.period, 5 * 3600 + 180);
    assert_eq!(s.last_modified_time_skip_period, 86_400);
    assert!(!s.check_regularly && s.check_now && !folder.paused());
    assert!(s.show_working_popup && s.publish_files_to_popup_button && s.publish_files_to_page);
    assert_eq!(
        s.actions.successful_and_new,
        FolderAction::Move(moved.path().to_string_lossy().into_owned())
    );
    assert_eq!(s.actions.successful_but_redundant, FolderAction::Ignore);

    // reopened: what was saved
    let (list, edit) = edit_first(&ui, &bound);
    assert_eq!(fields(&edit.get_period()), [0, 5, 3]);
    assert_eq!(fields(&edit.get_skip()), [1, 0, 0, 0]);
    assert!(edit.get_check_now() && !edit.get_check_regularly() && !edit.get_paused());
    assert!(edit.get_show_popup() && edit.get_publish_popup_button() && edit.get_publish_page());
    assert_eq!(edit.get_actions().row_data(0).unwrap().action, 2);
    assert_eq!(
        edit.get_actions().row_data(0).unwrap().location,
        moved.path().to_string_lossy().as_ref()
    );

    // cancelled: the worker sees what was applied
    edit.invoke_cancel();
    list.invoke_cancel();
    let worker = downloader(&store);
    let id = folder.id();
    let run = worker.work_on_import_folder(id).unwrap();
    assert!(run.checked, "check now makes it due: {run:?}");
    assert_eq!(run.error, None);
    assert_eq!(
        run.new_files, 1,
        "the day's skip leaves the recent file alone"
    );
    assert_eq!(run.imported, 1);
    assert!(!watched.path().join("a.png").exists(), "moved away");
    assert!(moved.path().join("a.png").is_file());
    assert!(watched.path().join("recent.png").exists());
    assert!(!saved(&store).settings.check_now, "check now is spent");

    // skip a second instead, and check again: the recent file is found
    let (list, edit) = edit_first(&ui, &bound);
    edit.invoke_skip_edited(0, 0);
    edit.invoke_skip_edited(3, 1);
    assert_eq!(fields(&edit.get_skip()), [0, 0, 0, 1]);
    edit.set_check_now(true);
    edit.invoke_action_chosen(0, 0);
    edit.invoke_apply();
    list.invoke_apply();
    assert_eq!(saved(&store).settings.last_modified_time_skip_period, 1);
    let run = worker.work_on_import_folder(id).unwrap();
    assert!(run.checked);
    assert_eq!(run.new_files, 1);
    assert_eq!(run.imported, 1);
    assert!(
        !watched.path().join("recent.png").exists(),
        "success: delete, as chosen"
    );
}

fn texts(model: &slint::ModelRc<slint::SharedString>) -> Vec<String> {
    (0..model.row_count())
        .map(|i| model.row_data(i).unwrap().to_string())
        .collect()
}

// leaf: audit-network-export-folder-predicates
#[test]
fn typed_predicates_make_the_export_folders_search_as_the_reference_does() {
    // oracle/record_export_folder_predicates.py: the reference's editor
    // taking typed predicates, and the folder it makes running
    let recorded: serde_json::Value = hydrus_testkit::fixture_json("export_folder_predicates.json");
    let (_dirs, store) = crate::subscriptions::store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());

    // typed in one at a time, as Enter enters them; rows removed
    crate::folders::open(&ui, "manage export folders\u{2026}");
    let list = export_list(&bound);
    list.invoke_add();
    let edit = export_edit(&bound);
    let steps = recorded["typing"].as_array().unwrap();
    let rows =
        |v: &serde_json::Value| -> Vec<String> { serde_json::from_value(v.clone()).unwrap() };
    assert_eq!(texts(&edit.get_predicates()), rows(&steps[0]["rows"]));
    for step in &steps[1..] {
        if let Some(typed) = step["typed"].as_str() {
            edit.set_typed(typed.into());
            edit.invoke_typed_accepted();
            assert_eq!(
                edit.get_typed(),
                step["left"].as_str().unwrap(),
                "{typed:?}"
            );
            // (the reference says nothing of a search it can't parse; here
            // the editor says why)
            assert_eq!(
                edit.get_error().is_empty(),
                step["left"].as_str().unwrap().is_empty(),
                "{typed:?}"
            );
        } else {
            edit.invoke_predicate_removed(step["removed"].as_i64().unwrap() as i32);
        }
        assert_eq!(texts(&edit.get_predicates()), rows(&step["rows"]), "{step}");
        // padded "system:" text is a tag in the reference (predicate type 0),
        // which reads "inbox" where the system predicate reads "system:inbox"
        if step["typed"] == "  system:inbox  " {
            let at = rows(&step["rows"])
                .iter()
                .position(|r| r == "inbox")
                .unwrap();
            assert_eq!(step["types"][at], 0, "a tag in the reference");
            assert!(!texts(&edit.get_predicates()).contains(&"system:inbox".to_owned()));
        }
    }
    edit.invoke_cancel();
    list.invoke_cancel();

    // a folder over the two predicates, run; then over one, run again
    let dest = tempfile::tempdir().unwrap();
    crate::folders::open(&ui, "manage export folders\u{2026}");
    let list = export_list(&bound);
    list.invoke_add();
    let edit = export_edit(&bound);
    edit.set_name("mine".into());
    edit.set_path(dest.path().to_string_lossy().into_owned().into());
    edit.set_phrase("{file_id}".into());
    edit.set_run_regularly(false);
    edit.set_run_now(true);
    for typed in ["system:filesize > 1B", "system:filetype is png"] {
        edit.set_typed(typed.into());
        edit.invoke_typed_accepted();
    }
    for (n, run) in recorded["runs"].as_array().unwrap().iter().enumerate() {
        if n > 0 {
            crate::folders::open(&ui, "manage export folders\u{2026}");
            let list = export_list(&bound);
            list.invoke_row_clicked(0, false, false);
            list.invoke_edit();
            let edit = export_edit(&bound);
            edit.invoke_predicate_removed(1);
            assert_eq!(texts(&edit.get_predicates()), rows(&run["rows"]));
            edit.set_run_now(true);
            edit.invoke_apply();
            list.invoke_apply();
        } else {
            assert_eq!(texts(&edit.get_predicates()), rows(&run["rows"]));
            edit.invoke_apply();
            list.invoke_apply();
        }
        let runs = crate::common::export_folders_until_done(&store);
        assert_eq!(runs.len(), 1, "{runs:?}");
        assert_eq!(runs[0].1.error, None);
        assert_eq!(
            exported(dest.path()),
            rows(&run["files"]),
            "{}",
            run["step"]
        );
    }
}

fn export_edit(bound: &Bound) -> hydrus_gui::ExportFolderWindow {
    bound
        .folders
        .export_edit
        .borrow()
        .as_ref()
        .expect("the export folder dialog opens")
        .clone_strong()
}

fn export_list(bound: &Bound) -> FoldersWindow {
    bound
        .folders
        .export_list
        .borrow()
        .as_ref()
        .expect("the export folders list opens")
        .clone_strong()
}

fn exported(dir: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn stored_export(store: &Store, name: &str) -> hydrus_parse::folders::ExportFolder {
    let folders: hydrus_store::settings::ExportFolders =
        store.read(hydrus_store::settings::get).unwrap();
    folders.0.into_iter().find(|f| f.name == name).unwrap()
}

#[test]
fn an_export_folders_search_phrase_schedule_and_overwrites_are_what_its_worker_does() {
    let (_dirs, store) = crate::subscriptions::store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let dest = tempfile::tempdir().unwrap();

    crate::folders::open(&ui, "manage export folders\u{2026}");
    let list = export_list(&bound);
    list.invoke_add();
    let edit = export_edit(&bound);
    edit.set_name("mine".into());
    edit.set_path(dest.path().to_string_lossy().into_owned().into());
    edit.set_phrase("{file_id}".into());
    // two typed predicates, anded: not tiny, and only pngs
    edit.set_typed("system:filesize > 1B".into());
    edit.invoke_typed_accepted();
    edit.set_typed("system:filetype is png".into());
    edit.invoke_typed_accepted();
    assert_eq!(edit.get_predicates().row_count(), 2, "{}", edit.get_error());
    // (a search that doesn't parse is refused and kept)
    edit.set_typed("system:nonsense words".into());
    edit.invoke_typed_accepted();
    assert!(!edit.get_error().is_empty());
    assert_eq!(edit.get_predicates().row_count(), 2);
    edit.set_typed("".into());
    // schedule: not regularly, popup, run now, every 3 hours
    edit.set_run_regularly(false);
    edit.set_show_popup(true);
    edit.set_run_now(true);
    edit.invoke_period_edited(1, 3);
    edit.invoke_period_edited(0, 0);
    // sidecar overwrite policy
    edit.set_overwrite_next(true);
    edit.set_overwrite_always(false);
    edit.invoke_apply();
    assert!(
        bound.folders.export_edit.borrow().is_none(),
        "{}",
        edit.get_asking_message()
    );
    list.invoke_apply();
    let folder = stored_export(&store, "mine");
    assert_eq!(folder.phrase, "{file_id}");
    assert_eq!(folder.search.predicates.len(), 2);
    assert!(!folder.run_regularly && folder.show_working_popup && folder.run_now);
    assert!(folder.overwrite_sidecars_on_next_run && !folder.always_overwrite_sidecars);
    assert_eq!(folder.period, 3 * 3600);

    // reopened: as saved
    crate::folders::open(&ui, "manage export folders\u{2026}");
    let list = export_list(&bound);
    list.invoke_row_clicked(0, false, false);
    list.invoke_edit();
    let edit = export_edit(&bound);
    assert_eq!(edit.get_name(), "mine");
    assert_eq!(edit.get_phrase(), "{file_id}");
    assert_eq!(edit.get_predicates().row_count(), 2);
    assert!(edit.get_run_now() && edit.get_show_popup() && !edit.get_run_regularly());
    assert!(edit.get_overwrite_next() && !edit.get_overwrite_always());
    edit.invoke_cancel();
    list.invoke_cancel();

    // the worker runs it: only pngs, named by file id; the run spends the
    // run-now flag and the one-time overwrite
    let runs = crate::common::export_folders_until_done(&store);
    assert_eq!(runs.len(), 1, "{runs:?}");
    assert_eq!(runs[0].1.error, None);
    let pngs = exported(dest.path());
    assert!(!pngs.is_empty());
    assert!(
        pngs.iter()
            .all(|n| n.rsplit_once('.').is_some_and(|(_, e)| e == "png")),
        "{pngs:?}"
    );
    assert!(
        pngs.iter()
            .all(|n| n.trim_end_matches(".png").parse::<u32>().is_ok()),
        "{pngs:?}"
    );
    let after = stored_export(&store, "mine");
    assert!(!after.run_now && !after.overwrite_sidecars_on_next_run);
    assert!(after.last_checked > 0);

    // the png predicate removed: every file
    crate::folders::open(&ui, "manage export folders\u{2026}");
    let list = export_list(&bound);
    list.invoke_row_clicked(0, false, false);
    list.invoke_edit();
    let edit = export_edit(&bound);
    edit.invoke_predicate_removed(1);
    assert_eq!(edit.get_predicates().row_count(), 1);
    edit.set_run_now(true);
    edit.invoke_apply();
    list.invoke_apply();
    assert_eq!(stored_export(&store, "mine").search.predicates.len(), 1);
    let runs = crate::common::export_folders_until_done(&store);
    assert_eq!(runs.len(), 1);
    let everything = exported(dest.path());
    assert!(everything.len() > pngs.len(), "{everything:?} vs {pngs:?}");
}

// leaf: audit-network-export-folder-type
#[cfg(unix)]
#[test]
fn an_export_folders_type_symlinks_and_trash_choices_reach_its_worker() {
    let (_dirs, store) = crate::subscriptions::store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let dest = tempfile::tempdir().unwrap();

    crate::folders::open(&ui, "manage export folders\u{2026}");
    let list = export_list(&bound);
    list.invoke_add();
    let edit = export_edit(&bound);
    edit.set_name("typed".into());
    edit.set_path(dest.path().to_string_lossy().into_owned().into());
    edit.set_phrase("{file_id}".into());
    edit.set_typed("system:filetype is png".into());
    edit.invoke_typed_accepted();
    edit.set_run_regularly(false);
    edit.set_run_now(true);
    // symlinks, and synchronise: with a stray file in the folder
    edit.set_symlinks(true);
    edit.invoke_type_chosen(1);
    assert!(
        !edit.get_delete_from_client(),
        "synchronising never trashes"
    );
    std::fs::write(dest.path().join("stray.txt"), b"not exported").unwrap();
    edit.invoke_apply();
    assert!(
        bound.folders.export_edit.borrow().is_none(),
        "{}",
        edit.get_asking_message()
    );
    list.invoke_apply();
    let folder = stored_export(&store, "typed");
    assert_eq!(
        folder.export_type,
        hydrus_parse::folders::ExportType::Synchronise
    );
    assert!(folder.export_symlinks && !folder.delete_from_client_after_export);

    let runs = crate::common::export_folders_until_done(&store);
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].1.error, None);
    let names = exported(dest.path());
    assert!(
        !names.contains(&"stray.txt".to_owned()),
        "synchronised away: {names:?}"
    );
    assert!(!names.is_empty());
    for name in &names {
        let meta = std::fs::symlink_metadata(dest.path().join(name)).unwrap();
        assert!(meta.file_type().is_symlink(), "{name}");
    }

    // back to a regular export that deletes from the client: the warning
    // comes when it is ticked and again on apply, and "no" keeps the dialog
    crate::folders::open(&ui, "manage export folders\u{2026}");
    let list = export_list(&bound);
    list.invoke_row_clicked(0, false, false);
    list.invoke_edit();
    let edit = export_edit(&bound);
    edit.invoke_type_chosen(0);
    edit.set_delete_from_client(true);
    edit.invoke_delete_toggled();
    assert!(
        edit.get_asking_message()
            .starts_with("This will delete the exported files")
    );
    edit.invoke_chosen(0);
    edit.invoke_apply();
    assert!(
        edit.get_asking_message()
            .starts_with("You have set this export folder to delete")
    );
    edit.invoke_chosen(1);
    assert!(bound.folders.export_edit.borrow().is_some());
    edit.invoke_chosen(0);
    edit.invoke_cancel();
    list.invoke_cancel();
    assert!(
        !stored_export(&store, "typed").delete_from_client_after_export,
        "cancelled"
    );
}

#[test]
fn an_import_folders_allowed_filetypes_decide_which_files_its_worker_imports() {
    let (_dirs, store) = fixture_store("import_folder");
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let watched = tempfile::tempdir().unwrap();
    place(watched.path(), "a.png", 1_000_000);
    place(watched.path(), "b p12.jpg", 1_000_000);

    let (list, edit) = edit_first(&ui, &bound);
    edit.set_path(watched.path().to_string_lossy().into_owned().into());
    edit.set_check_regularly(false);
    edit.set_paused(false);
    edit.set_check_now(true);
    // leave the source files where they are
    for row in 0..4 {
        edit.invoke_action_chosen(row, 1);
    }
    // the file filtering import options: custom, with png unticked
    edit.invoke_edit_import_options();
    let editor = bound
        .folders
        .import_options
        .borrow()
        .as_ref()
        .expect("it opens")
        .clone_strong();
    editor.invoke_kind_clicked(0);
    editor.set_custom_index(1);
    editor.invoke_changed();
    editor.invoke_filetype_expanded(0, true);
    let rows = editor.get_filetype_rows();
    let png = (0..rows.row_count())
        .map(|i| rows.row_data(i).unwrap())
        .find(|r| r.text == "png" && r.option >= 0)
        .expect("png is among the image group's filetypes");
    assert!(png.ticked);
    editor.invoke_filetype_ticked(png.group, png.option, false);
    let rows = editor.get_filetype_rows();
    let after = (0..rows.row_count())
        .map(|i| rows.row_data(i).unwrap())
        .filter(|r| r.option >= 0)
        .map(|r| (r.text.to_string(), r.ticked))
        .collect::<Vec<_>>();
    assert!(after.contains(&("png".to_owned(), false)));
    assert!(after.contains(&("jpeg".to_owned(), true)));
    editor.invoke_apply();
    edit.invoke_apply();
    assert!(
        bound.folders.import_edit.borrow().is_none(),
        "{}",
        edit.get_asking_message()
    );
    list.invoke_apply();

    // the worker imports the jpeg and leaves the png out
    let worker = downloader(&store);
    let run = worker.work_on_import_folder(saved(&store).id()).unwrap();
    assert_eq!(run.error, None);
    assert!(run.checked);
    assert_eq!(run.new_files, 2, "both are found by the check");
    assert_eq!(run.imported, 1, "only the allowed filetype comes in");
}

/// Keep every popup the work sends, even those it dismisses at its end.
fn log_popups(store: &Store) {
    store
        .write(|ctx| {
            ctx.conn().execute_batch(
                "CREATE TABLE IF NOT EXISTS popup_log (seq INTEGER PRIMARY KEY, job TEXT NOT NULL);
                 CREATE TRIGGER IF NOT EXISTS popup_logger AFTER INSERT ON popups
                 BEGIN INSERT INTO popup_log (job) VALUES (new.job); END;",
            )?;
            Ok(())
        })
        .unwrap();
}

/// The popups sent since the last call, as `(title, carries files)`.
fn popups_since(store: &Store) -> Vec<(String, bool)> {
    store
        .write(|ctx| {
            let jobs: Vec<String> = ctx
                .conn()
                .prepare("SELECT job FROM popup_log ORDER BY seq")?
                .query_map([], |r| r.get(0))?
                .collect::<Result<_, _>>()?;
            ctx.conn().execute("DELETE FROM popup_log", [])?;
            Ok(jobs
                .iter()
                .map(|job| {
                    let job: serde_json::Value = serde_json::from_str(job).unwrap();
                    (
                        job["status_title"].as_str().unwrap_or_default().to_owned(),
                        !job["files"].is_null(),
                    )
                })
                .collect())
        })
        .unwrap()
}

fn recorded_popups(case: &serde_json::Value) -> Vec<(String, bool)> {
    case["popups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["title"].as_str().unwrap_or_default().to_owned(),
                p["has_files"].as_bool().unwrap(),
            )
        })
        .collect()
}

// leaf: audit-network-export-folder-schedule
#[test]
fn an_export_folder_works_and_shows_popups_when_its_edited_schedule_says_as_the_reference_does() {
    let recorded = hydrus_testkit::fixture_json("folder_schedule.json");
    let (_dirs, store) = crate::subscriptions::store();
    log_popups(&store);
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let now = || hydrus_core::time::TimestampMs::now().millis() / 1000;
    for (n, case) in recorded["export"].as_array().unwrap().iter().enumerate() {
        let name = case["name"].as_str().unwrap();
        let options = &case["options"];
        let dest = tempfile::tempdir().unwrap();
        crate::folders::open(&ui, "manage export folders\u{2026}");
        let list = export_list(&bound);
        if n == 0 {
            list.invoke_add();
        } else {
            list.invoke_row_clicked(0, false, false);
            list.invoke_edit();
        }
        let edit = export_edit(&bound);
        if n == 0 {
            // (named as the recorder's)
            edit.set_name("schedule".into());
            edit.set_phrase("{hash}".into());
            edit.set_typed("system:filetype is png".into());
            edit.invoke_typed_accepted();
        }
        edit.set_path(dest.path().to_string_lossy().into_owned().into());
        edit.set_run_regularly(options["run_regularly"].as_bool().unwrap_or(true));
        edit.set_show_popup(options["show_working_popup"].as_bool().unwrap_or(false));
        edit.set_run_now(options["run_now"].as_bool().unwrap_or(false));
        // the recorded period: an hour
        edit.invoke_period_edited(1, 1);
        edit.invoke_period_edited(0, 0);
        edit.invoke_apply();
        assert!(
            bound.folders.export_edit.borrow().is_none(),
            "{name}: {}",
            edit.get_asking_message()
        );
        list.invoke_apply();
        assert_eq!(
            stored_export(&store, "schedule").period,
            recorded["period"].as_i64().unwrap()
        );
        // it was last checked this long ago
        let before = now() - case["age"].as_i64().unwrap();
        store
            .write(move |ctx| {
                let mut folders: hydrus_store::settings::ExportFolders =
                    hydrus_store::settings::get(ctx.conn())?;
                folders.0[0].last_checked = before;
                hydrus_store::settings::set(ctx.conn(), &folders)
            })
            .unwrap();
        popups_since(&store);
        let run = hydrus_download::export::work_on_export_folder(&store, "schedule").unwrap();
        assert_eq!(run.error, None, "{name}");
        assert_eq!(
            !exported(dest.path()).is_empty(),
            case["worked"].as_bool().unwrap(),
            "{name}: files written"
        );
        assert_eq!(
            popups_since(&store),
            recorded_popups(case),
            "{name}: popups"
        );
        let after = stored_export(&store, "schedule");
        assert_eq!(
            after.run_now,
            case["run_now_after"].as_bool().unwrap(),
            "{name}"
        );
        assert_eq!(
            after.run_regularly,
            case["run_regularly_after"].as_bool().unwrap(),
            "{name}"
        );
        assert_eq!(
            after.last_checked != before,
            case["last_checked_moved"].as_bool().unwrap(),
            "{name}"
        );
    }
}

// leaf: audit-network-import-folder-schedule
#[test]
fn an_import_folder_checks_and_shows_popups_when_its_edited_schedule_says_as_the_reference_does() {
    let recorded = hydrus_testkit::fixture_json("folder_schedule.json");
    let (_dirs, store) = fixture_store("import_folder");
    log_popups(&store);
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let worker = downloader(&store);
    let now = || hydrus_core::time::TimestampMs::now().millis() / 1000;
    // a file the client has not seen for each case, as the recorder had
    let images = [
        "p01_e.jpg",
        "p03_a.png",
        "p04_a.png",
        "p00_c.bmp",
        "p00_f_exif.jpg",
        "p00_g_lossy.jpg",
        "p00_b_bloated.png",
        "p00_h.gif",
    ];
    for (n, case) in recorded["import"].as_array().unwrap().iter().enumerate() {
        let name = case["name"].as_str().unwrap();
        let options = &case["options"];
        let watched = tempfile::tempdir().unwrap();
        let target = watched.path().join("a.png");
        std::fs::copy(
            hydrus_testkit::fixture_path(format!("auto_resolution/{}", images[n])),
            &target,
        )
        .unwrap();
        std::fs::File::options()
            .write(true)
            .open(&target)
            .unwrap()
            .set_modified(std::time::SystemTime::now() - Duration::from_secs(1_000_000))
            .unwrap();
        let (list, edit) = edit_first(&ui, &bound);
        edit.set_path(watched.path().to_string_lossy().into_owned().into());
        edit.set_check_regularly(options["check_regularly"].as_bool().unwrap_or(true));
        edit.set_paused(options["paused"].as_bool().unwrap_or(false));
        edit.set_check_now(options["check_now"].as_bool().unwrap_or(false));
        edit.set_show_popup(options["show_working_popup"].as_bool().unwrap_or(true));
        edit.set_publish_popup_button(
            options["publish_files_to_popup_button"]
                .as_bool()
                .unwrap_or(true),
        );
        edit.set_publish_page(false);
        // the recorded period: an hour
        edit.invoke_period_edited(1, 1);
        edit.invoke_period_edited(2, 0);
        // leave the source files where they are
        for row in 0..4 {
            edit.invoke_action_chosen(row, 1);
        }
        edit.invoke_apply();
        assert!(
            bound.folders.import_edit.borrow().is_none(),
            "{name}: {}",
            edit.get_asking_message()
        );
        list.invoke_apply();
        let folder = saved(&store);
        assert_eq!(folder.settings.period, recorded["period"].as_i64().unwrap());
        let id = folder.id();
        let before = now() - case["age"].as_i64().unwrap();
        store
            .write(move |ctx| {
                let mut settings = import_folders::import_folder(ctx.conn(), id)?
                    .unwrap()
                    .settings;
                settings.last_checked = before;
                import_folders::set_settings(ctx.conn(), id, &settings)
            })
            .unwrap();
        popups_since(&store);
        let run = worker.work_on_import_folder(id).unwrap();
        assert_eq!(run.error, None, "{name}");
        assert_eq!(
            run.checked,
            case["worked"].as_bool().unwrap(),
            "{name}: checked"
        );
        // (the work's own popup is titled by the folder: "drop box" here)
        let ours: Vec<(String, bool)> = popups_since(&store)
            .into_iter()
            .map(|(title, files)| (title.replace("drop box", "schedule"), files))
            .collect();
        assert_eq!(ours, recorded_popups(case), "{name}: popups");
        let after = saved(&store);
        assert_eq!(
            after.paused(),
            case["paused_after"].as_bool().unwrap(),
            "{name}"
        );
        assert_eq!(
            after.settings.check_now,
            case["check_now_after"].as_bool().unwrap(),
            "{name}"
        );
        assert_eq!(
            after.settings.last_checked != before,
            case["last_checked_moved"].as_bool().unwrap(),
            "{name}"
        );
    }
}

// leaf: audit-network-import-folder-filetypes
#[test]
fn an_import_folders_allowed_filetypes_veto_the_other_files_as_the_reference_does() {
    let recorded = hydrus_testkit::fixture_json("import_folder_filetypes.json");
    let sources = [
        ["p03_a.png", "p01_e.jpg", "p00_h.gif", "p00_c.bmp"],
        ["p04_a.png", "p02_e.jpg", "p03_h.gif", "p02_c.bmp"],
        ["p05_a.png", "p03_e.jpg", "p04_h.gif", "p04_c.bmp"],
        ["p06_a.png", "p04_e.jpg", "p05_h.gif", "p05_c.bmp"],
    ];
    let names = ["a.png", "b.jpg", "c.gif", "d.bmp"];
    let (_dirs, store) = fixture_store("import_folder");
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let worker = downloader(&store);
    // (the last folder stays for the edit after)
    let mut last_watched = None;
    for (n, case) in recorded["cases"].as_array().unwrap().iter().enumerate() {
        let name = case["name"].as_str().unwrap();
        let watched = tempfile::tempdir().unwrap();
        for (target, source) in names.iter().zip(sources[n]) {
            let path = watched.path().join(target);
            std::fs::copy(
                hydrus_testkit::fixture_path(format!("auto_resolution/{source}")),
                &path,
            )
            .unwrap();
            std::fs::File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_modified(std::time::SystemTime::now() - Duration::from_secs(1_000_000))
                .unwrap();
        }
        let (list, edit) = edit_first(&ui, &bound);
        edit.set_path(watched.path().to_string_lossy().into_owned().into());
        edit.set_check_regularly(false);
        edit.set_paused(false);
        edit.set_check_now(true);
        for row in 0..4 {
            edit.invoke_action_chosen(row, 1);
        }
        if let Some(allowed) = case["allowed"].as_array() {
            edit.invoke_edit_import_options();
            let editor = bound
                .folders
                .import_options
                .borrow()
                .as_ref()
                .expect("it opens")
                .clone_strong();
            editor.invoke_kind_clicked(0);
            editor.set_custom_index(1);
            editor.invoke_changed();
            // the options by the names the editor gives them
            let wanted: Vec<&str> = allowed
                .iter()
                .map(|t| match t.as_str().unwrap() {
                    "IMAGE_PNG" => "png",
                    "IMAGE_JPEG" => "jpeg",
                    "IMAGE_GIF" => "static gif",
                    "IMAGE_BMP" => "bitmap",
                    other => panic!("{other}"),
                })
                .collect();
            let groups = editor.get_filetype_rows().row_count();
            for g in 0..groups {
                editor.invoke_filetype_expanded(i32::try_from(g).unwrap(), true);
            }
            let rows = editor.get_filetype_rows();
            let options: Vec<_> = rows.iter().filter(|r| r.option >= 0).collect();
            for r in &options {
                let want = wanted.contains(&r.text.as_str());
                if r.ticked != want {
                    editor.invoke_filetype_ticked(r.group, r.option, want);
                }
            }
            let mut ticked: Vec<String> = editor
                .get_filetype_rows()
                .iter()
                .filter(|r| r.option >= 0 && r.ticked)
                .map(|r| r.text.to_string())
                .collect();
            ticked.sort();
            let mut wanted_sorted = wanted.clone();
            wanted_sorted.sort_unstable();
            assert_eq!(ticked, wanted_sorted, "{name}: the editor's ticks");
            editor.invoke_apply();
        }
        edit.invoke_apply();
        assert!(
            bound.folders.import_edit.borrow().is_none(),
            "{name}: {}",
            edit.get_asking_message()
        );
        list.invoke_apply();
        let folder = saved(&store);
        let run = worker.work_on_import_folder(folder.id()).unwrap();
        assert_eq!(run.error, None, "{name}");
        let id = folder.id();
        let mut seeds: Vec<(String, i64, String)> = store
            .read(move |conn| hydrus_store::queues::file_seeds(conn, id))
            .unwrap()
            .into_iter()
            .map(|s| {
                (
                    std::path::Path::new(&s.data)
                        .strip_prefix(watched.path())
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    s.status.code(),
                    s.note,
                )
            })
            .collect();
        seeds.sort();
        let theirs: Vec<(String, i64, String)> = case["seeds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| {
                (
                    s[0].as_str().unwrap().to_owned(),
                    s[1].as_i64().unwrap(),
                    s[2].as_str().unwrap().to_owned(),
                )
            })
            .collect();
        assert_eq!(seeds, theirs, "{name}");
        last_watched = Some(watched);
    }
    let _watched = last_watched;
    // the last folder (only bmp allowed) edited to allow every filetype: the
    // ignored files are forgotten at once, and found again by the next check
    let (list, edit) = edit_first(&ui, &bound);
    edit.set_check_now(true);
    edit.invoke_edit_import_options();
    let editor = bound
        .folders
        .import_options
        .borrow()
        .as_ref()
        .expect("it opens")
        .clone_strong();
    for g in 0..editor.get_filetype_rows().row_count() {
        editor.invoke_filetype_expanded(i32::try_from(g).unwrap(), true);
    }
    for r in editor
        .get_filetype_rows()
        .iter()
        .filter(|r| r.option >= 0 && !r.ticked)
    {
        editor.invoke_filetype_ticked(r.group, r.option, true);
    }
    let unticked = editor
        .get_filetype_rows()
        .iter()
        .filter(|r| r.option >= 0 && !r.ticked)
        .count();
    assert_eq!(unticked, 0, "every filetype is allowed in the editor");
    editor.invoke_apply();
    edit.invoke_apply();
    list.invoke_apply();
    let folder = saved(&store);
    let id = folder.id();
    let seeds_of = |store: &Store| -> Vec<(String, i64, String)> {
        let mut seeds: Vec<(String, i64, String)> = store
            .read(move |conn| hydrus_store::queues::file_seeds(conn, id))
            .unwrap()
            .into_iter()
            .map(|s| {
                (
                    std::path::Path::new(&s.data)
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    s.status.code(),
                    s.note,
                )
            })
            .collect();
        seeds.sort();
        seeds
    };
    let after = &recorded["after_edit"];
    assert_eq!(
        seeds_of(&store).len(),
        after["seeds_forgotten"].as_u64().unwrap() as usize
    );
    let run = worker.work_on_import_folder(id).unwrap();
    assert_eq!(run.error, None);
    let theirs: Vec<(String, i64, String)> = after["seeds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s[0].as_str().unwrap().to_owned(),
                s[1].as_i64().unwrap(),
                s[2].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(seeds_of(&store), theirs);

    // the skip period: files modified more recently than it wait
    let skip = &recorded["skip"];
    let watched = tempfile::tempdir().unwrap();
    for (target, source) in [("new.png", "p06_b_bloated.png"), ("old.jpg", "p05_e.jpg")] {
        let path = watched.path().join(target);
        std::fs::copy(
            hydrus_testkit::fixture_path(format!("auto_resolution/{source}")),
            &path,
        )
        .unwrap();
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(
                std::time::SystemTime::now()
                    - Duration::from_secs(skip["ages"][target].as_u64().unwrap()),
            )
            .unwrap();
    }
    for (n, check) in skip["checks"].as_array().unwrap().iter().enumerate() {
        let (list, edit) = edit_first(&ui, &bound);
        edit.set_path(watched.path().to_string_lossy().into_owned().into());
        edit.set_check_regularly(false);
        edit.set_paused(false);
        edit.set_check_now(true);
        for row in 0..4 {
            edit.invoke_action_chosen(row, 1);
        }
        // a day, then a second
        if n == 0 {
            edit.invoke_skip_edited(0, 1);
            edit.invoke_skip_edited(2, 0);
        } else {
            edit.invoke_skip_edited(0, 0);
            edit.invoke_skip_edited(3, 1);
        }
        edit.invoke_apply();
        list.invoke_apply();
        let folder = saved(&store);
        assert_eq!(
            folder.settings.last_modified_time_skip_period,
            check["period"].as_i64().unwrap()
        );
        let id = folder.id();
        let run = worker.work_on_import_folder(id).unwrap();
        assert_eq!(run.error, None);
        let mut seeds: Vec<(String, i64)> = store
            .read(move |conn| hydrus_store::queues::file_seeds(conn, id))
            .unwrap()
            .into_iter()
            .map(|s| {
                (
                    std::path::Path::new(&s.data)
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    s.status.code(),
                )
            })
            .collect();
        seeds.sort();
        let theirs: Vec<(String, i64)> = check["seeds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| (s[0].as_str().unwrap().to_owned(), s[1].as_i64().unwrap()))
            .collect();
        assert_eq!(seeds, theirs, "skip period {}", check["period"]);
    }
}

// leaf: audit-network-export-folder-filename
#[test]
fn an_export_folders_sidecar_overwrite_options_decide_what_its_worker_writes_as_the_reference_did()
{
    let recorded = hydrus_testkit::fixture_json("export_folder_overwrite.json");
    let (_dirs, store) = fixture_store("export_folder");
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let names: Vec<String> = recorded["names"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_str().unwrap().to_owned())
        .collect();
    let sidecars = |dir: &std::path::Path| -> std::collections::BTreeMap<String, String> {
        exported(dir)
            .into_iter()
            .filter(|n| {
                std::path::Path::new(n).extension().is_some_and(|e| {
                    e.eq_ignore_ascii_case("txt") || e.eq_ignore_ascii_case("json")
                })
            })
            .map(|n| {
                let text = std::fs::read_to_string(dir.join(&n)).unwrap();
                (n, text)
            })
            .collect()
    };
    for case in recorded["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let dest = tempfile::tempdir().unwrap();
        crate::folders::open(&ui, "manage export folders\u{2026}");
        let list = export_list(&bound);
        let row = list
            .get_rows()
            .iter()
            .position(|r| r.cells.row_data(0).unwrap() == "regular")
            .expect("the recorded regular folder");
        list.invoke_row_clicked(i32::try_from(row).unwrap(), false, false);
        list.invoke_edit();
        let edit = export_edit(&bound);
        edit.set_path(dest.path().to_string_lossy().into_owned().into());
        // the files named by their hashes
        edit.set_phrase("{hash}".into());
        edit.set_run_regularly(false);
        edit.set_run_now(true);
        edit.set_overwrite_next(case["next_run"].as_bool().unwrap());
        edit.set_overwrite_always(case["always"].as_bool().unwrap());
        edit.invoke_apply();
        assert!(
            bound.folders.export_edit.borrow().is_none(),
            "{name}: {}",
            edit.get_asking_message()
        );
        list.invoke_apply();
        for (n, run) in case["runs"].as_array().unwrap().iter().enumerate() {
            for file in &names {
                let sidecar = dest.path().join(format!("{file}.txt"));
                if case["stale"].as_bool().unwrap() {
                    std::fs::write(&sidecar, recorded["stale"].as_str().unwrap()).unwrap();
                } else {
                    let _ = std::fs::remove_file(&sidecar);
                }
            }
            if n > 0 {
                // (run again: the next-run option was spent by the first)
                store
                    .write(|ctx| {
                        let mut folders: hydrus_store::settings::ExportFolders =
                            hydrus_store::settings::get(ctx.conn())?;
                        for f in &mut folders.0 {
                            f.run_now = f.name == "regular";
                        }
                        hydrus_store::settings::set(ctx.conn(), &folders)
                    })
                    .unwrap();
            }
            let result = hydrus_download::export::work_on_export_folder(&store, "regular").unwrap();
            assert_eq!(result.error, None, "{name}, run {n}");
            let theirs: std::collections::BTreeMap<String, String> = run["sidecars"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_owned()))
                .collect();
            assert_eq!(sidecars(dest.path()), theirs, "{name}, run {n}: sidecars");
            let after = stored_export(&store, "regular");
            assert_eq!(
                after.overwrite_sidecars_on_next_run,
                run["overwrite_next_after"].as_bool().unwrap(),
                "{name}, run {n}"
            );
            assert_eq!(
                after.always_overwrite_sidecars,
                run["always_after"].as_bool().unwrap(),
                "{name}, run {n}"
            );
        }
    }
}
