//! Database > how boned am I?, opened from the real menu, against
//! `oracle/record_how_boned.py`: the window loads its statistics on a worker
//! and shows the reference's files, views and duplicates tabs and its message,
//! for the default domain and after choosing "my files". (The statistics
//! themselves, with the clock held still, are also checked in the model test.)

use std::time::{Duration, Instant};

use hydrus_gui::{HowBonedWindow, MainWindow, Pages, bind, headless, how_boned_window};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use serde_json::Value;
use slint::ComponentHandle as _;
use slint::Model as _;

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_owned())
        .collect()
}

fn wait_loaded(window: &HowBonedWindow) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while window.get_loading() {
        assert!(Instant::now() < deadline, "statistics never arrived");
        slint::platform::update_timers_and_animations();
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn shown(window: &HowBonedWindow) -> (Vec<Vec<String>>, Vec<String>, Vec<String>, String) {
    let files = window.get_files();
    let rows = (0..files.row_count())
        .map(|i| {
            let cells = files.row_data(i).unwrap().cells;
            (0..cells.row_count())
                .map(|j| cells.row_data(j).unwrap().to_string())
                .collect()
        })
        .collect();
    let list = |m: slint::ModelRc<slint::SharedString>| {
        (0..m.row_count())
            .map(|i| m.row_data(i).unwrap().to_string())
            .collect::<Vec<_>>()
    };
    (
        rows,
        list(window.get_views()),
        list(window.get_duplicates()),
        window.get_bones_text().to_string(),
    )
}

fn assert_recorded(window: &HowBonedWindow, recorded: &Value) {
    let (rows, views, duplicates, bones) = shown(window);
    let recorded_rows: Vec<Vec<String>> = recorded["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(strings)
        .collect();
    assert_eq!(rows, recorded_rows);
    assert_eq!(views, strings(&recorded["views"]));
    assert_eq!(duplicates, strings(&recorded["duplicates"]));
    assert_eq!(bones, recorded["bones_text"].as_str().unwrap_or(""));
    assert_eq!(window.get_loading_text(), "");
}

// leaf: audit-media-menu-database-how-boned-am-i
#[test]
fn how_boned_opens_from_the_database_menu_and_shows_the_reference_s_statistics() {
    let fixture = hydrus_testkit::fixture_json("how_boned.json");
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let _bound = bind(&ui, Pages::open(store.clone()).unwrap());

    // database > how boned am I?
    let titles = ui.get_menu_titles();
    let database = (0..titles.row_count())
        .position(|i| titles.row_data(i).unwrap().label == "database")
        .unwrap();
    ui.invoke_menu_title_pressed(i32::try_from(database).unwrap(), 10.0, 22.0);
    let pane = ui.get_menu_panes().row_data(0).unwrap();
    let line = (0..pane.lines.row_count())
        .position(|i| pane.lines.row_data(i).unwrap().label == "how boned am I?")
        .expect("the database menu offers how boned am I?");
    ui.invoke_menu_line_clicked(0, i32::try_from(line).unwrap(), 0.0, 0.0, 0.0);

    let window = how_boned_window::opened().expect("the window opened");
    assert!(window.window().is_visible());
    wait_loaded(&window);
    assert_recorded(&window, &fixture["default"]);

    // choosing "my files" searches that domain instead
    let domains = window.get_domains();
    let my_files = (0..domains.row_count())
        .position(|i| domains.row_data(i).unwrap() == "my files")
        .expect("my files is offered");
    window.set_domain_index(i32::try_from(my_files).unwrap());
    window.invoke_domain_chosen(i32::try_from(my_files).unwrap());
    assert!(window.get_loading());
    wait_loaded(&window);
    assert_recorded(&window, &fixture["my_files"]);

    // stopping a search drops its result
    window.invoke_refresh();
    window.invoke_cancel_search();
    assert!(!window.get_loading());
    assert_eq!(window.get_loading_text(), "cancelled!");
    std::thread::sleep(Duration::from_millis(200));
    slint::platform::update_timers_and_animations();
    assert_eq!(window.get_loading_text(), "cancelled!");
}
