//! Tags > sibling/parent sync > review current sibling/parent sync, against the
//! reference's panel (`oracle/record_tag_sync_review.py`).
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_store::settings::BackgroundWork;
use slint::Model as _;

fn open_review(ui: &MainWindow) {
    let top = ui
        .get_menu_titles()
        .iter()
        .position(|r| r.label == "tags")
        .unwrap();
    ui.invoke_menu_title_pressed(i32::try_from(top).unwrap(), 0.0, 22.0);
    let pane = ui.get_menu_panes().row_data(0).unwrap();
    let row = pane.lines.iter().position(|r| r.label == "sync").unwrap();
    ui.invoke_menu_line_hovered(0, i32::try_from(row).unwrap(), 200.0, 22.0, 0.0);
    let lines: Vec<_> = ui
        .get_menu_panes()
        .row_data(1)
        .unwrap()
        .lines
        .iter()
        .collect();
    let at = lines
        .iter()
        .position(|l| l.label == "review current sibling/parent sync")
        .unwrap();
    ui.invoke_menu_line_clicked(1, i32::try_from(at).unwrap(), 0.0, 0.0, 0.0);
}

// leaf: audit-media-menu-tags-review-current-sibling-parent-sync
#[test]
fn the_review_window_reads_as_the_reference_s_panel_does() {
    let recorded = hydrus_testkit::fixture_json("tag_sync_review.json");
    let (_dirs, store) = crate::subscriptions::store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let _bound = bind(&ui, Pages::open(store.clone()).unwrap());
    open_review(&ui);
    let window = hydrus_gui::tag_sync_review_opened().unwrap();

    assert_eq!(window.get_message(), recorded["message"].as_str().unwrap());
    assert_eq!(window.get_use_listbook(), recorded["listbook"]);

    // The tabs are the reference's, in its order, opening on its default.
    let tabs = recorded["tabs"].as_array().unwrap();
    let names: Vec<String> = window
        .get_service_names()
        .iter()
        .map(|n| n.to_string())
        .collect();
    let expected: Vec<&str> = tabs.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names, expected);
    assert_eq!(window.get_service_index(), recorded["current_tab"]);
    for (i, tab) in tabs.iter().enumerate() {
        window.invoke_service_chosen(i32::try_from(i).unwrap());
        assert_eq!(window.get_service_index(), i32::try_from(i).unwrap());
        assert_eq!(
            window.get_summary(),
            tab["siblings_and_parents"].as_str().unwrap()
        );
        assert_eq!(
            window.get_progress(),
            tab["progress"].as_str().unwrap(),
            "{}",
            tab["name"]
        );
    }

    // The status line under each combination of the two sync switches, and its colour.
    for case in recorded["status_by_switches"].as_array().unwrap() {
        let (idle, active) = (
            case["idle"].as_bool().unwrap(),
            case["active"].as_bool().unwrap(),
        );
        store
            .write_and_refresh(move |ctx| {
                hydrus_store::settings::set(
                    ctx.conn(),
                    &BackgroundWork {
                        tag_display_during_idle: idle,
                        tag_display_during_active: active,
                        ..hydrus_store::settings::get::<BackgroundWork>(ctx.conn())?
                    },
                )
            })
            .unwrap();
        window.invoke_refresh_clicked();
        assert_eq!(
            window.get_status(),
            case["text"].as_str().unwrap(),
            "{case}"
        );
        assert_eq!(
            window.get_status_valid(),
            case["object_name"] == "HydrusValid"
        );
    }
    window.invoke_close_clicked();
}

// leaf: audit-media-menu-tags-review-current-sibling-parent-sync
#[test]
fn the_review_lists_repositories_after_local_services_as_the_reference_does() {
    let recorded = hydrus_testkit::fixture_json("tag_sync_review.json");
    let legacy = hydrus_testkit::legacy_fixture("repositories");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = hydrus_store::Store::open(dir.path()).unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let _bound = bind(&ui, Pages::open(store.clone()).unwrap());
    open_review(&ui);
    let window = hydrus_gui::tag_sync_review_opened().unwrap();
    let tabs = recorded["repositories"]["tabs"].as_array().unwrap();
    let names: Vec<String> = window
        .get_service_names()
        .iter()
        .map(|n| n.to_string())
        .collect();
    let expected: Vec<&str> = tabs.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names, expected);
    assert_eq!(
        window.get_service_index(),
        recorded["repositories"]["current_tab"]
    );
    for (i, tab) in tabs.iter().enumerate() {
        window.invoke_service_chosen(i32::try_from(i).unwrap());
        assert_eq!(
            window.get_progress(),
            tab["progress"].as_str().unwrap(),
            "{}",
            tab["name"]
        );
    }
    window.invoke_close_clicked();
}
