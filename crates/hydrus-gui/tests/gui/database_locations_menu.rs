//! Database > locations…, from the real menu (`MoveMediaFilesPanel`): the
//! list of media locations with weights and limits, the thumbnail location
//! override, "move files now" with its run-time question, and the
//! granularity window (this client, and an offline folder).

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use super::database_menu_jobs::{from_menu_path, pump, shown};
use hydrus_gui::database_locations_window::{max_size_window, opened, runtime_question};
use hydrus_gui::{
    DatabaseLocationsWindow, MainWindow, Pages, SearchPage, bind, headless, message_window,
};
use hydrus_gui_model::database_locations as model;
use hydrus_store::Store;
use hydrus_store::storage_locations as locations;
use slint::{ComponentHandle as _, Model as _};

struct Rig {
    home: tempfile::TempDir,
    store: std::sync::Arc<Store>,
    ui: MainWindow,
    _bound: hydrus_gui::Bound,
    _windows: headless::Windows,
    /// What the folder picker answers next, and what it was asked.
    next: Rc<RefCell<Vec<PathBuf>>>,
    asked: Rc<RefCell<Vec<String>>>,
}

fn rig() -> Rig {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    std::fs::create_dir_all(home.path().join("client_files")).unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let next: Rc<RefCell<Vec<PathBuf>>> = Rc::default();
    let asked: Rc<RefCell<Vec<String>>> = Rc::default();
    {
        let (next, asked) = (next.clone(), asked.clone());
        hydrus_gui::set_picker(move |_, title| {
            asked.borrow_mut().push(title.to_owned());
            std::mem::take(&mut *next.borrow_mut())
        });
    }
    Rig {
        home,
        store,
        ui,
        _bound: bound,
        _windows: windows,
        next,
        asked,
    }
}

impl Rig {
    fn open(&self) -> DatabaseLocationsWindow {
        from_menu_path(&self.ui, &[], "locations\u{2026}");
        let window = opened().expect("the window opened");
        assert!(window.window().is_visible());
        window
    }
    fn review(&self) -> locations::Review {
        self.store.read(locations::review).unwrap()
    }
    fn weight_of(&self, path: &Path) -> Option<i64> {
        self.review()
            .locations
            .iter()
            .find(|l| l.path == path)
            .map(|l| l.weight)
    }
    /// Pick this folder for the next action.
    fn pick(&self, path: &Path) {
        *self.next.borrow_mut() = vec![path.to_path_buf()];
    }
    /// Answer the main window's question.
    fn answer(&self, yes: bool) -> String {
        let question = self.ui.get_question().to_string();
        assert!(!question.is_empty(), "a question is asked");
        self.ui.invoke_answer(yes);
        question
    }
}

fn cells(window: &DatabaseLocationsWindow) -> Vec<Vec<String>> {
    window
        .get_rows()
        .iter()
        .map(|row| row.cells.iter().map(|c| c.to_string()).collect())
        .collect()
}

fn select(window: &DatabaseLocationsWindow, path: &Path) {
    let at = cells(window)
        .iter()
        .position(|row| row[0] == path.display().to_string())
        .expect("the location is listed");
    window.invoke_clicked(i32::try_from(at).unwrap(), false, false);
}

fn warning(text: &str) {
    let window = message_window().expect("a warning is shown");
    assert_eq!(window.get_message(), text);
    window.invoke_cancelled();
}

// leaf: audit-media-database-locations-paths
#[test]
fn locations_lists_weights_and_adds_changes_and_removes_them_with_the_reference_s_guards() {
    let rig = rig();
    let window = rig.open();
    assert_eq!(window.get_warning(), model::WARNING);
    let media = rig.home.path().join("client_files");
    let rows = cells(&window);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0][0], media.display().to_string());
    assert_eq!(rows[0][1], "yes", "beneath the db dir");
    assert_eq!(rows[0][4], "1");
    // nothing selected: nothing to change
    assert_eq!(window.get_buttons().iter().collect::<Vec<_>>(), [false; 4]);

    // adding a location gives it weight 1, and it is listed with its share
    let other = tempfile::tempdir().unwrap();
    rig.pick(other.path());
    window.invoke_action("add".into());
    assert_eq!(rig.asked.borrow().last().unwrap(), model::PICK_LOCATION);
    assert_eq!(rig.weight_of(other.path()), Some(1));
    let rows = cells(&window);
    assert_eq!(rows.len(), 2);
    let new = rows
        .iter()
        .find(|r| r[0] == other.path().display().to_string())
        .unwrap();
    assert_eq!((new[1].as_str(), new[4].as_str()), ("no", "1"));
    // ... once; entering it again is told so
    rig.pick(other.path());
    window.invoke_action("add".into());
    warning(model::ALREADY_ENTERED);
    // cancelling the picker adds nothing
    window.invoke_action("add".into());
    assert_eq!(rig.review().locations.len(), 2);

    // weights go up and down; down from 1 asks to remove
    select(&window, other.path());
    assert_eq!(
        window.get_buttons().iter().collect::<Vec<_>>(),
        [true, true, true, true]
    );
    window.invoke_action("increase".into());
    assert_eq!(rig.weight_of(other.path()), Some(2));
    window.invoke_action("decrease".into());
    assert_eq!(rig.weight_of(other.path()), Some(1));
    window.invoke_action("decrease".into());
    assert_eq!(
        rig.answer(false),
        model::remove_question(
            true,
            rig.review()
                .locations
                .iter()
                .any(|l| l.path == other.path() && l.files_share > 0.0)
        )
    );
    assert_eq!(rig.weight_of(other.path()), Some(1));
    window.invoke_action("remove".into());
    rig.answer(true);
    assert!(
        rig.weight_of(other.path()).is_none_or(|w| w == 0),
        "removed from the weighted locations"
    );

    // the last weighted location cannot be removed
    select(&window, &media);
    window.invoke_action("remove".into());
    warning(model::CANNOT_EMPTY_ALL);
    assert_eq!(rig.weight_of(&media), Some(1));
    // kept across a restart
    drop(window);
    let reopened = Store::open(rig.home.path()).unwrap();
    assert_eq!(
        reopened
            .read(locations::review)
            .unwrap()
            .locations
            .iter()
            .find(|l| l.path == media)
            .map(|l| l.weight),
        Some(1)
    );
}

// leaf: audit-media-database-locations-max-size
#[test]
fn a_location_s_maximum_size_is_edited_in_its_own_dialog() {
    let rig = rig();
    let media = rig.home.path().join("client_files");
    let other = tempfile::tempdir().unwrap();
    let path = other.path().to_path_buf();
    rig.store
        .write_and_refresh(move |ctx| locations::add_location(ctx.conn(), &path))
        .unwrap();
    let window = rig.open();

    // two unlimited locations: either can be limited
    select(&window, other.path());
    window.invoke_action("max".into());
    let dialog = max_size_window().expect("the dialog opened");
    assert!(dialog.window().is_visible());
    assert_eq!(dialog.get_window_title(), model::MAX_SIZE_TITLE);
    assert_eq!(dialog.get_message(), model::MAX_SIZE_MESSAGE);
    assert!(dialog.get_no_limit(), "unlimited to begin with");

    // cancelling changes nothing
    dialog.invoke_cancel();
    assert!(!dialog.window().is_visible());
    assert_eq!(
        rig.review()
            .locations
            .iter()
            .map(|l| l.max_bytes)
            .collect::<Vec<_>>(),
        [None, None]
    );

    // 5 GB (unit 3) is kept, shown in the list, and survives a restart
    window.invoke_action("max".into());
    let dialog = max_size_window().unwrap();
    dialog.set_no_limit(false);
    dialog.set_amount(5);
    dialog.set_unit(3);
    dialog.invoke_apply();
    assert!(!dialog.window().is_visible());
    let five = 5 * 1024_i64.pow(3);
    let limit = |store: &Store| {
        store
            .read(locations::review)
            .unwrap()
            .locations
            .into_iter()
            .find(|l| l.path == other.path())
            .unwrap()
            .max_bytes
    };
    assert_eq!(limit(&rig.store), Some(five));
    let row = cells(&window)
        .into_iter()
        .find(|r| r[0] == other.path().display().to_string())
        .unwrap();
    assert_eq!(
        row[5],
        hydrus_core::numbers::human_bytes(u64::try_from(five).unwrap())
    );
    assert_eq!(limit(&Store::open(rig.home.path()).unwrap()), Some(five));

    // activating the row opens it with the saved value, and "no limit" clears it
    window.invoke_activated(
        i32::try_from(
            cells(&window)
                .iter()
                .position(|r| r[0] == other.path().display().to_string())
                .unwrap(),
        )
        .unwrap(),
    );
    let dialog = max_size_window().unwrap();
    assert!(!dialog.get_no_limit());
    assert_eq!((dialog.get_amount(), dialog.get_unit()), (5, 3));
    dialog.set_no_limit(true);
    dialog.invoke_apply();
    assert_eq!(limit(&rig.store), None);

    // with one limited, the other must stay unlimited: no dialog to set it
    select(&window, &media);
    window.invoke_action("max".into());
    let dialog = max_size_window().unwrap();
    dialog.set_no_limit(false);
    dialog.set_amount(5);
    dialog.set_unit(3);
    dialog.invoke_apply();
    select(&window, other.path());
    assert!(!window.get_buttons().row_data(2).unwrap());
    select(&window, &media);
    assert!(window.get_buttons().row_data(2).unwrap());
}

// leaf: audit-media-database-locations-thumbnails
#[test]
fn the_thumbnail_location_override_is_set_and_cleared() {
    let rig = rig();
    let media = rig.home.path().join("client_files");
    let window = rig.open();
    assert_eq!(window.get_thumbnail_location(), "none set");
    assert!(!window.get_can_clear_thumbnails());

    // a regular file location cannot be the thumbnail location
    rig.pick(&media);
    window.invoke_action("set thumbnails".into());
    assert_eq!(rig.asked.borrow().last().unwrap(), model::PICK_THUMBNAILS);
    warning(model::IS_FILE_LOCATION);
    assert_eq!(window.get_thumbnail_location(), "none set");

    // another folder can
    let thumbs = tempfile::tempdir().unwrap();
    rig.pick(thumbs.path());
    window.invoke_action("set thumbnails".into());
    assert_eq!(
        window.get_thumbnail_location(),
        thumbs.path().display().to_string()
    );
    assert!(window.get_can_clear_thumbnails());
    let overridden = |store: &Store| {
        store
            .read(locations::review)
            .unwrap()
            .locations
            .into_iter()
            .filter(|l| l.thumbnail_override)
            .map(|l| l.path)
            .collect::<Vec<_>>()
    };
    assert_eq!(overridden(&rig.store), [thumbs.path().to_path_buf()]);
    assert_eq!(
        overridden(&Store::open(rig.home.path()).unwrap()),
        [thumbs.path().to_path_buf()]
    );
    // ... and that folder cannot then be added as a file location
    rig.pick(thumbs.path());
    window.invoke_action("add".into());
    warning(model::IS_THUMBNAIL_LOCATION);

    // clearing asks first
    window.invoke_action("clear thumbnails".into());
    assert_eq!(rig.answer(false), model::CLEAR_THUMBNAILS);
    assert_eq!(overridden(&rig.store).len(), 1);
    window.invoke_action("clear thumbnails".into());
    rig.answer(true);
    assert!(overridden(&rig.store).is_empty());
    assert_eq!(window.get_thumbnail_location(), "none set");
    assert!(!window.get_can_clear_thumbnails());
}

// leaf: audit-media-database-locations-rebalance
#[test]
fn move_files_now_asks_for_a_run_time_and_moves_folders_with_a_popup() {
    let rig = rig();
    let window = rig.open();
    assert_eq!(window.get_rebalance_status(), model::rebalance_label(false));
    assert!(!window.get_can_rebalance());

    let other = tempfile::tempdir().unwrap();
    rig.pick(other.path());
    window.invoke_action("add".into());
    assert_eq!(window.get_rebalance_status(), model::rebalance_label(true));
    assert!(window.get_can_rebalance());

    // a location that is not there stops it
    let gone = tempfile::tempdir().unwrap();
    let gone_path = gone.path().to_path_buf();
    rig.pick(&gone_path);
    window.invoke_action("add".into());
    drop(gone);
    window.invoke_action("rebalance".into());
    warning(&model::missing_path_warning(
        &gone_path.display().to_string(),
    ));
    std::fs::create_dir_all(&gone_path).unwrap();

    // the question, in the reference's words; "forget it" moves nothing
    window.invoke_action("rebalance".into());
    let question = runtime_question().expect("asked for a run time");
    assert_eq!(question.get_message(), model::RUNTIME_QUESTION);
    assert_eq!(question.get_no_label(), "forget it");
    let choices: Vec<String> = question
        .get_choices()
        .iter()
        .map(|c| c.to_string())
        .collect();
    assert_eq!(
        choices,
        model::RUNTIMES
            .iter()
            .map(|(l, _)| (*l).to_owned())
            .collect::<Vec<_>>()
    );
    question.invoke_cancelled();
    pump(Duration::from_millis(300));
    assert!(shown(&rig.store).is_empty());
    assert!(rig.store.read(locations::next_move).unwrap().is_some());

    // "run indefinitely" runs to the end, with the popup
    window.invoke_action("rebalance".into());
    runtime_question().unwrap().invoke_chosen(4);
    assert!(window.get_busy());
    let deadline = Instant::now() + Duration::from_secs(60);
    while window.get_busy() || rig.store.read(locations::next_move).unwrap().is_some() {
        assert!(Instant::now() < deadline, "the files never finished moving");
        pump(Duration::from_millis(50));
    }
    // (the finished popup dismisses itself, so there is none left to read)
    assert!(shown(&rig.store).is_empty());
    assert_eq!(window.get_rebalance_status(), model::rebalance_label(false));
    // every location now holds folders
    let moved = rig.review();
    assert!(
        moved.locations.iter().all(|l| l.files_share > 0.0),
        "{moved:?}"
    );
}

fn granularity_window(
    rig: &Rig,
    window: &DatabaseLocationsWindow,
) -> hydrus_gui::GranularityWindow {
    let _ = rig;
    window.invoke_action("granularity".into());
    let granularity = hydrus_gui::granularity_window::opened().expect("the window opened");
    assert!(granularity.window().is_visible());
    granularity
}

// leaf: audit-media-database-locations-granularity-client
#[test]
fn this_client_s_granularity_is_changed_after_the_last_check_and_reported() {
    use hydrus_gui_model::database_granularity as granularity;
    let rig = rig();
    let media = rig.home.path().join("client_files");
    // a file in the granularity-2 layout
    let file = "3ab4567.png";
    let from = hydrus_store::storage::prefix_dir(&media, "f3a");
    std::fs::create_dir_all(&from).unwrap();
    std::fs::write(from.join(file), b"x").unwrap();
    let window = rig.open();
    assert_eq!(window.get_granularity(), model::granularity_label(2));
    let g = granularity_window(&rig, &window);
    assert_eq!(g.get_warning(), granularity::WARNING);
    assert_eq!(g.get_intro(), granularity::INTRO);
    assert_eq!(g.get_offline_label(), granularity::OFFLINE);
    assert_eq!(g.get_client_label(), granularity::granularity_label(2));

    // the last check, in the reference's words; "no" changes nothing
    let (title, message, yes, no) = granularity::client_question(2);
    g.invoke_action("2to3".into());
    let question = hydrus_gui::granularity_window::question().expect("the last check");
    assert_eq!(question.get_window_title(), title);
    assert_eq!(question.get_message(), message);
    assert_eq!(question.get_no_label(), no);
    assert_eq!(
        question
            .get_choices()
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>(),
        [yes]
    );
    question.invoke_cancelled();
    pump(Duration::from_millis(300));
    assert!(from.join(file).is_file());
    assert_eq!(g.get_client_label(), granularity::granularity_label(2));

    // "yes" moves the files, says how it went, and the windows follow
    g.invoke_action("2to3".into());
    hydrus_gui::granularity_window::question()
        .unwrap()
        .invoke_chosen(0);
    let deadline = Instant::now() + Duration::from_secs(60);
    while message_window().is_none_or(|m| !m.window().is_visible()) {
        assert!(Instant::now() < deadline, "the migration never reported");
        pump(Duration::from_millis(50));
    }
    let report = message_window().unwrap();
    assert!(
        report.get_message().starts_with("1 files moved in "),
        "{}",
        report.get_message()
    );
    assert!(
        report
            .get_message()
            .contains("You now have finer, lower-latency file storage.")
    );
    report.invoke_cancelled();
    assert!(
        hydrus_store::storage::prefix_dir(&media, "f3a")
            .join("b")
            .join(file)
            .is_file()
    );
    assert!(!from.join(file).exists());
    assert_eq!(g.get_client_label(), granularity::granularity_label(3));
    assert_eq!(window.get_granularity(), model::granularity_label(3));
    let reopened = Store::open(rig.home.path()).unwrap();
    assert_eq!(
        reopened
            .read(hydrus_store::storage::FileStorage::load)
            .unwrap()
            .granularity(),
        3
    );

    // and back again
    let (_, message, _, _) = granularity::client_question(3);
    g.invoke_action("3to2".into());
    let question = hydrus_gui::granularity_window::question().unwrap();
    assert_eq!(question.get_message(), message);
    question.invoke_chosen(0);
    let deadline = Instant::now() + Duration::from_secs(60);
    while g.get_client_label() != granularity::granularity_label(2) {
        assert!(Instant::now() < deadline, "the migration never finished");
        pump(Duration::from_millis(50));
    }
    assert!(from.join(file).is_file());
}

// leaf: audit-media-database-locations-granularity-offline
#[test]
fn an_offline_folder_is_regranularised_after_its_ready_check_and_scan() {
    use hydrus_gui_model::database_granularity as granularity;
    let rig = rig();
    let window = rig.open();
    let g = granularity_window(&rig, &window);

    // a backup folder in the granularity-2 layout
    let backup = tempfile::tempdir().unwrap();
    for prefix in hydrus_store::granularity::prefixes('f', 2) {
        std::fs::create_dir_all(hydrus_store::storage::prefix_dir(backup.path(), &prefix)).unwrap();
    }
    let file = "3ab4567.png";
    let old = hydrus_store::storage::prefix_dir(backup.path(), "f3a");
    std::fs::write(old.join(file), b"x").unwrap();

    // the ready check, then the folder picker; declining either does nothing
    let (title, message, yes, no) = granularity::folder_ready();
    g.invoke_action("folder 2to3".into());
    let question = hydrus_gui::granularity_window::question().unwrap();
    assert_eq!(question.get_window_title(), title);
    assert_eq!(question.get_message(), message);
    assert_eq!(question.get_no_label(), no);
    assert_eq!(
        question
            .get_choices()
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>(),
        [yes]
    );
    question.invoke_cancelled();
    assert!(rig.asked.borrow().is_empty());
    g.invoke_action("folder 2to3".into());
    hydrus_gui::granularity_window::question()
        .unwrap()
        .invoke_chosen(0);
    assert_eq!(rig.asked.borrow().as_slice(), [granularity::PICK_FOLDER]);

    // the scan of the chosen folder is reported before anything moves
    rig.pick(backup.path());
    g.invoke_action("folder 2to3".into());
    hydrus_gui::granularity_window::question()
        .unwrap()
        .invoke_chosen(0);
    let (title, message) =
        granularity::folder_check(hydrus_store::granularity::estimate(backup.path()), 2, 3);
    assert_eq!(title, "Looking good.");
    let question = hydrus_gui::granularity_window::question().unwrap();
    assert_eq!(question.get_window_title(), title);
    assert_eq!(question.get_message(), message);
    question.invoke_cancelled();
    pump(Duration::from_millis(300));
    assert!(old.join(file).is_file());

    // going ahead moves its files and says how it went; the client is untouched
    rig.pick(backup.path());
    g.invoke_action("folder 2to3".into());
    hydrus_gui::granularity_window::question()
        .unwrap()
        .invoke_chosen(0);
    hydrus_gui::granularity_window::question()
        .unwrap()
        .invoke_chosen(0);
    let deadline = Instant::now() + Duration::from_secs(60);
    while message_window().is_none_or(|m| !m.window().is_visible()) {
        assert!(Instant::now() < deadline, "the migration never reported");
        pump(Duration::from_millis(50));
    }
    let report = message_window().unwrap().get_message().to_string();
    assert!(report.starts_with("1 files moved in "), "{report}");
    assert!(report.contains("Your folder has been granularised to level 3."));
    message_window().unwrap().invoke_cancelled();
    assert!(old.join("b").join(file).is_file());
    assert_eq!(hydrus_store::granularity::estimate(backup.path()), Some(3));
    assert_eq!(g.get_client_label(), granularity::granularity_label(2));
}
