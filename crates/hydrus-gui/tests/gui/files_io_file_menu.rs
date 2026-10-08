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
fn file_menu(ui: &MainWindow, path: &[&str]) -> Vec<Vec<(String, bool, bool)>> {
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

const EXIT_QUESTION: &str =
    "Are you sure you want to exit the client? (Will auto-yes in 15 seconds)";
const RESTART_QUESTION: &str =
    "Are you sure you want to restart the client? (Will auto-yes in 15 seconds)";

// leaf: audit-options-menu-menu-file-exit
// leaf: audit-options-menu-menu-file-restart
// leaf: audit-options-menu-menu-file-exit-force-maintenance
#[test]
fn exit_restart_and_force_maintenance_ask_as_the_reference_does_and_close_the_client() {
    let (_dirs, store) = fixture_store("basic");
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let _bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    hydrus_gui::client_exit::RESTART.store(false, Ordering::SeqCst);

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
