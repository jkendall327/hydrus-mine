//! The File menu's exit, restart, exit/force maintenance, open-directory and
//! import/export-folder entries (`FrameGUI._InitialiseMenuInfoFile`),
//! driven through the real menu bar on real stores, with the folder
//! workers the entries reach run afterwards on temporary directories.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use slint::{ComponentHandle as _, Model as _};

use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
use hydrus_store::import::import_legacy;
use hydrus_store::settings::{self, ExportFolders, FolderSettings, GuiSettings, ShutdownWork};
use hydrus_store::{Store, import_folders};

use crate::common::widgets;

/// The store migrated from a legacy fixture.
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

fn lines(ui: &MainWindow, pane: usize) -> Vec<(String, bool, bool)> {
    let panes = ui.get_menu_panes();
    let Some(pane) = panes.row_data(pane) else {
        return Vec::new();
    };
    (0..pane.lines.row_count())
        .map(|i| {
            let line = pane.lines.row_data(i).unwrap();
            let label = if line.kind == 2 {
                "---".to_owned()
            } else {
                line.label.to_string()
            };
            (label, line.usable, line.checked)
        })
        .collect()
}

/// file > `path`: points along the submenus, then chooses the last. Returns
/// the lines of each pane that was open on the way (the last one's, too).
pub(crate) fn file_menu(ui: &MainWindow, path: &[&str]) -> Vec<Vec<(String, bool, bool)>> {
    ui.invoke_menu_title_pressed(0, 10.0, 22.0);
    let mut seen = Vec::new();
    for (depth, label) in path.iter().enumerate() {
        let shown = lines(ui, depth);
        let at = shown
            .iter()
            .position(|l| l.0 == *label)
            .unwrap_or_else(|| panic!("{label} in {shown:?}"));
        seen.push(shown);
        let at = i32::try_from(at).unwrap();
        let depth = i32::try_from(depth).unwrap();
        if usize::try_from(depth).unwrap() + 1 == path.len() {
            ui.invoke_menu_line_clicked(depth, at, 0.0, 0.0, 0.0);
        } else {
            ui.invoke_menu_line_hovered(
                depth,
                at,
                200.0 + 150.0 * depth as f32,
                40.0 + 22.0 * at as f32,
                150.0 * depth as f32,
            );
        }
    }
    seen
}

/// The pointer is lifted from every menu.
fn no_menus(ui: &MainWindow) {
    assert_eq!(ui.get_menu_open(), -1);
}

fn request_close(ui: &MainWindow) {
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
}

fn exit_settings(store: &Store, confirm: bool, action: u8, last_done: i64) {
    store
        .write(move |ctx| {
            let mut gui: GuiSettings = settings::get(ctx.conn())?;
            gui.confirm_exit = confirm;
            settings::set(ctx.conn(), &gui)?;
            let mut shutdown: ShutdownWork = settings::get(ctx.conn())?;
            shutdown.action = action;
            shutdown.last_done = last_done;
            settings::set(ctx.conn(), &shutdown)
        })
        .unwrap();
}

fn last_done(store: &Store) -> i64 {
    store.read(settings::get::<ShutdownWork>).unwrap().last_done
}

/// The restart flag is process-wide: the tests that set it take turns.
static RESTART_FLAG: std::sync::Mutex<()> = std::sync::Mutex::new(());

const EXIT_QUESTION: &str =
    "Are you sure you want to exit the client? (Will auto-yes in 15 seconds)";
const RESTART_QUESTION: &str =
    "Are you sure you want to restart the client? (Will auto-yes in 15 seconds)";

#[test]
fn exit_restart_and_force_maintenance_ask_as_the_reference_does_and_close_the_client() {
    let _turn = RESTART_FLAG
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_dirs, store) = fixture_store("basic");
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let _bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    hydrus_gui::client_exit::RESTART.store(false, Ordering::SeqCst);
    hydrus_gui::client_exit::set_space(Some(Rc::new(|| (1, 1 << 40))));

    // the entries are all there, in the reference's order (confirmation on:
    // choosing exit asks the exit question, answered no)
    exit_settings(&store, true, 0, 0);
    let shown = file_menu(&ui, &["exit"]);
    let labels: Vec<&str> = shown[0].iter().map(|l| l.0.as_str()).collect();
    assert_eq!(
        &labels[labels.len() - 3..],
        ["restart", "exit/force maintenance", "exit"]
    );
    assert_eq!(ui.get_question(), EXIT_QUESTION);
    ui.invoke_answer(false);
    assert!(ui.window().is_visible(), "declined, the client stays");
    assert!(ui.get_question().is_empty());
    assert_eq!(last_done(&store), 0, "no maintenance asked for or run");

    // restart asks its own question; declined, it neither restarts nor
    // leaves the next plain close restarting
    file_menu(&ui, &["restart"]);
    assert_eq!(ui.get_question(), RESTART_QUESTION);
    ui.invoke_answer(false);
    assert!(ui.window().is_visible());
    assert!(!hydrus_gui::client_exit::RESTART.load(Ordering::SeqCst));
    request_close(&ui);
    assert_eq!(ui.get_question(), EXIT_QUESTION, "back to a plain exit");
    ui.invoke_answer(false);

    // restart, accepted: the window closes and the process is to start again
    file_menu(&ui, &["restart"]);
    ui.invoke_answer(true);
    assert!(!ui.window().is_visible());
    assert!(hydrus_gui::client_exit::RESTART.load(Ordering::SeqCst));
    hydrus_gui::client_exit::RESTART.store(false, Ordering::SeqCst);
    assert_eq!(last_done(&store), 0, "shutdown work was not due");

    // exit/force maintenance: the shutdown work runs whatever the options
    // say, even though it is switched off and not yet due again
    ui.show().unwrap();
    let _bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let recently = hydrus_core::time::TimestampMs::now().secs() - 10;
    exit_settings(&store, false, 0, recently);
    file_menu(&ui, &["exit/force maintenance"]);
    assert!(!ui.window().is_visible(), "no confirmation: closed at once");
    assert!(!hydrus_gui::client_exit::RESTART.load(Ordering::SeqCst));
    assert!(last_done(&store) > recently, "the work ran and registered");

    // plain exit leaves the same options alone
    ui.show().unwrap();
    let _bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let before = last_done(&store);
    file_menu(&ui, &["exit"]);
    assert!(!ui.window().is_visible());
    assert_eq!(last_done(&store), before);
    no_menus(&ui);
}

// leaf: audit-options-menu-menu-file-database-directory
// leaf: audit-options-menu-menu-file-installation-directory
// leaf: audit-options-menu-menu-file-manage-import-folders
// leaf: audit-options-menu-menu-file-manage-export-folders
#[test]
fn open_entries_launch_directories_and_manage_entries_open_the_folder_lists() {
    let (_dirs, store) = fixture_store("basic");
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let launched: Rc<RefCell<Vec<String>>> = Rc::default();
    hydrus_gui::set_launcher({
        let launched = launched.clone();
        move |target| launched.borrow_mut().push(target.to_owned())
    });

    // what the reference launched from each entry (recorded): its database
    // directory, then its installation directory
    let recorded = hydrus_testkit::fixture_json("file_menu_exit.json");
    let theirs = recorded["launched"].as_array().unwrap();
    assert_eq!(
        (
            theirs[0]["is_db_dir"].as_bool(),
            theirs[1]["is_base_dir"].as_bool()
        ),
        (Some(true), Some(true))
    );
    let shown = file_menu(&ui, &["open", "database directory"]);
    assert_eq!(
        shown[1].iter().map(|l| l.0.as_str()).collect::<Vec<_>>(),
        [
            "installation directory",
            "database directory",
            "quick export directory"
        ]
    );
    assert_eq!(
        *launched.borrow(),
        [store.dir().to_string_lossy().into_owned()]
    );
    // (the Rust client's installation is its executable's directory)
    file_menu(&ui, &["open", "installation directory"]);
    let exe = std::env::current_exe().unwrap();
    assert_eq!(
        launched.borrow().last().unwrap(),
        &exe.parent().unwrap().to_string_lossy().into_owned()
    );
    assert_eq!(launched.borrow().len(), 2);
    no_menus(&ui);

    // the manage entries open the real lists, from the store
    assert!(bound.folders.import_list.borrow().is_none());
    file_menu(
        &ui,
        &["import/export folders", "manage import folders\u{2026}"],
    );
    let list = bound
        .folders
        .import_list
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(list.get_window_title(), "edit import folders");
    file_menu(
        &ui,
        &["import/export folders", "manage export folders\u{2026}"],
    );
    let export = bound
        .folders
        .export_list
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(export.get_window_title(), "edit export folders");
    assert_eq!(launched.borrow().len(), 2, "nothing was launched");
}

fn paused(store: &Store) -> (bool, bool) {
    let folders: FolderSettings = store.read(settings::get).unwrap();
    (folders.pause_import_folders, folders.pause_export_folders)
}

/// A downloader on the store, to run the import-folder worker as the daemon does.
fn downloader(store: &Arc<Store>) -> hydrus_download::Downloader {
    let net = Arc::new(
        hydrus_net::NetEngine::new(store.clone(), hydrus_net::NetOptions::default()).unwrap(),
    );
    let importer = hydrus_import::FileImporter::new(store.clone(), hydrus_media::MediaTools::new());
    hydrus_download::Downloader::new(store.clone(), net, importer).unwrap()
}

// leaf: audit-options-menu-menu-file-import-folders
// leaf: audit-options-menu-menu-file-check-import-folder-now-folder
// leaf: audit-options-menu-menu-file-check-all
#[test]
fn import_folders_are_paused_and_checked_from_the_file_menu_and_the_worker_obeys() {
    let (_dirs, store) = fixture_store("import_folder");
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let _bound = bind(&ui, Pages::open(store.clone()).unwrap());

    // a watched directory with one old picture, and a second folder over it
    let watched = tempfile::tempdir().unwrap();
    let to = watched.path().join("lease.png");
    std::fs::copy(hydrus_testkit::fixture_path("import_folder/a.png"), &to).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&to)
        .unwrap()
        .set_modified(std::time::UNIX_EPOCH + Duration::from_secs(1_600_000_000))
        .unwrap();
    let path = watched.path().to_string_lossy().into_owned();
    let id = store
        .write(move |ctx| {
            let mut folder = import_folders::find_import_folder(ctx.conn(), "drop box")?.unwrap();
            folder.settings.path = path;
            folder.settings.check_now = false;
            folder.settings.check_regularly = false;
            folder.settings.last_modified_time_skip_period = 0;
            folder.settings.actions = hydrus_parse::folders::FolderActions::default();
            import_folders::set_settings(ctx.conn(), folder.id(), &folder.settings)?;
            // (paused by hand: checking it from the menu is to unpause it, or
            // the worker below would never look at it)
            hydrus_store::queues::set_paused(ctx.conn(), folder.id(), Some(true), None)?;
            let mut second = folder.settings.clone();
            second.path = std::env::temp_dir().to_string_lossy().into_owned();
            import_folders::create_import_folder(
                ctx.conn(),
                "second",
                &second,
                &hydrus_core::import_options::ImportOptionsSlice::default(),
                false,
                0,
            )?;
            Ok(folder.id())
        })
        .unwrap();
    let checks = |store: &Store| -> Vec<(String, bool)> {
        store
            .read(import_folders::import_folders)
            .unwrap()
            .iter()
            .map(|f| (f.name().to_owned(), f.settings.check_now))
            .collect()
    };
    assert_eq!(
        checks(&store),
        [("drop box".to_owned(), false), ("second".to_owned(), false)]
    );

    // the menu lists "check all", a separator, then each folder by name
    let shown = file_menu(
        &ui,
        &["import/export folders", "check import folder now", "second"],
    );
    let names: Vec<&str> = shown[2].iter().map(|l| l.0.as_str()).collect();
    assert_eq!(names, ["check all", "---", "drop box", "second"]);
    assert_eq!(
        checks(&store),
        [("drop box".to_owned(), false), ("second".to_owned(), true)],
        "only the named folder"
    );
    file_menu(
        &ui,
        &[
            "import/export folders",
            "check import folder now",
            "check all",
        ],
    );
    assert!(checks(&store).iter().all(|c| c.1), "all of them");
    assert!(
        store
            .read(import_folders::import_folders)
            .unwrap()
            .iter()
            .all(|f| !f.paused()),
        "checking unpauses"
    );
    // (only the one the worker is to look at stays flagged)
    store
        .write(|ctx| {
            for folder in import_folders::import_folders(ctx.conn())? {
                let mut s = folder.settings.clone();
                s.check_now = folder.name() == "drop box";
                import_folders::set_settings(ctx.conn(), folder.id(), &s)?;
            }
            Ok(())
        })
        .unwrap();

    // pause > import folders: ticked once paused, and the worker does nothing
    let worker = downloader(&store);
    let shown = file_menu(&ui, &["import/export folders", "pause", "import folders"]);
    assert_eq!(
        shown[2],
        [
            ("import folders".to_owned(), true, false),
            ("export folders".to_owned(), true, false)
        ]
    );
    assert_eq!(paused(&store), (true, false));
    let run = worker.work_on_import_folder(id).unwrap();
    assert!(!run.checked, "paused folders are not checked");
    assert_eq!(checks(&store)[0], ("drop box".to_owned(), true));
    let shown = file_menu(&ui, &["import/export folders", "pause", "import folders"]);
    assert_eq!(
        shown[2],
        [
            ("import folders".to_owned(), true, true),
            ("export folders".to_owned(), true, false)
        ],
        "the menu showed it ticked"
    );
    assert_eq!(paused(&store), (false, false));

    // unpaused, the flagged folder is checked now, and finds the picture
    let run = worker.work_on_import_folder(id).unwrap();
    assert!(run.checked, "{run:?}");
    assert_eq!(run.new_files, 1);
    assert_eq!(run.error, None);
    assert_eq!(checks(&store)[0], ("drop box".to_owned(), false));
}

// leaf: audit-options-menu-menu-file-export-folders
// leaf: audit-options-menu-menu-file-run-export-folder-now-folder
// leaf: audit-options-menu-menu-file-run-all
#[test]
fn export_folders_are_paused_and_run_from_the_file_menu_and_the_worker_obeys() {
    let (_dirs, store) = fixture_store("export_folder");
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let _bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let work = tempfile::tempdir().unwrap();
    // every folder to its own empty directory, none due on its own
    store
        .write({
            let root = work.path().to_path_buf();
            move |ctx| {
                let mut folders: ExportFolders = settings::get(ctx.conn())?;
                for folder in &mut folders.0 {
                    let dir = root.join(&folder.name);
                    std::fs::create_dir_all(&dir).unwrap();
                    folder.path = dir.to_string_lossy().into_owned();
                    folder.run_regularly = false;
                    folder.run_now = false;
                }
                settings::set(ctx.conn(), &folders)
            }
        })
        .unwrap();
    let flags = |store: &Store| -> Vec<(String, bool)> {
        let folders: ExportFolders = store.read(settings::get).unwrap();
        let mut flags: Vec<_> = folders.0.into_iter().map(|f| (f.name, f.run_now)).collect();
        flags.sort();
        flags
    };
    let all_false = flags(&store);
    assert_eq!(all_false.len(), 3);
    assert!(all_false.iter().all(|f| !f.1));

    // "run all", a separator, then the folders by name
    let shown = file_menu(
        &ui,
        &["import/export folders", "run export folder now", "sync"],
    );
    let names: Vec<&str> = shown[2].iter().map(|l| l.0.as_str()).collect();
    assert_eq!(names, ["run all", "---", "delete after", "regular", "sync"]);
    let one = flags(&store);
    assert_eq!(
        one.iter()
            .filter(|f| f.1)
            .map(|f| f.0.as_str())
            .collect::<Vec<_>>(),
        ["sync"],
        "only the named folder"
    );

    // paused, the worker runs nothing and leaves the flag alone
    file_menu(&ui, &["import/export folders", "pause", "export folders"]);
    assert_eq!(paused(&store), (false, true));
    assert!(
        hydrus_download::export::work_export_folders(&store)
            .unwrap()
            .is_empty()
    );
    assert_eq!(flags(&store), one);
    // unpaused, it runs the flagged folder once and clears the flag
    let shown = file_menu(&ui, &["import/export folders", "pause", "export folders"]);
    assert!(shown[2][1].2, "ticked while paused");
    assert_eq!(paused(&store), (false, false));
    let runs = crate::common::export_folders_until_done(&store);
    assert_eq!(
        runs.iter().map(|r| r.0.as_str()).collect::<Vec<_>>(),
        ["sync"]
    );
    assert_eq!(flags(&store), all_false);

    // run all flags every folder, and all of them run
    file_menu(
        &ui,
        &["import/export folders", "run export folder now", "run all"],
    );
    assert!(flags(&store).iter().all(|f| f.1));
    let runs = crate::common::export_folders_until_done(&store);
    assert_eq!(runs.len(), 3);
    assert_eq!(flags(&store), all_false);
}

// leaf: audit-options-menu-menu-file-check-all
// leaf: audit-options-menu-menu-file-check-import-folder-now-folder
// leaf: audit-options-menu-menu-file-run-all
// leaf: audit-options-menu-menu-file-run-export-folder-now-folder
#[test]
fn checking_and_running_folders_from_the_file_menu_flags_and_says_what_the_reference_did() {
    use hydrus_store::popups;

    let recorded = hydrus_testkit::fixture_json("folder_runs.json");
    // (the export folders of this fixture stand in for the recorded three,
    // in order: the second is the one run alone)
    let (_dirs, store) = fixture_store("export_folder");
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let _bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let export_names: Vec<String> = {
        let folders: ExportFolders = store.read(settings::get).unwrap();
        let mut names: Vec<_> = folders.0.iter().map(|f| f.name.clone()).collect();
        names.sort();
        names
    };
    assert_eq!(export_names.len(), 3);
    let imports: Vec<(String, bool, bool)> = recorded["import_folders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            (
                f["name"].as_str().unwrap().to_owned(),
                f["paused"].as_bool().unwrap(),
                f["check_regularly"].as_bool().unwrap(),
            )
        })
        .collect();
    let recorded_exports: Vec<String> = recorded["export_folders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap().to_owned())
        .collect();
    // import folders as recorded, once
    store
        .write({
            let imports = imports.clone();
            move |ctx| {
                for (name, paused, regularly) in &imports {
                    let settings = hydrus_parse::folders::ImportFolderSettings {
                        path: format!("/nonexistent/{name}"),
                        check_regularly: *regularly,
                        ..hydrus_parse::folders::ImportFolderSettings::default()
                    };
                    import_folders::create_import_folder(
                        ctx.conn(),
                        name,
                        &settings,
                        &hydrus_core::import_options::ImportOptionsSlice::default(),
                        *paused,
                        0,
                    )?;
                }
                Ok(())
            }
        })
        .unwrap();
    let state = |store: &Store| -> serde_json::Value {
        let mut imports = serde_json::Map::new();
        for folder in store.read(import_folders::import_folders).unwrap() {
            if recorded["import_folders"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["name"] == folder.name())
            {
                imports.insert(
                    folder.name().to_owned(),
                    serde_json::json!({"paused": folder.paused(), "check_now": folder.settings.check_now}),
                );
            }
        }
        let folders: ExportFolders = store.read(settings::get).unwrap();
        let mut exports = serde_json::Map::new();
        for (i, name) in recorded_exports.iter().enumerate() {
            let ours = folders
                .0
                .iter()
                .find(|f| f.name == export_names[i])
                .unwrap();
            exports.insert(name.clone(), serde_json::json!({"run_now": ours.run_now}));
        }
        serde_json::json!({"imports": imports, "exports": exports})
    };
    let said = |store: &Store| -> Vec<String> {
        store
            .read(|c| popups::all(c, hydrus_core::TimestampMs::now().0 / 1000 + 1))
            .unwrap()
            .into_iter()
            .filter_map(|job| job.status_text_1)
            .collect()
    };
    let mut seen = 0;
    for case in recorded["cases"].as_array().unwrap() {
        // the folders as the recording began each case
        store
            .write({
                let imports = imports.clone();
                let sync_paused = case["sync_paused"].as_bool().unwrap();
                move |ctx| {
                    let conn = ctx.conn();
                    for folder in import_folders::import_folders(conn)? {
                        if let Some((_, paused, _)) =
                            imports.iter().find(|(n, _, _)| n == folder.name())
                        {
                            let mut settings = folder.settings.clone();
                            settings.check_now = false;
                            import_folders::set_settings(conn, folder.id(), &settings)?;
                            hydrus_store::queues::set_paused(
                                conn,
                                folder.id(),
                                Some(*paused),
                                None,
                            )?;
                        }
                    }
                    let mut exports: ExportFolders = settings::get(conn)?;
                    for folder in &mut exports.0 {
                        folder.run_now = false;
                    }
                    settings::set(conn, &exports)?;
                    let mut folders: FolderSettings = settings::get(conn)?;
                    folders.pause_import_folders = sync_paused;
                    folders.pause_export_folders = sync_paused;
                    settings::set(conn, &folders)
                }
            })
            .unwrap();
        assert_eq!(state(&store), case["before"], "{}", case["what"]);
        let before_said = said(&store);
        let (menu, entry): (&[&str], &str) = match case["what"].as_str().unwrap() {
            "check one" => (
                &["import/export folders", "check import folder now"],
                "paused one",
            ),
            "check one, not paused" => (
                &["import/export folders", "check import folder now"],
                "drop box",
            ),
            "check all" => (
                &["import/export folders", "check import folder now"],
                "check all",
            ),
            "run one" => (
                &["import/export folders", "run export folder now"],
                export_names[1].as_str(),
            ),
            _ => (
                &["import/export folders", "run export folder now"],
                "run all",
            ),
        };
        let mut path = menu.to_vec();
        path.push(entry);
        file_menu(&ui, &path);
        assert_eq!(state(&store), case["after"], "{}", case["what"]);
        let mut new: Vec<String> = said(&store);
        new.retain(|text| !before_said.contains(text));
        let theirs: Vec<String> = case["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m.as_str().unwrap().to_owned())
            .collect();
        if theirs.is_empty() {
            assert!(new.is_empty(), "{} said {new:?}", case["what"]);
        } else {
            assert_eq!(new, theirs, "{}", case["what"]);
        }
        // (so that the next case's message is new)
        store
            .write(|ctx| {
                for job in popups::all(ctx.conn(), hydrus_core::TimestampMs::now().0 / 1000 + 1)? {
                    popups::update(ctx.conn(), &job.key, 0, |job| job.dismissed = true)?;
                }
                Ok(())
            })
            .unwrap();
        seen += 1;
    }
    assert_eq!(seen, 10);
}

/// Pages for one recorded run: a search page, and (as the run names them)
/// a url import page holding four URLs and a simple downloader holding two,
/// their files playing unless the run pauses them.
fn exit_session(store: &Arc<Store>, kinds: &[String]) -> Pages {
    use hydrus_core::import_options::ImportOptionsSlice;
    use hydrus_core::pages::{DownloaderKind, Page, PageContent, PageKey, Session};
    use hydrus_search::FileSearchContext;
    use hydrus_store::queues::{self, FileSeedMeta, NewFileSeed, QueueKind, SeedType};
    let paused = kinds.iter().any(|k| k == "paused");
    let seeds = |urls: Vec<String>| -> Vec<NewFileSeed> {
        urls.into_iter()
            .map(|u| NewFileSeed {
                seed_type: SeedType::Url,
                data: u.clone(),
                data_for_comparison: u,
                source_time: None,
                referral_url: None,
                meta: FileSeedMeta::default(),
            })
            .collect()
    };
    let urls: Vec<String> = (1..=4)
        .map(|n| format!("https://site.example/post/{n}"))
        .collect();
    let files: Vec<String> = (1..=2)
        .map(|n| format!("https://site.example/file/{n}.jpg"))
        .collect();
    let kinds = kinds.to_vec();
    let (url_queue, simple_queue) = store
        .write(move |ctx| {
            let conn = ctx.conn();
            let options = ImportOptionsSlice::default();
            let url_queue =
                queues::create_queue(conn, QueueKind::Urls, "url import", None, &options, 0)?;
            queues::add_file_seeds(conn, url_queue, &seeds(urls), false, 0)?;
            queues::set_paused(conn, url_queue, Some(paused), Some(paused))?;
            let simple_queue = queues::create_simple_downloader(
                conn,
                None,
                &options,
                &queues::SimpleDownloader::default(),
                0,
            )?;
            queues::add_file_seeds(conn, simple_queue, &seeds(files), false, 0)?;
            queues::set_paused(conn, simple_queue, Some(false), Some(false))?;
            Ok((url_queue, simple_queue))
        })
        .unwrap();
    let page = |name: &str, content: PageContent| Page {
        key: PageKey::random(),
        name: name.into(),
        content,
    };
    let downloader = |name: &str, kind: DownloaderKind, queue: i64| {
        page(
            name,
            PageContent::Downloader {
                kind,
                queues: vec![queue],
                sort: None,
                page: None,
            },
        )
    };
    let mut pages = vec![page(
        "a",
        PageContent::Search {
            search: FileSearchContext::default(),
            synchronised: true,
            sort: None,
            lock: None,
            collect: None,
        },
    )];
    if kinds.iter().any(|k| k == "urls") {
        pages.push(downloader("url import", DownloaderKind::Urls, url_queue));
    }
    if kinds.iter().any(|k| k == "simple") {
        pages.push(downloader(
            "simple downloader",
            DownloaderKind::Simple,
            simple_queue,
        ));
    }
    let session = Session {
        name: hydrus_store::sessions::LAST_SESSION.into(),
        pages,
    };
    store
        .write(move |ctx| hydrus_store::sessions::save(ctx.conn(), &session, 0))
        .unwrap();
    Pages::open(store.clone()).unwrap()
}

/// One recorded question is on screen as recorded: its text, title, buttons
/// and (for the pages' dialog) the delay before "yes" can be pressed.
/// Returns whether it is the pages' dialog.
fn check_question(ui: &MainWindow, asked: &serde_json::Value, name: &str) -> bool {
    let text = asked["text"].as_str().unwrap();
    if asked["kind"] == "yes_no_no" {
        let dialog = hydrus_gui::reasons_dialog::dialog()
            .unwrap_or_else(|| panic!("{name}: the pages' dialog is asked"));
        assert!(
            ui.get_question().is_empty(),
            "{name}: not also the plain one"
        );
        assert_eq!(dialog.get_message(), text, "{name}");
        assert_eq!(
            dialog.get_window_title(),
            asked["title"].as_str().unwrap(),
            "{name}"
        );
        let mut buttons = vec![asked["yes_label"].as_str().unwrap().to_owned()];
        buttons.extend(
            asked["no_labels"]
                .as_array()
                .unwrap()
                .iter()
                .map(|l| l.as_str().unwrap().to_owned()),
        );
        let shown: Vec<String> = dialog.get_choices().iter().map(|c| c.to_string()).collect();
        assert_eq!(shown, buttons, "{name}: buttons");
        let enabled: Vec<bool> = dialog.get_choices_enabled().iter().collect();
        assert_eq!(
            enabled[0],
            !asked["disable_yes_initially"].as_bool().unwrap(),
            "{name}: yes starts disabled"
        );
        true
    } else {
        assert_eq!(ui.get_question(), text, "{name}");
        assert_eq!(
            ui.get_question_title(),
            asked["title"].as_str().unwrap(),
            "{name}: title"
        );
        let win = ui.window();
        widgets::position(win, asked["yes_label"].as_str().unwrap(), 0);
        widgets::position(win, asked["no_label"].as_str().unwrap(), 0);
        false
    }
}

// leaf: audit-options-menu-menu-file-exit
// leaf: audit-options-menu-menu-file-exit-force-maintenance
// leaf: audit-options-menu-menu-file-restart
#[test]
fn exit_restart_and_force_maintenance_replay_the_references_questions_and_outcomes() {
    let _turn = RESTART_FLAG
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let recorded = hydrus_testkit::fixture_json("file_menu_exit.json");
    let (_dirs, store) = fixture_store("basic");
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let mut seen_work = (false, false);
    // a stand-in program for the client: it says it was started
    let scratch = tempfile::tempdir().unwrap();
    let program = scratch.path().join("client.sh");
    std::fs::write(&program, "#!/bin/sh\necho \"$2\" > \"$1\"\n").unwrap();
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let mut restarts = 0;
    let mut saw_restart = false;
    let (mut saw_auto_yes, mut saw_low_space, mut saw_importers) = (false, false, false);
    for run in recorded["exits"].as_array().unwrap() {
        let asked = run["asked"].as_array().unwrap();
        let accepted_at_last = run["answer"].as_bool().unwrap()
            && asked
                .last()
                .is_some_and(|q| q["auto_yes_time"].as_u64().is_some());
        // (a question that is accepted is answered once by its button and
        // once by waiting out its auto-yes)
        for auto in [false, true] {
            if auto && !accepted_at_last {
                continue;
            }
            let name = format!(
                "{}{}",
                run["name"].as_str().unwrap(),
                if auto { " (auto-yes)" } else { "" }
            );
            let name = name.as_str();
            ui.show().unwrap();
            let kinds: Vec<String> = run["pages"]
                .as_array()
                .map(|p| p.iter().map(|k| k.as_str().unwrap().to_owned()).collect())
                .unwrap_or_default();
            let _bound = bind(&ui, exit_session(&store, &kinds));
            widgets::lay_out(ui.window(), 1100.0, 700.0);
            hydrus_gui::client_exit::RESTART.store(false, Ordering::SeqCst);
            // (the room on the drive: as recorded, else plenty)
            let space = run["space"].as_array().map_or((1, 1 << 40), |s| {
                (s[0].as_u64().unwrap(), s[1].as_u64().unwrap())
            });
            hydrus_gui::client_exit::set_space(Some(Rc::new(move || space)));
            // (the reference's options: leave on shutdown or never; work due or
            // done a moment ago)
            let recently = hydrus_core::time::TimestampMs::now().secs() - 10;
            let due = run["work_due"].as_bool().unwrap();
            let before = if due { 0 } else { recently };
            exit_settings(
                &store,
                run["confirm"].as_bool().unwrap(),
                u8::from(run["idle_shutdown"] == 1),
                before,
            );
            file_menu(&ui, &[run["entry"].as_str().unwrap()]);
            for (n, question) in asked.iter().enumerate() {
                let last = n + 1 == asked.len();
                let pages_dialog = check_question(&ui, question, name);
                saw_low_space |= question["yes_label"] == "try exiting now";
                saw_importers |= pages_dialog
                    && question["text"]
                        .as_str()
                        .unwrap()
                        .contains("Importers will save and continue their work on the next start.");
                let answer = run["answer"].as_bool().unwrap();
                if auto && last {
                    // nothing is answered by itself before the time
                    let wait = Duration::from_secs(question["auto_yes_time"].as_u64().unwrap());
                    headless::advance_time(wait.checked_sub(Duration::from_secs(2)).unwrap());
                    assert!(
                        pages_dialog || !ui.get_question().is_empty(),
                        "{name}: still asked just before the time"
                    );
                    assert!(
                        !pages_dialog || hydrus_gui::reasons_dialog::dialog().is_some(),
                        "{name}: still asked just before the time"
                    );
                    headless::advance_time(Duration::from_secs(3));
                    saw_auto_yes = true;
                } else if pages_dialog {
                    let dialog = hydrus_gui::reasons_dialog::dialog().unwrap();
                    // "yes" waits a moment before it can be pressed
                    headless::advance_time(Duration::from_secs(2));
                    assert!(dialog.get_choices_enabled().iter().all(|e| e), "{name}");
                    dialog.invoke_chosen(i32::from(!answer));
                } else {
                    ui.invoke_answer(answer);
                }
            }
            if asked.is_empty() {
                assert!(ui.get_question().is_empty(), "{name}: nothing is asked");
                assert!(
                    hydrus_gui::reasons_dialog::dialog().is_none_or(|d| !d.window().is_visible()),
                    "{name}: nothing is asked"
                );
            }
            if let Some(exit) = run["exited"].as_array().unwrap().first() {
                assert!(!ui.window().is_visible(), "{name}: the client exits");
                assert_eq!(
                    hydrus_gui::client_exit::RESTART.load(Ordering::SeqCst),
                    exit["restart"].as_bool().unwrap(),
                    "{name}"
                );
                let ran = last_done(&store) > before;
                assert_eq!(
                    ran,
                    exit["idle_work"].as_bool().unwrap(),
                    "{name}: shutdown work"
                );
                seen_work = (seen_work.0 || ran, seen_work.1 || !ran);
            } else {
                assert!(ui.window().is_visible(), "{name}: the client stays");
                assert!(
                    !hydrus_gui::client_exit::RESTART.load(Ordering::SeqCst),
                    "{name}"
                );
                assert_eq!(last_done(&store), before, "{name}");
            }
            // once out, the process starts again (with its arguments) just when
            // the reference restarted
            restarts += 1;
            let started = scratch.path().join(format!("started-{restarts}"));
            let relaunched = hydrus_gui::client_exit::relaunch_if_requested(
                &program,
                [started.clone().into_os_string(), "--db-dir=x".into()],
            )
            .unwrap();
            let restarted = run["exited"]
                .as_array()
                .unwrap()
                .first()
                .is_some_and(|exit| exit["restart"].as_bool().unwrap());
            assert_eq!(relaunched, restarted, "{name}");
            if relaunched {
                let start = std::time::Instant::now();
                while !started.exists() {
                    assert!(
                        start.elapsed() < Duration::from_secs(10),
                        "{name}: never started"
                    );
                    std::thread::sleep(Duration::from_millis(10));
                }
                std::thread::sleep(Duration::from_millis(100));
                assert_eq!(
                    std::fs::read_to_string(&started).unwrap().trim(),
                    "--db-dir=x"
                );
                saw_restart = true;
            } else {
                std::thread::sleep(Duration::from_millis(50));
                assert!(!started.exists(), "{name}");
            }
            // (the next run starts a plain exit again)
            hydrus_gui::client_exit::RESTART.store(false, Ordering::SeqCst);
            if ui.window().is_visible() {
                ui.hide().unwrap();
            }
        }
    }
    hydrus_gui::client_exit::set_space(None);
    assert!(saw_restart);
    assert!(saw_auto_yes && saw_low_space && saw_importers);
    assert_eq!(
        seen_work,
        (true, true),
        "work ran and was left, as recorded"
    );
}
