//! Saved title policy reaches actual current and future duplicates pages.
use hydrus_gui::{MainWindow, OptionsWindow, Pages, bind, headless};
use hydrus_store::{Store, duplicates_progress, settings, similar::SimilarFilesSettings};
use slint::{ComponentHandle as _, Model as _};
use std::sync::Arc;
const LABEL: &str = "Hide the \"x% done\" notification on preparation tab when >99% searched:";
fn duplicates(store: Arc<Store>) -> Pages {
    let mut pages = Pages::open(store).unwrap();
    pages.open_duplicates_with_context(
        hydrus_search::LocationContext::default(),
        hydrus_search::TagContext::default(),
        vec![],
        "duplicates",
    );
    pages
}
fn options(ui: &MainWindow, bound: &hydrus_gui::Bound) -> (OptionsWindow, i32) {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let row = ui
        .get_menu_panes()
        .row_data(0)
        .unwrap()
        .lines
        .iter()
        .position(|r| r.label == "options…")
        .unwrap();
    ui.invoke_menu_line_clicked(0, row as i32, 0.0, 0.0, 0.0);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    let page = window
        .get_pages()
        .iter()
        .position(|r| r.text == "duplicates")
        .unwrap();
    window.invoke_page_chosen(page as i32);
    let row = window
        .get_rows()
        .iter()
        .position(|r| r.label == LABEL)
        .unwrap();
    (window, row as i32)
}
fn counts(store: &Store, row: &serde_json::Value) {
    let counts = row["counts"].as_object().unwrap().clone();
    let distance = row["distance"].as_u64().unwrap() as u32;
    store
        .write_and_refresh(move |c| {
            let mut similar: SimilarFilesSettings = settings::get(c.conn())?;
            similar.search_distance = distance;
            settings::set(c.conn(), &similar)?;
            c.conn().execute("DELETE FROM similar_search_status", [])?;
            let mut insert = c.conn().prepare(
                "INSERT INTO similar_search_status(hash_id,searched_distance) VALUES(?,?)",
            )?;
            let mut id = 1i64;
            for (searched, n) in counts {
                let searched: i64 = searched.parse().unwrap();
                let searched = (searched >= 0).then_some(searched);
                for _ in 0..n.as_u64().unwrap() {
                    insert.execute(rusqlite::params![id, searched])?;
                    id += 1;
                }
            }
            Ok(())
        })
        .unwrap();
}
fn matches(ui: &MainWindow, row: &serde_json::Value) {
    let data = ui.get_duplicates();
    assert_eq!(data.preparation_name, row["page_name"].as_str().unwrap());
    assert_eq!(data.eligible, row["eligible"].as_str().unwrap());
    assert_eq!(data.searched, row["searched"].as_str().unwrap());
    assert_eq!(data.can_start, row["can_start"].as_bool().unwrap());
    let gauge: [u64; 2] = serde_json::from_value(row["gauge"].clone()).unwrap();
    assert!(
        (data.searched_fraction - gauge[0] as f32 / gauge[1].max(1) as f32).abs() < f32::EPSILON
    );
}
#[test]
fn actual_checkbox_relabels_visible_preparation_without_changing_work_and_future_pages_reopen_policy()
 {
    let fixture = hydrus_testkit::fixture_json("duplicates_progress_option.json");
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    counts(&store, &fixture["cases"][1]["publisher"][6]);
    let bound = bind(&ui, duplicates(store.clone()));
    matches(&ui, &fixture["cases"][1]["publisher"][6]);
    let (cancelled, row) = options(&ui, &bound);
    cancelled.invoke_check_toggled(row, false);
    assert!(
        store
            .read(duplicates_progress::load)
            .unwrap()
            .hide_caught_up
    );
    cancelled.invoke_cancel();
    cancelled.invoke_apply();
    assert!(
        store
            .read(duplicates_progress::load)
            .unwrap()
            .hide_caught_up
    );
    let (hidden, row) = options(&ui, &bound);
    hidden.hide().unwrap();
    hidden.invoke_check_toggled(row, false);
    hidden.invoke_apply();
    hidden.show().unwrap();
    hidden.invoke_apply();
    assert!(
        store
            .read(duplicates_progress::load)
            .unwrap()
            .hide_caught_up
    );
    for case in fixture["cases"].as_array().unwrap() {
        counts(&store, &case["publisher"][6]);
        ui.invoke_refresh_page();
        let before = ui.get_duplicates();
        let (edit, row) = options(&ui, &bound);
        edit.invoke_check_toggled(row, case["hide"].as_bool().unwrap());
        assert_eq!(
            ui.get_duplicates().preparation_name,
            before.preparation_name,
            "draft does not publish"
        );
        edit.invoke_apply();
        matches(&ui, &case["publisher"][6]);
        assert_eq!(ui.get_duplicates().searched, before.searched);
        assert_eq!(
            ui.get_duplicates().can_start,
            before.can_start,
            "display policy cannot alter search work"
        );
        assert_eq!(
            Store::open(store.dir())
                .unwrap()
                .read(duplicates_progress::load)
                .unwrap()
                .hide_caught_up,
            case["reopened"]
        );
        for row in case["publisher"].as_array().unwrap() {
            counts(&store, row);
            ui.invoke_refresh_page();
            matches(&ui, row);
        }
    }
    // Fresh binding uses saved policy; retired child cannot overwrite it.
    counts(&store, &fixture["cases"][1]["publisher"][6]);
    ui.invoke_refresh_page();
    let (old, row) = options(&ui, &bound);
    old.invoke_check_toggled(row, false);
    let successor = bind(&ui, duplicates(store.clone()));
    matches(&ui, &fixture["cases"][1]["publisher"][6]);
    old.show().unwrap();
    old.invoke_apply();
    assert!(
        store
            .read(duplicates_progress::load)
            .unwrap()
            .hide_caught_up
    );
    let native = windows.get(0).unwrap();
    let pixels = headless::render(&native, 1100, 700);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("duplicates-progress-policy-native.png"),
        &pixels,
        1100,
        700,
    )
    .unwrap();
    let (retired, row) = options(&ui, &successor);
    retired.invoke_check_toggled(row, false);
    store
        .write(|c| {
            let mut gui: settings::GuiSettings = settings::get(c.conn())?;
            gui.confirm_exit = false;
            settings::set(c.conn(), &gui)?;
            // This boundary tests completed exit, independently of due maintenance.
            let mut shutdown: hydrus_store::settings::ShutdownWork =
                hydrus_store::settings::get(c.conn())?;
            shutdown.action = 0;
            settings::set(c.conn(), &shutdown)
        })
        .unwrap();
    let _ = ui
        .window()
        .dispatch_event_with_result(slint::platform::WindowEvent::CloseRequested);
    assert!(
        !ui.window().is_visible(),
        "completed exit precedes retained callbacks"
    );
    retired.show().unwrap();
    retired.invoke_check_toggled(row, false);
    retired.invoke_apply();
    assert!(
        store
            .read(duplicates_progress::load)
            .unwrap()
            .hide_caught_up,
        "accepted close retires draft"
    );
}
